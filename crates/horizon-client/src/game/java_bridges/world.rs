//! The level's blocks, signals and events.

use super::*;

horizon_java_host::java_bridges! { table for Game<'static> {
    "net/minecraft/world/level/Level".getBlockState
    "(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/state/BlockState;" if "net/minecraft/server/level/ServerLevel" =>
    fn level_get_block_state(game, pos: Pos) -> BlockStateId {
        game.world.block(pos.0.x, pos.0.y, pos.0.z)
    }

    "net/minecraft/world/level/Level".isLoaded
    "(Lnet/minecraft/core/BlockPos;)Z" if "net/minecraft/server/level/ServerLevel" =>
    fn level_is_loaded(game, pos: Pos) -> bool {
        game.world.chunks.contains_key(&(pos.0.x >> 4, pos.0.z >> 4))
    }

    "net/minecraft/world/level/Level".setBlock
    "(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/state/BlockState;II)Z" if "net/minecraft/server/level/ServerLevel" =>
    fn level_set_block(game, pos: Pos, state: State, flags: i32, _limit: i32) -> bool {
        let Some(state) = state.0 else { return false };
        let set = game.with_level(|b, level, ticks| b.set_block(level, ticks, pos.0, state, flags));
        game.after_java_change(set)
    }

    "net/minecraft/world/level/Level".removeBlock
    "(Lnet/minecraft/core/BlockPos;Z)Z" if "net/minecraft/server/level/ServerLevel" =>
    fn level_remove_block(game, pos: Pos, moved: bool) -> bool {
        let removed = game.with_level(|b, level, ticks| b.remove_block(level, ticks, pos.0, moved));
        game.after_java_change(removed)
    }

    "net/minecraft/world/level/Level".destroyBlock
    "(Lnet/minecraft/core/BlockPos;ZLnet/minecraft/world/entity/Entity;I)Z" if "net/minecraft/server/level/ServerLevel" =>
    fn level_destroy_block(game, pos: Pos, drop: bool, _breaker: Skip, _limit: i32) -> bool {
        let destroyed = game.with_level(|b, level, ticks| b.destroy_block(level, ticks, pos.0, drop));
        game.after_java_change(destroyed)
    }

    "net/minecraft/world/level/Level".updateNeighbourForOutputSignal
    "(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/Block;)V" if "net/minecraft/server/level/ServerLevel" =>
    fn level_update_output_signal(game, pos: Pos, _block: Skip) -> () {
        game.with_level(|b, level, ticks| b.update_neighbour_for_output_signal(level, ticks, pos.0));
        game.after_java_change(())
    }

    "net/minecraft/server/level/ServerLevel".updateNeighborsAt
    "(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/Block;)V" =>
    fn level_update_neighbors(game, pos: Pos, block: State) -> () {
        let Some(state) = block.0 else { return };
        game.with_level(|b, level, ticks| {
            let block = b.block(state);
            b.update_neighbors_at(level, ticks, pos.0, block);
        });
        game.after_java_change(())
    }

    "net/minecraft/world/level/ScheduledTickAccess".scheduleTick
    "(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/Block;I)V" if "net/minecraft/server/level/ServerLevel" =>
    fn level_schedule_tick(game, pos: Pos, _block: Skip, delay: i32) -> () {
        game.with_level(|b, level, ticks| b.schedule_tick(level, ticks, pos.0, delay));
    }

    "net/minecraft/world/level/SignalGetter".hasNeighborSignal
    "(Lnet/minecraft/core/BlockPos;)Z" if "net/minecraft/server/level/ServerLevel" =>
    fn level_has_neighbor_signal(game, pos: Pos) -> bool {
        game.with_level(|b, level, ticks| b.has_neighbor_signal(level, ticks, pos.0))
    }

    "net/minecraft/world/level/SignalGetter".getBestNeighborSignal
    "(Lnet/minecraft/core/BlockPos;)I" if "net/minecraft/server/level/ServerLevel" =>
    fn level_best_neighbor_signal(game, pos: Pos) -> i32 {
        game.with_level(|b, level, ticks| b.best_neighbor_signal(level, ticks, pos.0))
    }

    "net/minecraft/world/level/SignalGetter".getSignal
    "(Lnet/minecraft/core/BlockPos;Lnet/minecraft/core/Direction;)I" if "net/minecraft/server/level/ServerLevel" =>
    fn level_signal(game, pos: Pos, direction: Dir) -> i32 {
        game.with_level(|b, level, ticks| b.signal(level, ticks, pos.0, direction.0))
    }

    "net/minecraft/server/level/ServerLevel".levelEvent
    "(Lnet/minecraft/world/entity/Entity;ILnet/minecraft/core/BlockPos;I)V" =>
    fn level_event(game, _source: Skip, kind: i32, pos: Pos, data: i32) -> () {
        game.level_event_at(kind, [pos.0.x, pos.0.y, pos.0.z], data);
    }

    "net/minecraft/server/level/ServerLevel".gameEvent
    "(Lnet/minecraft/core/Holder;Lnet/minecraft/world/phys/Vec3;Lnet/minecraft/world/level/gameevent/GameEvent$Context;)V" =>
    fn level_game_event(game, event: Text, at: Point, _context: Skip) -> () {
        game.post_game_event(interned(event.0), at.0, Default::default());
    }

    "net/minecraft/world/level/Level".getHeight
    "(Lnet/minecraft/world/level/levelgen/Heightmap$Types;II)I" if "net/minecraft/server/level/ServerLevel" =>
    fn level_height(game, kind: Text, x: i32, z: i32) -> i32 {
        game.heightmap_at(&kind.0, x, z)
    }

    "net/minecraft/world/level/BlockAndLightGetter".getRawBrightness
    "(Lnet/minecraft/core/BlockPos;I)I" if "net/minecraft/server/level/ServerLevel" =>
    fn level_raw_brightness(game, pos: Pos, darken: i32) -> i32 {
        game.with_level(|_, level, _| horizon_world::level::raw_brightness(level, pos.0, darken))
    }

    "net/minecraft/world/level/Level".getSkyDarken
    "()I" if "net/minecraft/server/level/ServerLevel" =>
    fn level_sky_darken(game) -> i32 {
        game.sky_darken()
    }

    "net/minecraft/server/level/ServerPlayer".awardStat
    "(Lnet/minecraft/stats/Stat;I)V" =>
    fn player_award_stat(game, stat: Text, count: i32) -> () {
        if let Some((kind, key)) = stat.0.split_once('\t') {
            game.stats.add(kind, key, count);
        }
    }

    "net/minecraft/server/level/ServerPlayer".sendSystemMessage
    "(Lnet/minecraft/network/chat/Component;Z)V" =>
    fn player_system_message(game, message: Bytes, _overlay: bool) -> () {
        let tag = horizon_world::nbt::read_named(&message.0, Default::default());
        if let Ok((root, _)) = tag && let Some(text) = root.tag.get("text") {
            game.system_message(remote::remote_text(text));
        }
    }

    "net/minecraft/world/clock/ServerClockManager$ServerClockInstance".totalTicks
    "()J" =>
    fn clock_total_ticks(game) -> i64 {
        game.world.day_time
    }

    "net/minecraft/server/level/ServerLevel".sendBlockUpdated
    "(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/block/state/BlockState;I)V" =>
    fn level_send_block_updated(_game, _pos: Skip, _old: Skip, _current: Skip, _flags: Skip) -> () {
        // Horizon's block log already tells clients of every change.
    }
}}
