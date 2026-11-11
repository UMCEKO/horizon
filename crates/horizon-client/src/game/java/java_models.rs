// game-module-cfg: feature java-mods
//! Geometry the Java client bakes from a mod block entity's data (cable parts, inserted cells): the
//! replies are parsed here into `dynamic_models`, which the mesher draws from, and the sections remesh.

use super::*;
use horizon_assets::item_model::FoilType;
use horizon_assets::model::{BakedLayer, BakedQuadData, Direction};
use horizon_client::item_render::{ItemRenderState, LayerGeometry, LayerRender, RecordedQuad};
use horizon_client::{dynamic_models, dynamic_renders};
use horizon_java_host::wire::In;
use std::cell::RefCell;

/// A stack (its saved form) and an `ItemDisplayContext` ordinal.
pub(in crate::game) type ItemKey = (Vec<u8>, u8);

/// One piece of a recorded item: its quads in the frame of a pose, and how that pose moves.
struct ItemPiece {
    motion: dynamic_renders::Draw,
    foil: FoilType,
    quads: Arc<[RecordedQuad]>,
    extents: Arc<Vec<glam::Vec3>>,
}

/// `ItemStackRenderState` as the Java client filled it for a stack.
struct ItemRecording {
    uses_block_light: bool,
    oversized_in_gui: bool,
    animated: bool,
    pieces: Vec<ItemPiece>,
    /// Game time the recording arrived.
    since: i64,
}

/// Mod items only their own code can draw: which they are, what the Java client recorded of them, and what to ask it next.
#[derive(Default)]
struct ItemRecordings {
    external: std::collections::HashSet<String>,
    cache: HashMap<ItemKey, Option<Arc<ItemRecording>>>,
    /// Each item's last recording per context: a stack whose data just changed keeps drawing it until its own arrives.
    latest: HashMap<(String, u8), Arc<ItemRecording>>,
    queued: Vec<ItemKey>,
    decorated: std::collections::HashSet<String>,
    decorations: HashMap<Vec<u8>, Option<Arc<[u8]>>>,
    decorations_queued: Vec<Vec<u8>>,
}

thread_local! {
    static ITEMS: RefCell<ItemRecordings> = RefCell::default();
}

fn read_ids(input: &mut In) -> std::collections::HashSet<String> {
    let count = input.int().unwrap_or(0);
    (0..count)
        .map_while(|_| input.text().map(str::to_owned))
        .collect()
}

/// One atlas the Java client's resource reload stitched: its texture, size and sprites by the names the window draws them by.
struct JavaAtlas {
    texture: String,
    size: (i32, i32),
    sprites: Vec<String>,
}

/// What the Java client's first resource reload made for the window (`ClientResources.take`), taken as its boot ended.
struct ClientResources {
    /// Java state ids whose geometry the Java client bakes from block entity data.
    states: Vec<i32>,
    external: std::collections::HashSet<String>,
    decorated: std::collections::HashSet<String>,
    atlases: Vec<JavaAtlas>,
}

fn parse_resources(reply: &[u8]) -> Option<ClientResources> {
    let mut input = In(reply);
    let count = input.int()?;
    let states = (0..count).map(|_| input.int()).collect::<Option<_>>()?;
    let external = read_ids(&mut input);
    let decorated = read_ids(&mut input);
    let mut atlases = Vec::new();
    for _ in 0..input.int()? {
        input.text()?;
        let texture = input.text()?.to_owned();
        let size = (input.int()?, input.int()?);
        let count = input.int()?;
        let sprites = (0..count)
            .map(|_| input.text().map(str::to_owned))
            .collect::<Option<_>>()?;
        atlases.push(JavaAtlas {
            texture,
            size,
            sprites,
        });
    }
    Some(ClientResources {
        states,
        external,
        decorated,
        atlases,
    })
}

/// The boot's export, parsed once; None without a Java client.
fn client_resources() -> Option<&'static ClientResources> {
    static PARSED: std::sync::OnceLock<Option<ClientResources>> = std::sync::OnceLock::new();
    PARSED
        .get_or_init(|| {
            let reply = horizon_java_host::get()?.client_resources();
            if reply.is_empty() {
                return None;
            }
            let parsed = parse_resources(reply);
            if parsed.is_none() {
                log::warn!("Horizon: the Java client's resource export was malformed");
            }
            parsed
        })
        .as_ref()
}

