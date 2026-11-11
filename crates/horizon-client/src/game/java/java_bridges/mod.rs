// game-module-cfg: feature java-mods
//! Vanilla methods Java mods reach, served by Horizon's own ports (`docs/java-mods.md`).

use super::*;
pub(super) use horizon_java_host::bridge::{Bytes, Dir, Point, Pos, Skip, State, Text};
pub(super) use horizon_world::registry::BlockStateId;

mod block_entities;
pub(super) mod client_world;
mod effects;
mod entities;
mod level_api;
mod player;
mod rules;
mod server;
mod values;
mod world;

/// Every family's table, for the class processor.
pub(super) fn bridges() -> Vec<horizon_java_host::bridge::Bridge> {
    horizon_java_host::bridge::after_native::<Game<'static>>(Game::java_push_stale);
    horizon_java_host::bridge::before_native::<Game<'static>>(Game::java_run_native);
    [
        block_entities::table(),
        client_world::table(),
        effects::table(),
        entities::table(),
        level_api::table(),
        player::table(),
        rules::table(),
        server::table(),
        values::table(),
        world::table(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// #268: whether a native Java calls during a run of block entity ticks ends the run after the entry that called it.
/// Stopping is the default: only a native that changes nothing in Horizon lets the run go on.
pub(super) fn ends_run(native: &str) -> bool {
    !matches!(
        native,
        // The run's own question before each entry.
        "block_entity_run_entry"
        // Does nothing in Horizon.
        | "level_send_block_updated"
        // Reads of blocks, chunks, signals, height and light.
        | "level_get_block_state"
        | "level_is_loaded"
        | "level_should_tick_blocks_at"
        | "chunk_cache_is_position_ticking"
        | "level_has_chunk"
        | "level_has_neighbor_signal"
        | "level_best_neighbor_signal"
        | "level_signal"
        | "level_height"
        | "level_raw_brightness"
        | "level_brightness"
        | "level_sky_darken"
        | "level_no_block_collision"
        // Reads of the level's data, biomes, weather, time and players.
        | "clock_total_ticks"
        | "level_data_hardcore"
        | "level_data_difficulty"
        | "level_data_difficulty_locked"
        | "level_biome"
        | "level_noise_biome"
        | "level_uncached_noise_biome"
        | "level_sea_level"
        | "level_seed"
        | "level_rain_level"
        | "level_thunder_level"
        | "level_bright_outside"
        | "level_overworld_clock_time"
        | "level_default_clock_time"
        | "level_difficulty"
        | "level_difficulty_at"
        | "level_moon_brightness"
        | "level_respawn_data"
        | "level_has_nearby_alive_player"
        | "level_is_raided"
        | "level_forced_chunks"
        // Reads of the server's settings and permissions.
        | "player_list_is_op"
        | "server_profile_permissions"
        | "integrated_profile_permissions"
        | "world_data_hardcore"
        | "world_data_difficulty"
        | "world_data_difficulty_locked"
        | "world_data_allow_commands"
        | "world_data_game_type"
        | "world_data_level_name"
    )
}

/// A name a port wants for good, kept once per name; game events are a short list.
pub(super) fn interned(name: String) -> &'static str {
    thread_local! {
        static NAMES: std::cell::RefCell<std::collections::HashSet<&'static str>> = Default::default();
    }
    NAMES.with_borrow_mut(|names| match names.get(name.as_str()) {
        Some(known) => known,
        None => {
            let known: &'static str = name.leak();
            names.insert(known);
            known
        }
    })
}

impl Game<'_> {
    /// What a level change from Java leaves for the game to answer: sounds, particles, updates to clients.
    pub(super) fn after_java_change<R>(&mut self, out: R) -> R {
        if let Err(e) = self.process_level_events() {
            log::warn!("Horizon: level events after a Java mod's change: {e}");
        }
        out
    }
}
