// game-module-cfg: feature java-mods
//! The Java client's level reads the window's own chunks, the data the server sent this client: the window publishes them as shared
//! columns, which bridged natives read on the Java client's thread without copying a chunk.
#![allow(dead_code)]

use super::*;
use horizon_client::chunk::Chunk;
use horizon_client::chunk_map::Touched;
use horizon_client::fast_hash::{FxHashMap, FxHashSet};
use std::sync::{Arc, Mutex, RwLock};

const AIR: u8 = 1;
const SOLID: u8 = 2;
const FLUID: u8 = 4;
const LEAVES: u8 = 8;

/// The window's chunks as of its last publish, with what reading them needs from the game's tables.
struct View {
    chunks: FxHashMap<(i32, i32), Arc<Chunk>>,
    min_y: i32,
    air: BlockStateId,
    tables: Arc<Tables>,
}

/// Per state, what a heightmap counts; per biome index, its name.
struct Tables {
    flags: Vec<u8>,
    biomes: Vec<String>,
}

static VIEW: RwLock<Option<View>> = RwLock::new(None);
static TABLES: Mutex<Option<Arc<Tables>>> = Mutex::new(None);
/// The columns the Java client has been told arrived and not yet left.
static ANNOUNCED: Mutex<Option<FxHashSet<(i32, i32)>>> = Mutex::new(None);
/// The columns written since the Java client was last told of arrivals and drops.
static PENDING: std::sync::LazyLock<Mutex<FxHashSet<(i32, i32)>>> =
    std::sync::LazyLock::new(Default::default);

fn view() -> Option<std::sync::RwLockReadGuard<'static, Option<View>>> {
    VIEW.read().ok().filter(|v| v.is_some())
}

impl View {
    fn chunk(&self, x: i32, z: i32) -> Option<&Arc<Chunk>> {
        self.chunks.get(&(x >> 4, z >> 4))
    }

    fn flags(&self, state: u32) -> u8 {
        self.tables.flags.get(state as usize).copied().unwrap_or(0)
    }

    /// `Level.getHeight`: one above the highest block the heightmap counts, or the level's bottom outside its chunks.
    fn height(&self, kind: &str, x: i32, z: i32) -> i32 {
        let Some(chunk) = self.chunk(x, z) else {
            return self.min_y;
        };
        let (cx, cz) = (x & 15, z & 15);
        match kind {
            "world_surface" | "world_surface_wg" => {
                chunk.world_surface(cx, cz, |v| self.flags(v) & AIR != 0)
            }
            "motion_blocking" => {
                chunk.motion_blocking(cx, cz, |v| self.flags(v) & (SOLID | FLUID) != 0)
            }
            _ => {
                let counts = |v: u32| {
                    let flags = self.flags(v);
                    match kind {
                        "ocean_floor" | "ocean_floor_wg" => flags & SOLID != 0,
                        _ => flags & (SOLID | FLUID) != 0 && flags & LEAVES == 0,
                    }
                };
                chunk
                    .highest(self.air.0, |y| counts(chunk.get(cx, y, cz)))
                    .map_or(chunk.min_y(), |y| y + 1)
            }
        }
    }
}