/// Sprites of the Java client's atlases the window's atlas lacks, which Rust can't draw where Java names them.
fn missing_sprites(
    atlases: &[JavaAtlas],
    have: &HashMap<String, horizon_client::hud::Rect>,
) -> Vec<String> {
    atlases
        .iter()
        .flat_map(|atlas| &atlas.sprites)
        .filter(|name| !have.contains_key(horizon_client::item_render::sprite_name(name)))
        .cloned()
        .collect()
}

/// The queued stacks to record, as the request and its encoded form; None when there are none.
pub(in crate::game) fn java_items_request() -> Option<(Vec<ItemKey>, Vec<u8>)> {
    let keys = ITEMS.with_borrow_mut(|items| std::mem::take(&mut items.queued));
    if keys.is_empty() {
        return None;
    }
    let mut out = horizon_java_host::wire::Out::default();
    out.int(keys.len() as i32);
    for (nbt, context) in &keys {
        out.bytes(nbt);
        out.0.push(*context);
    }
    Some((keys, out.0))
}

/// The queued stacks whose decorations to record, as the request and its encoded form.
pub(in crate::game) fn java_decorations_request() -> Option<(Vec<Vec<u8>>, Vec<u8>)> {
    let keys = ITEMS.with_borrow_mut(|items| std::mem::take(&mut items.decorations_queued));
    if keys.is_empty() {
        return None;
    }
    let mut out = horizon_java_host::wire::Out::default();
    out.int(keys.len() as i32);
    for nbt in &keys {
        out.bytes(nbt);
    }
    Some((keys, out.0))
}

fn recorded_quad(input: &mut In) -> Option<RecordedQuad> {
    let texture = input.text()?.to_owned();
    let mut corners = [[0.0; 5]; 4];
    let mut color = 0;
    for corner in &mut corners {
        for value in corner.iter_mut() {
            *value = input.float()?;
        }
        color = input.int()? as u32;
    }
    let normal = [input.float()?, input.float()?, input.float()?];
    let translucent = input.bool()?;
    let cull = input.bool()?;
    Some(RecordedQuad {
        texture,
        corners,
        color,
        normal,
        translucent,
        cull,
    })
}

fn moves(op: &dynamic_renders::Op) -> bool {
    use dynamic_renders::{Op, Param};
    let varies = |p: &Param| !matches!(p, Param::Const(_));
    match op {
        Op::Translate(p) | Op::Scale(p) => p.iter().any(varies),
        Op::Rotate(_, p) => varies(p),
        Op::Quat(_) | Op::Matrix(_) => false,
    }
}

/// One stack's recording; `Some(None)` when its model failed in Java, None when the reply is cut short.
fn parse_recording(input: &mut In, since: i64) -> Option<Option<ItemRecording>> {
    let pieces = input.int()?;
    if pieces < 0 {
        return Some(None);
    }
    let uses_block_light = input.bool()?;
    let oversized_in_gui = input.bool()?;
    let mut list = Vec::new();
    for _ in 0..pieces {
        let ops = (0..input.int()?)
            .map(|_| op(input))
            .collect::<Option<Vec<_>>>()?;
        let foil = match input.byte()? {
            0 => FoilType::None,
            1 => FoilType::Standard,
            _ => FoilType::Special,
        };
        let quads = (0..input.int()?)
            .map(|_| recorded_quad(input))
            .collect::<Option<Vec<_>>>()?;
        let extents = quads
            .iter()
            .flat_map(|q| q.corners.map(|c| glam::Vec3::new(c[0], c[1], c[2])))
            .collect();
        list.push(ItemPiece {
            motion: dynamic_renders::Draw {
                translucent: false,
                ops,
                quads: Vec::new(),
                item: None,
            },
            foil,
            quads: quads.into(),
            extents: Arc::new(extents),
        });
    }
    Some(Some(ItemRecording {
        uses_block_light,
        oversized_in_gui,
        animated: list
            .iter()
            .any(|p: &ItemPiece| p.motion.ops.iter().any(moves)),
        pieces: list,
        since,
    }))
}

