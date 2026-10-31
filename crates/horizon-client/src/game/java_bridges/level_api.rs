//! What `Level` and `ServerLevel` answer that Java has no data for.

use super::*;
use horizon_client::collisions::{SceneCollisions, no_collision_empty, no_collision_or_liquid};
use horizon_client::mobs::spawning::MOON_BRIGHTNESS;
use horizon_core::geometry::{Aabb, Vec3};
use horizon_core::ticks::{ScheduledTick, TickPriority, TickTypeId};
use horizon_java_host::bridge::{Bounds, Doubles, Longs};
use horizon_world::fluid::FluidKind;
use horizon_world::level::BlockEventData;

impl Game<'_> {
    /// The biome of the block column as `Flat.toHolder` reads it; plains where nothing is loaded.
    fn java_biome(&self, x: i32, y: i32, z: i32) -> Text {
        let name = self
            .world
            .chunks
            .get(&(x >> 4, z >> 4))
            .map_or("minecraft:plains", |chunk| {
                self.res.biomes().get(chunk.biome(x, y, z)).name.as_str()
            });
        Text(format!("worldgen/biome\t{name}"))
    }

    fn java_moon_brightness(&self) -> f32 {
        let phase = self.world.day_time.div_euclid(24000).rem_euclid(8);
        MOON_BRIGHTNESS[phase as usize]
    }

    fn java_schedule_fluid(&mut self, pos: BlockPos, fluid: &str, delay: i32, priority: i32) {
        let Some(kind) = FluidKind::from_name(fluid) else {
            return;
        };
        self.with_level(|_, level, ticks| {
            let tick = ScheduledTick {
                kind: TickTypeId(kind as u32),
                pos,
                trigger_tick: level.game_time().wrapping_add(i64::from(delay)),
                priority: TickPriority::by_value(priority),
                sub_tick_order: ticks.next_sub_tick(),
            };
            ticks.fluids.schedule(tick);
        });
    }
}

