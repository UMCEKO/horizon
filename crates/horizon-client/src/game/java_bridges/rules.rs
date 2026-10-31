//! Weather, chunks, spawn protection, world rules.

use super::*;

horizon_java_host::java_bridges! { table for Game<'static> {
    "net/minecraft/server/level/ServerLevel".canSpreadFireAround
    "(Lnet/minecraft/core/BlockPos;)Z" =>
    fn level_can_spread_fire_around(game, pos: Pos) -> bool {
        game.with_level(|_, level, _| level.can_spread_fire_around(pos.0))
    }

    "net/minecraft/server/level/ServerLevel".setChunkForced
    "(IIZ)Z" =>
    fn level_set_chunk_forced(game, x: i32, z: i32, forced: bool) -> bool {
        let key = (x, z);
        let updated = if forced {
            game.streaming.forced.insert(key)
        } else {
            game.streaming.forced.remove(&key)
        };
        if forced && updated {
            game.streaming.rank_forced(x, z, x, z);
        }
        game.after_java_change(updated)
    }

    "net/minecraft/server/level/ServerLevel".resetWeatherCycle
    "()V" =>
    fn level_reset_weather_cycle(game) -> () {
        let weather = &mut game.world.weather;
        weather.raining = false;
        weather.thundering = false;
        weather.rain_time = 0;
        weather.thunder_time = 0;
        game.after_java_change(())
    }
}}