/// Stores the recordings answering `keys`.
pub(in crate::game) fn java_items_apply(keys: Vec<ItemKey>, reply: &[u8], now: i64) {
    let mut input = In(reply);
    if input.int().is_none() {
        return;
    }
    let mut answers = Vec::new();
    for _ in 0..keys.len() {
        match parse_recording(&mut input, now) {
            Some(answer) => answers.push(answer.map(Arc::new)),
            None => {
                log::warn!("Horizon: a malformed Java items reply was dropped");
                return;
            }
        }
    }
    ITEMS.with_borrow_mut(|items| {
        if items.cache.len() > 4096 {
            items.cache.clear();
        }
        for ((nbt, context), answer) in keys.into_iter().zip(answers) {
            if let Some(recording) = &answer
                && let Some(id) = item_id(&nbt)
            {
                items.latest.insert((id, context), Arc::clone(recording));
            }
            items.cache.insert((nbt, context), answer);
        }
    });
}

/// The item id of a stack's saved form.
fn item_id(nbt: &[u8]) -> Option<String> {
    let tag = super::java_mods::read_tag(nbt)?;
    horizon_client::save::item_from_nbt(&tag).map(|stack| stack.item.to_string())
}

/// Stores the decorations answering `keys`.
pub(in crate::game) fn java_decorations_apply(keys: Vec<Vec<u8>>, reply: &[u8]) {
    let mut input = In(reply);
    if input.int().is_none() {
        return;
    }
    let mut answers = Vec::new();
    for _ in 0..keys.len() {
        let Some(length) = input.int() else {
            log::warn!("Horizon: a malformed Java decorations reply was dropped");
            return;
        };
        let Ok(length) = usize::try_from(length) else {
            answers.push(None);
            continue;
        };
        if input.0.len() < length {
            log::warn!("Horizon: a malformed Java decorations reply was dropped");
            return;
        }
        let (frame, rest) = input.0.split_at(length);
        input.0 = rest;
        answers.push(Some(Arc::<[u8]>::from(frame)));
    }
    ITEMS.with_borrow_mut(|items| {
        if items.decorations.len() > 4096 {
            items.decorations.clear();
        }
        items.decorations.extend(keys.into_iter().zip(answers));
    });
}

/// A stack's saved form with its count taken away: what its drawing can depend on besides the count.
fn stack_key(stack: &ItemStack) -> Option<Vec<u8>> {
    let mut one = stack.clone();
    one.count = 1;
    super::java_mods::write_tag(horizon_items::item_nbt::item_to_nbt(&one))
}

/// What a mod's decorators recorded over this stack, once the Java client answered; asks for it otherwise.
pub(in crate::game) fn java_decorations(stack: &ItemStack) -> Option<Arc<[u8]>> {
    if !ITEMS.with_borrow(|items| items.decorated.contains(&*stack.item)) {
        return None;
    }
    // A decorator sees the whole stack, count included.
    let key = super::java_mods::write_tag(horizon_items::item_nbt::item_to_nbt(stack))?;
    ITEMS.with_borrow_mut(|items| match items.decorations.get(&key) {
        Some(answer) => answer.clone(),
        None => {
            if !items.decorations_queued.contains(&key) {
                items.decorations_queued.push(key);
            }
            None
        }
    })
}

struct Entry {
    pos: [i32; 3],
    /// The Java block state the geometry was baked for, -1 when none.
    state: i32,
    key: i32,
    quads: Option<(Vec<BakedQuadData>, Vec<i32>)>,
}

/// One quad and the tint color the block's code resolved for it (-1 for none).
fn quad(input: &mut In) -> Option<(BakedQuadData, i32)> {
    let mut vertices = [[0.0; 5]; 4];
    for value in vertices.iter_mut().flatten() {
        *value = input.float()?;
    }
    let sprite = input.text()?.to_owned();
    let face = *Direction::ALL.get(usize::from(input.byte()?))?;
    let cull = Direction::ALL.get(usize::from(input.byte()?)).copied();
    let tint = input.int()?;
    let color = input.int()?;
    let shade = input.bool()?;
    let emission = input.byte()?.min(15);
    let layer = match input.byte()? {
        0 => BakedLayer::Solid,
        1 => BakedLayer::Cutout,
        2 => BakedLayer::CutoutMipped,
        3 => BakedLayer::Translucent,
        4 => BakedLayer::Tripwire,
        _ => return None,
    };
    Some((
        BakedQuadData {
            vertices,
            sprite,
            face,
            cull,
            tint,
            shade,
            emission,
            layer,
        },
        color,
    ))
}