horizon_java_host::java_bridges! { table for Game<'static> {
    "net/minecraft/world/level/LevelAccessor".hasChunk
    "(II)Z" if "net/minecraft/server/level/ServerLevel" =>
    fn level_has_chunk(game, x: i32, z: i32) -> bool {
        game.world.chunks.contains_key(&(x, z))
    }

    "net/minecraft/world/level/BlockAndLightGetter".getBrightness
    "(Lnet/minecraft/world/level/LightLayer;Lnet/minecraft/core/BlockPos;)I" if "net/minecraft/server/level/ServerLevel" =>
    fn level_brightness(game, layer: i32, pos: Pos) -> i32 {
        let light = i32::from(game.world.light(pos.0.x, pos.0.y, pos.0.z));
        if layer == 0 { light >> 4 } else { light & 15 }
    }

    "net/minecraft/world/level/LevelReader".getBiome
    "(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/core/Holder;" if "net/minecraft/server/level/ServerLevel" =>
    fn level_biome(game, pos: Pos) -> Text {
        let [x, y, z] = game.streaming.biome_zoom.quart_at_block(pos.0).map(|quart| quart << 2);
        game.java_biome(x, y, z)
    }

    "net/minecraft/world/level/LevelReader".getNoiseBiome
    "(III)Lnet/minecraft/core/Holder;" if "net/minecraft/server/level/ServerLevel" =>
    fn level_noise_biome(game, x: i32, y: i32, z: i32) -> Text {
        game.java_biome(x << 2, y << 2, z << 2)
    }

    "net/minecraft/server/level/ServerLevel".getUncachedNoiseBiome
    "(III)Lnet/minecraft/core/Holder;" =>
    fn level_uncached_noise_biome(game, x: i32, y: i32, z: i32) -> Text {
        game.java_biome(x << 2, y << 2, z << 2)
    }

    "net/minecraft/server/level/ServerLevel".getSeaLevel
    "()I" =>
    fn level_sea_level(game) -> i32 {
        match game.dim {
            Dim::Overworld => 63,
            Dim::Nether => 32,
            Dim::End => 0,
        }
    }

    "net/minecraft/server/level/ServerLevel".getSeed
    "()J" =>
    fn level_seed(game) -> i64 {
        game.level.seed
    }

    "net/minecraft/world/level/Level".getRainLevel
    "(F)F" if "net/minecraft/server/level/ServerLevel" =>
    fn level_rain_level(game, partial: f32) -> f32 {
        game.world.weather.levels(partial).0
    }

    "net/minecraft/world/level/Level".getThunderLevel
    "(F)F" if "net/minecraft/server/level/ServerLevel" =>
    fn level_thunder_level(game, partial: f32) -> f32 {
        game.world.weather.levels(partial).1
    }

    "net/minecraft/world/level/Level".isBrightOutside
    "()Z" if "net/minecraft/server/level/ServerLevel" =>
    fn level_bright_outside(game) -> bool {
        game.dim == Dim::Overworld && game.sky_darken() < 4
    }

    "net/minecraft/world/level/Level".getOverworldClockTime
    "()J" if "net/minecraft/server/level/ServerLevel" =>
    fn level_overworld_clock_time(game) -> i64 {
        game.world.day_time
    }

    "net/minecraft/world/level/Level".getDefaultClockTime
    "()J" if "net/minecraft/server/level/ServerLevel" =>
    fn level_default_clock_time(game) -> i64 {
        if game.dim == Dim::Nether { 0 } else { game.world.day_time }
    }

    "net/minecraft/world/level/LevelAccessor".getDifficulty
    "()Lnet/minecraft/world/Difficulty;" if "net/minecraft/server/level/ServerLevel" =>
    fn level_difficulty(game) -> i32 {
        game.difficulty_id()
    }

    "net/minecraft/server/level/ServerLevel".getCurrentDifficultyAt
    "(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/DifficultyInstance;" =>
    fn level_difficulty_at(game, pos: Pos) -> Doubles {
        let key = (pos.0.x >> 4, pos.0.z >> 4);
        let (local, moon) = if game.world.chunks.contains_key(&key) {
            (game.world.inhabited.get(&key).copied().unwrap_or(0), game.java_moon_brightness())
        } else {
            (0, 0.0)
        };
        Doubles(vec![f64::from(game.difficulty_id()), game.world.day_time as f64, local as f64, f64::from(moon)])
    }

    "net/minecraft/server/level/ServerLevel".getMoonBrightness
    "(Lnet/minecraft/core/BlockPos;)F" =>
    fn level_moon_brightness(game, _pos: Skip) -> f32 {
        game.java_moon_brightness()
    }

    "net/minecraft/server/level/ServerLevel".getRespawnData
    "()Lnet/minecraft/world/level/storage/LevelData$RespawnData;" =>
    fn level_respawn_data(game) -> Doubles {
        match game.level.world_spawn {
            Some([x, y, z]) => {
                let [yaw, pitch] = game.level.world_spawn_angle;
                Doubles(vec![f64::from(x), f64::from(y), f64::from(z), f64::from(yaw), f64::from(pitch)])
            }
            None => Doubles(Vec::new()),
        }
    }

    "net/minecraft/world/level/CollisionGetter".noBlockCollision
    "(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/phys/AABB;Z)Z" if "net/minecraft/server/level/ServerLevel" =>
    fn level_no_block_collision(game, _entity: Skip, bounds: Bounds, fluids: bool) -> bool {
        let b = bounds.0;
        let area = Aabb::new(Vec3::new(b[0], b[1], b[2]), Vec3::new(b[3], b[4], b[5]));
        let mut scene = SceneCollisions { world: &game.world, binding: game.res.collision(), subject: None };
        if fluids { no_collision_or_liquid(&mut scene, area) } else { no_collision_empty(&mut scene, area) }
    }

    "net/minecraft/world/level/EntityGetter".hasNearbyAlivePlayer
    "(DDDD)Z" if "net/minecraft/server/level/ServerLevel" =>
    fn level_has_nearby_alive_player(game, x: f64, y: f64, z: f64, distance: f64) -> bool {
        game.alive_player_near(x, y, z, distance)
    }

    "net/minecraft/world/level/ScheduledTickAccess".scheduleTick
    "(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/Block;ILnet/minecraft/world/ticks/TickPriority;)V" if "net/minecraft/server/level/ServerLevel" =>
    fn level_schedule_block_tick(game, pos: Pos, block: State, delay: i32, priority: i32) -> () {
        let Some(state) = block.0 else { return };
        game.with_level(|b, level, ticks| {
            let block = b.block(state);
            ticks.schedule_block(level.game_time(), pos.0, block, delay, TickPriority::by_value(priority));
        });
    }

    "net/minecraft/world/level/ScheduledTickAccess".scheduleTick
    "(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/material/Fluid;I)V" if "net/minecraft/server/level/ServerLevel" =>
    fn level_schedule_fluid_tick(game, pos: Pos, fluid: Text, delay: i32) -> () {
        game.java_schedule_fluid(pos.0, &fluid.0, delay, 0);
    }

    "net/minecraft/world/level/ScheduledTickAccess".scheduleTick
    "(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/material/Fluid;ILnet/minecraft/world/ticks/TickPriority;)V" if "net/minecraft/server/level/ServerLevel" =>
    fn level_schedule_fluid_tick_priority(game, pos: Pos, fluid: Text, delay: i32, priority: i32) -> () {
        game.java_schedule_fluid(pos.0, &fluid.0, delay, priority);
    }

    "net/minecraft/server/level/ServerLevel".sectionsToVillage
    "(Lnet/minecraft/core/SectionPos;)I" =>
    fn level_sections_to_village(game, section: Pos) -> i32 {
        let at = section.0;
        let centers = game.village_centers();
        raids::sections_to_village(&centers, raids::section([at.x, at.y, at.z]))
    }

    "net/minecraft/server/level/ServerLevel".isRaided
    "(Lnet/minecraft/core/BlockPos;)Z" =>
    fn level_is_raided(game, pos: Pos) -> bool {
        game.raiding_nearby([pos.0.x, pos.0.y, pos.0.z]).is_some()
    }

    "net/minecraft/server/level/ServerLevel".blockEvent
    "(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/Block;II)V" =>
    fn level_block_event(game, pos: Pos, block: State, param_a: i32, param_b: i32) -> () {
        let Some(state) = block.0 else { return };
        game.with_level(|b, _, ticks| {
            let event = BlockEventData { pos: pos.0, block: b.block(state), param_a, param_b };
            if !ticks.block_events.contains(&event) {
                ticks.block_events.push(event);
            }
        });
    }

    "net/minecraft/server/level/ServerLevel".getForceLoadedChunks
    "()Lit/unimi/dsi/fastutil/longs/LongSet;" =>
    fn level_forced_chunks(game) -> Longs {
        Longs(game.streaming.forced.iter().map(|&(x, z)| i64::from(x) & 0xFFFF_FFFF | (i64::from(z) & 0xFFFF_FFFF) << 32).collect())
    }

    "net/minecraft/server/level/ServerLevel".globalLevelEvent
    "(ILnet/minecraft/core/BlockPos;I)V" =>
    fn level_global_event(game, kind: i32, pos: Pos, data: i32) -> () {
        game.global_level_event(kind, [pos.0.x, pos.0.y, pos.0.z], data);
    }
}}