impl Game<'_> {
    fn java_client_tables(&self) -> Arc<Tables> {
        let mut held = TABLES.lock().unwrap();
        let facts = self.res.facts();
        if let Some(tables) = held
            .as_ref()
            .filter(|t| t.flags.len() == facts.states.len())
        {
            return tables.clone();
        }
        let flags = (0..facts.states.len() as u32)
            .map(|i| {
                let state = BlockStateId(i);
                let fact = facts.get(state);
                let leaves = self
                    .pack
                    .assets
                    .registry
                    .state(state)
                    .is_some_and(|s| s.name.ends_with("_leaves"));
                (u8::from(state == facts.air) * AIR)
                    | (u8::from(fact.legacy_solid) * SOLID)
                    | (u8::from(fact.fluid != horizon_client::blocks::Fluid::None) * FLUID)
                    | (u8::from(leaves) * LEAVES)
            })
            .collect();
        let biomes = self
            .res
            .biomes()
            .list
            .iter()
            .map(|b| b.name.clone())
            .collect();
        let tables = Arc::new(Tables { flags, biomes });
        *held = Some(tables.clone());
        tables
    }

    /// Brings the Java client's shared columns up to the window's chunks: only the columns written since the last call are re-read.
    pub(in crate::game) fn java_publish_client_world(&mut self) {
        let touched = self.world.chunks.take_touched();
        if matches!(touched, Touched::Fresh) {
            *ANNOUNCED.lock().unwrap() = None;
        }
        let fresh = VIEW.read().unwrap().is_none();
        let keys = match touched {
            Touched::Keys(keys) if !fresh => {
                if keys.is_empty() {
                    return;
                }
                keys
            }
            _ => {
                let tables = self.java_client_tables();
                let chunks = self
                    .world
                    .chunks
                    .iter()
                    .map(|(k, c)| (*k, c.clone()))
                    .collect();
                let mut pending = PENDING.lock().unwrap();
                pending.extend(self.world.chunks.keys().copied());
                if let Some(old) = VIEW.read().unwrap().as_ref() {
                    pending.extend(old.chunks.keys().copied());
                }
                drop(pending);
                *VIEW.write().unwrap() = Some(View {
                    chunks,
                    min_y: self.world.bottom_y(),
                    air: self.world.air,
                    tables,
                });
                return;
            }
        };
        let mut guard = VIEW.write().unwrap();
        let Some(view) = guard.as_mut() else { return };
        let mut pending = PENDING.lock().unwrap();
        for key in keys {
            match self.world.chunks.get(&key) {
                Some(chunk) => {
                    view.chunks.insert(key, chunk.clone());
                }
                None => {
                    view.chunks.remove(&key);
                }
            }
            pending.insert(key);
        }
    }

    /// The columns that left and arrived since the Java client was last told, as `ClientChunkCache.drop` and `replaceWithPacketData` would hear of them.
    pub(in crate::game) fn java_client_chunk_changes(&self) -> (Vec<(i32, i32)>, Vec<(i32, i32)>) {
        let mut announced = ANNOUNCED.lock().unwrap();
        let announced = announced.get_or_insert_with(FxHashSet::default);
        let (mut left, mut arrived) = (Vec::new(), Vec::new());
        for key in PENDING.lock().unwrap().drain() {
            match (
                self.world.chunks.contains_key(&key),
                announced.contains(&key),
            ) {
                (true, false) => {
                    announced.insert(key);
                    arrived.push(key);
                }
                (false, true) => {
                    announced.remove(&key);
                    left.push(key);
                }
                _ => {}
            }
        }
        (left, arrived)
    }
}

/// The Java client left its level: it reads nothing until the window joins another.
pub(in crate::game) fn java_forget_client_world() {
    *VIEW.write().unwrap() = None;
    *ANNOUNCED.lock().unwrap() = None;
    PENDING.lock().unwrap().clear();
}

horizon_java_host::java_bridges! { table for () {
    "dev/horizonlab/javamods/boot/ClientWorld".state
    "(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/state/BlockState;" =>
    fn client_world_state(_game, pos: Pos) -> BlockStateId {
        let guard = view();
        let Some(view) = guard.as_deref().and_then(Option::as_ref) else { return BlockStateId(0) };
        view.chunk(pos.0.x, pos.0.z)
            .map_or(view.air, |chunk| chunk.block(pos.0.x & 15, pos.0.y, pos.0.z & 15))
    }

    "dev/horizonlab/javamods/boot/ClientWorld".height
    "(Lnet/minecraft/world/level/levelgen/Heightmap$Types;II)I" =>
    fn client_world_height(_game, kind: Text, x: i32, z: i32) -> i32 {
        view().map_or(0, |v| v.as_ref().map_or(0, |view| view.height(&kind.0, x, z)))
    }

    "dev/horizonlab/javamods/boot/ClientWorld".brightness
    "(Lnet/minecraft/world/level/LightLayer;Lnet/minecraft/core/BlockPos;)I" =>
    fn client_world_brightness(_game, layer: i32, pos: Pos) -> i32 {
        let light = view()
            .and_then(|v| v.as_ref()?.chunk(pos.0.x, pos.0.z).map(|c| c.light(pos.0.x & 15, pos.0.y, pos.0.z & 15)))
            .map_or(0xF0, i32::from);
        if layer == 0 { light >> 4 } else { light & 15 }
    }

    "dev/horizonlab/javamods/boot/ClientWorld".rawBrightness
    "(Lnet/minecraft/core/BlockPos;I)I" =>
    fn client_world_raw_brightness(_game, pos: Pos, darken: i32) -> i32 {
        let light = view()
            .and_then(|v| v.as_ref()?.chunk(pos.0.x, pos.0.z).map(|c| c.light(pos.0.x & 15, pos.0.y, pos.0.z & 15)))
            .map_or(0xF0, i32::from);
        ((light >> 4) - darken).max(light & 15)
    }

    "dev/horizonlab/javamods/boot/ClientWorld".biome
    "(III)Lnet/minecraft/core/Holder;" =>
    fn client_world_biome(_game, x: i32, y: i32, z: i32) -> Text {
        let name = view()
            .and_then(|v| {
                let view = v.as_ref()?;
                let (bx, by, bz) = (x << 2, y << 2, z << 2);
                let index = view.chunk(bx, bz)?.biome(bx, by, bz);
                view.tables.biomes.get(usize::from(index)).cloned()
            })
            .unwrap_or_else(|| "minecraft:plains".to_owned());
        Text(format!("worldgen/biome\t{name}"))
    }
}}