/// The models reply: entries, or None when it is cut short or malformed.
fn parse_models(reply: &[u8]) -> Option<Vec<Entry>> {
    let mut input = In(reply);
    let count = usize::try_from(input.int()?).ok()?;
    let mut entries = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let pos = BlockPos::unpack(input.long()?);
        let state = input.int()?;
        let key = input.int()?;
        let quads = if input.bool()? {
            let quads = usize::try_from(input.int()?).ok()?;
            let mut list = Vec::with_capacity(quads.min(4096));
            let mut colors = Vec::with_capacity(quads.min(4096));
            for _ in 0..quads {
                let (quad, color) = quad(&mut input)?;
                list.push(quad);
                colors.push(color);
            }
            Some((list, colors))
        } else {
            None
        };
        entries.push(Entry {
            pos: [pos.x, pos.y, pos.z],
            state,
            key,
            quads,
        });
    }
    Some(entries)
}

/// Whether Java state `state` is the block the window holds now; a state with no Horizon one can't be told apart, and -1 names none.
fn recorded_for(state: i32, native: BlockStateId, to_horizon: &[Option<BlockStateId>]) -> bool {
    match usize::try_from(state)
        .ok()
        .and_then(|id| to_horizon.get(id))
    {
        Some(Some(recorded)) => *recorded == native,
        _ => true,
    }
}

/// Java state ids as Horizon's; those with no Horizon state are skipped.
fn horizon_states(java: &[i32], to_horizon: &[Option<BlockStateId>]) -> Vec<BlockStateId> {
    java.iter()
        .filter_map(|&id| to_horizon.get(usize::try_from(id).ok()?).copied().flatten())
        .collect()
}

fn param(input: &mut In) -> Option<dynamic_renders::Param> {
    use dynamic_renders::Param;
    Some(match input.float()? as i32 {
        0 => Param::Const(input.float()?),
        1 => Param::Linear(input.float()?, input.float()?),
        2 => Param::Sine(
            input.float()?,
            input.float()?,
            input.float()?,
            input.float()?,
        ),
        _ => return None,
    })
}

fn op(input: &mut In) -> Option<dynamic_renders::Op> {
    use dynamic_renders::Op;
    fn floats(input: &mut In, n: usize) -> Option<Vec<f32>> {
        (0..n).map(|_| input.float()).collect()
    }
    Some(match input.byte()? {
        b'T' => Op::Translate([param(input)?, param(input)?, param(input)?]),
        b'S' => Op::Scale([param(input)?, param(input)?, param(input)?]),
        b'R' => {
            let axis = floats(input, 3)?;
            Op::Rotate(
                glam::Vec3::new(axis[0], axis[1], axis[2]).normalize_or(glam::Vec3::Y),
                param(input)?,
            )
        }
        b'Q' => {
            let q = floats(input, 4)?;
            Op::Quat(glam::Quat::from_xyzw(q[0], q[1], q[2], q[3]).normalize())
        }
        b'M' => Op::Matrix(glam::Mat4::from_cols_slice(&floats(input, 16)?)),
        _ => return None,
    })
}

/// The renders reply: each position's pieces, or None there when nothing draws now.
type Recorded = ([i32; 3], i32, Option<Vec<dynamic_renders::Draw>>);

fn parse_renders(reply: &[u8]) -> Option<Vec<Recorded>> {
    let mut input = In(reply);
    let count = usize::try_from(input.int()?).ok()?;
    let mut entries = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let pos = BlockPos::unpack(input.long()?);
        let pos = [pos.x, pos.y, pos.z];
        let state = input.int()?;
        let draws = input.int()?;
        if draws < 0 {
            entries.push((pos, state, None));
            continue;
        }
        let mut list = Vec::new();
        for _ in 0..draws {
            let translucent = input.bool()?;
            let ops = (0..input.int()?)
                .map(|_| op(&mut input))
                .collect::<Option<Vec<_>>>()?;
            let quads = (0..input.int()?)
                .map(|_| {
                    let texture = input.text()?.to_owned();
                    let mut corners = [[0.0; 5]; 4];
                    let (mut color, mut light) = ([0; 4], [0; 4]);
                    for k in 0..4 {
                        for value in &mut corners[k] {
                            *value = input.float()?;
                        }
                        color[k] = input.int()? as u32;
                        light[k] = input.int()? as u32;
                    }
                    Some(dynamic_renders::Quad {
                        texture,
                        corners,
                        color,
                        light,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            let item = if input.bool()? {
                let tag = super::java_mods::read_tag(input.bytes()?);
                let (context, seed) = (input.byte()?, input.int()?);
                tag.and_then(|tag| horizon_client::save::item_from_nbt(&tag))
                    .map(|stack| dynamic_renders::Item {
                        stack,
                        context,
                        seed,
                    })
            } else {
                None
            };
            list.push(dynamic_renders::Draw {
                translucent,
                ops,
                quads,
                item,
            });
        }
        entries.push((pos, state, Some(list)));
    }
    Some(entries)
}

impl Game<'_> {
    /// `ItemModelResolver.updateForTopItem` of a mod's item that only its code can draw: what the Java client recorded, with the pieces where they are now; nothing until a first recording arrives.
    pub(in crate::game) fn java_item_state(
        &self,
        stack: &ItemStack,
        display: horizon_assets::item_model::DisplayContext,
        state: &mut ItemRenderState,
    ) {
        if !ITEMS.with_borrow(|items| items.external.contains(&*stack.item)) {
            return;
        }
        state.layers.clear();
        let Some(nbt) = stack_key(stack) else {
            return;
        };
        let Some(context) = horizon_assets::item_model::DisplayContext::ALL
            .iter()
            .position(|c| *c == display)
            .map(|c| c as u8)
        else {
            return;
        };
        let key = (nbt, context);
        let recording = ITEMS.with_borrow_mut(|items| match items.cache.get(&key) {
            Some(answer) => answer.clone(),
            None => {
                if !items.queued.contains(&key) {
                    items.queued.push(key);
                }
                items
                    .latest
                    .get(&(stack.item.to_string(), context))
                    .cloned()
            }
        });
        let Some(recording) = recording else {
            return;
        };
        let ticks = (self.world.game_time - recording.since) as f32 + self.clock.fraction() as f32;
        state.oversized_in_gui = recording.oversized_in_gui;
        state.animated = recording.animated;
        state.layers = recording
            .pieces
            .iter()
            .map(|piece| LayerRender {
                geometry: LayerGeometry::Recorded(Arc::clone(&piece.quads)),
                tints: Vec::new(),
                foil: piece.foil,
                matrix: piece.motion.matrix(ticks),
                particle: String::new(),
                uses_block_light: recording.uses_block_light,
                extents: Arc::clone(&piece.extents),
            })
            .collect();
    }

    /// Takes a renders reply: what mods' block entity renderers draw at each changed position.
    pub(in crate::game) fn java_renders_apply(&mut self, reply: &[u8]) {
        let Some(entries) = parse_renders(reply) else {
            log::warn!("Horizon: a malformed Java renders reply was dropped");
            return;
        };
        for (pos, state, draws) in entries {
            if !self.recorded_for_here(pos, state) {
                continue;
            }
            match draws {
                Some(draws) => dynamic_renders::set(pos, draws, self.world.game_time),
                None => dynamic_renders::remove(pos),
            }
        }
    }

    /// Whether output the Java client recorded for `state` still belongs to the block this window holds at `pos`: its copy hears of a break only from the server's update, after the window's own prediction removed the block.
    fn recorded_for_here(&self, pos: [i32; 3], state: i32) -> bool {
        let native = self.world.block(pos[0], pos[1], pos[2]);
        horizon_java_host::bridge::STATES
            .get()
            .is_none_or(|map| recorded_for(state, native, &map.to_horizon))
    }

    /// Takes a models reply: stores the geometry and remeshes the sections whose blocks changed.
    pub(in crate::game) fn java_models_apply(&mut self, reply: &[u8]) {
        let Some(entries) = parse_models(reply) else {
            log::warn!("Horizon: a malformed Java models reply was dropped");
            return;
        };
        let mut changed = Vec::new();
        for Entry {
            pos,
            state,
            key,
            quads,
        } in entries
        {
            if key < 0 {
                if dynamic_models::place(pos, None) {
                    changed.push(pos);
                }
                continue;
            }
            if let Some((quads, colors)) = quads
                && dynamic_models::put_geometry(key, quads.into(), colors.into())
            {
                changed.extend(dynamic_models::positions_of(key));
            }
            if self.recorded_for_here(pos, state) && dynamic_models::place(pos, Some(key)) {
                changed.push(pos);
            }
        }
        for [x, y, z] in changed {
            self.world.mark_block_dirty(x, y, z);
        }
    }

    /// As the window joins, before its chunks: the states drawn from block entity data, and columns holding them wait for the Java client's answer.
    pub(in crate::game) fn java_client_arm(&mut self) {
        let Some(resources) = client_resources() else {
            return;
        };
        let Some(map) = horizon_java_host::bridge::STATES.get() else {
            return;
        };
        dynamic_models::set_dynamic_states(horizon_states(&resources.states, &map.to_horizon));
        dynamic_models::set_answering(true);
    }

    /// As the Java client joins: the block states whose geometry it bakes and the items it draws, from what its boot's resource reload exported.
    pub(in crate::game) fn java_client_resources(&mut self) {
        let Some(resources) = client_resources() else {
            return;
        };
        ITEMS.with_borrow_mut(|items| {
            items.external = resources.external.clone();
            items.decorated = resources.decorated.clone();
        });
        static CHECKED: std::sync::Once = std::sync::Once::new();
        CHECKED.call_once(|| {
            let sprites: usize = resources.atlases.iter().map(|a| a.sprites.len()).sum();
            let missing = missing_sprites(&resources.atlases, &self.gui.atlas.sprites);
            log::info!(
                "Horizon: the Java client stitched {} atlases ({sprites} sprites): {}",
                resources.atlases.len(),
                resources
                    .atlases
                    .iter()
                    .map(|a| format!("{} {}x{} {}", a.texture, a.size.0, a.size.1, a.sprites.len()))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            if !missing.is_empty() {
                log::warn!(
                    "Horizon: {} sprites of the Java client's atlases aren't in the window's atlas, so what Java draws with them is left out: {}",
                    missing.len(),
                    missing.iter().take(20).cloned().collect::<Vec<_>>().join(", ")
                );
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_java_host::wire::Out;

    fn write_quad(out: &mut Out, sprite: &str, cull: u8) {
        for v in 0..20 {
            out.float(v as f32 / 4.0);
        }
        out.text(sprite);
        out.0.extend([1, cull]);
        out.int(0x00ff_00ff);
        out.int(0x7f12_3456);
        out.bool(false);
        out.0.extend([7, 3]);
    }

    #[test]
    fn output_recorded_for_a_state_the_window_no_longer_holds_is_ignored() {
        let (tank, air) = (BlockStateId(40), BlockStateId(0));
        let map = [Some(air), None, Some(tank)];
        assert!(recorded_for(2, tank, &map));
        assert!(!recorded_for(2, air, &map));
        assert!(recorded_for(1, air, &map));
        assert!(recorded_for(-1, air, &map));
        assert!(recorded_for(9, air, &map));
    }

    #[test]
    fn two_entries_share_one_key_and_one_quad_list() {
        let mut out = Out::default();
        out.int(3);
        out.long(BlockPos::new(-3, 64, 9).pack());
        out.int(7);
        out.int(5);
        out.bool(true);
        out.int(2);
        write_quad(&mut out, "ae2:block/cable", 6);
        write_quad(&mut out, "ae2:block/cable_glass", 4);
        out.long(BlockPos::new(100, -2, -50).pack());
        out.int(7);
        out.int(5);
        out.bool(false);
        out.long(BlockPos::new(1, 2, 3).pack());
        out.int(-1);
        out.int(-1);
        out.bool(false);

        let entries = parse_models(&out.0).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].pos, [-3, 64, 9]);
        assert_eq!((entries[0].key, entries[1].key, entries[2].key), (5, 5, -1));
        assert_eq!(entries[1].pos, [100, -2, -50]);
        assert!(entries[1].quads.is_none() && entries[2].quads.is_none());
        let (quads, colors) = entries[0].quads.as_ref().unwrap();
        assert_eq!(quads.len(), 2);
        assert_eq!(colors, &[0x7f12_3456; 2]);
        assert_eq!(quads[0].vertices[3][4], 19.0 / 4.0);
        assert_eq!(quads[0].sprite, "ae2:block/cable");
        assert_eq!((quads[0].face, quads[0].cull), (Direction::Up, None));
        assert_eq!(quads[1].cull, Some(Direction::West));
        assert_eq!(quads[0].tint, 0x00ff_00ff);
        assert!(!quads[0].shade);
        assert_eq!(
            (quads[0].emission, quads[0].layer),
            (7, BakedLayer::Translucent)
        );

        assert!(parse_models(&out.0[..out.0.len() - 1]).is_none());
        assert!(parse_models(&[]).is_none());
    }

    #[test]
    fn a_recording_keeps_its_lighting_foil_and_motion() {
        let mut out = Out::default();
        out.int(1);
        out.bool(true);
        out.bool(false);
        out.int(1);
        out.0.push(b'R');
        for v in [0.0, 0.0, 1.0] {
            out.float(v);
        }
        for v in [1.0, 0.5, 0.25] {
            out.float(v);
        }
        out.0.push(1);
        out.int(1);
        out.text("t:a");
        for corner in 0..4 {
            for v in 0..5 {
                out.float((corner * v) as f32);
            }
            out.int(-1);
        }
        for v in [0.0, 1.0, 0.0] {
            out.float(v);
        }
        out.bool(false);
        out.bool(true);

        let recording = parse_recording(&mut In(&out.0), 7).unwrap().unwrap();
        assert!(recording.uses_block_light && !recording.oversized_in_gui && recording.animated);
        assert_eq!(recording.since, 7);
        let piece = &recording.pieces[0];
        assert_eq!(piece.foil, FoilType::Standard);
        assert_eq!(piece.quads[0].normal, [0.0, 1.0, 0.0]);
        assert_eq!(piece.extents.len(), 4);
        let turned = piece.motion.matrix(2.0).transform_vector3(glam::Vec3::X);
        assert!((turned - glam::Vec3::new(1f32.cos(), 1f32.sin(), 0.0)).length() < 1e-5);

        let mut failed = Out::default();
        failed.int(-1);
        assert!(parse_recording(&mut In(&failed.0), 0).unwrap().is_none());
        assert!(parse_recording(&mut In(&out.0[..out.0.len() - 1]), 0).is_none());
    }

    #[test]
    fn states_map_to_horizon_ids_and_skip_unmapped_ones() {
        let map = [None, Some(BlockStateId(40)), None];
        assert_eq!(horizon_states(&[1, 2, 9, -1], &map), vec![BlockStateId(40)]);
    }

    #[test]
    fn the_boot_export_reads_back_and_names_the_sprites_the_window_lacks() {
        let mut out = Out::default();
        out.int(2);
        out.int(7);
        out.int(8);
        out.int(1);
        out.text("mekanism:robit");
        out.int(1);
        out.text("mekanism:energy_cube");
        out.int(1);
        out.text("mekanism:entity/robit");
        out.text("mekanism:textures/atlas/robit.png");
        out.int(512);
        out.int(256);
        out.int(2);
        out.text("mekanism:entity/robit/robit");
        out.text("minecraft:block/stone");
        let parsed = parse_resources(&out.0).unwrap();
        assert_eq!(parsed.states, vec![7, 8]);
        assert!(parsed.external.contains("mekanism:robit"));
        assert!(parsed.decorated.contains("mekanism:energy_cube"));
        assert_eq!(parsed.atlases[0].size, (512, 256));
        let rect = horizon_client::hud::Rect {
            u0: 0.0,
            v0: 0.0,
            u1: 1.0,
            v1: 1.0,
            w: 16,
            h: 16,
        };
        let have = HashMap::from([("block/stone".to_owned(), rect)]);
        assert_eq!(
            missing_sprites(&parsed.atlases, &have),
            vec!["mekanism:entity/robit/robit".to_owned()]
        );
        assert!(parse_resources(&out.0[..out.0.len() - 3]).is_none());
    }
}
