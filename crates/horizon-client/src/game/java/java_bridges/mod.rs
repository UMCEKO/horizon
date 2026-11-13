// game-module-cfg: feature java-mods
//! Vanilla methods Java mods reach, served by Horizon's own ports (`docs/java-mods.md`).

use super::*;
pub(super) use horizon_java_host::bridge::{Bytes, Dir, LevelOn, Point, Pos, Skip, State, Text};
pub(super) use horizon_world::registry::BlockStateId;

mod block_entities;
pub(super) mod client_world;
mod effects;
mod entities;
mod level_api;
mod packets;
mod player;
mod rules;
mod server;
mod values;
mod world;
mod worldgen;

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
        packets::table(),
        player::table(),
        rules::table(),
        server::table(),
        values::table(),
        world::table(),
        worldgen::table(),
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
        | "chunk_scan_structures"
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

/// Runs `f`, then `restore` on every exit, a panic included.
fn restored<G, R>(game: &mut G, f: impl FnOnce(&mut G) -> R, restore: impl FnOnce(&mut G)) -> R {
    let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut *game)));
    restore(game);
    out.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// The native level of the `ServerLevel` Java called on.
fn level_dim(on: &LevelOn) -> Dim {
    match on {
        LevelOn::Overworld => Dim::Overworld,
        LevelOn::Nether => Dim::Nether,
        LevelOn::End => Dim::End,
        LevelOn::Named(id) => Dim::extra(id)
            .unwrap_or_else(|| panic!("Horizon: Java called a level Horizon doesn't hold: {id:?}")),
    }
}

/// Most rounds of block events a level put in place for one call drains before it goes back.
const DRAIN_ROUNDS: usize = 64;

impl Game<'_> {
    /// Runs `f` with the level Java called on in place, swapped in as `levels.rs` does for a command's level and back on every exit.
    pub(super) fn on_level<R>(&mut self, on: &LevelOn, f: impl FnOnce(&mut Self) -> R) -> R {
        let dim = level_dim(on);
        if dim == self.dim || self.options.execution_role.is_client() {
            return f(self);
        }
        let back = self.dim;
        let aside = java_mods::set_block_events_aside();
        if let Err(e) = self.enter_level(dim) {
            aside.restore();
            panic!("Horizon: a Java mod's call on {}: {e}", dim.id());
        }
        restored(
            self,
            |game| {
                let out = f(game);
                game.java_push_stale();
                for _ in 0..DRAIN_ROUNDS {
                    if !java_mods::has_block_events() {
                        break;
                    }
                    game.java_drain_block_events();
                }
                out
            },
            |game| {
                if let Err(e) = game.enter_level(back) {
                    log::error!("Horizon: {} back in place after a Java mod's call: {e}", back.id());
                }
                aside.restore();
            },
        )
    }

    /// What a level change from Java leaves for the game to answer: sounds, particles, updates to clients.
    pub(super) fn after_java_change<R>(&mut self, out: R) -> R {
        if let Err(e) = self.process_level_events() {
            log::warn!("Horizon: level events after a Java mod's change: {e}");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::restored;

    #[test]
    fn the_level_goes_back_after_a_call_that_panics_and_one_that_returns() {
        let swap_back = |level: &mut (&str, u32)| {
            level.0 = "overworld";
            level.1 += 1;
        };
        let mut level = ("nether", 0);
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            restored(&mut level, |_| -> () { panic!("a mod's bridge failed") }, swap_back)
        }));
        assert!(caught.is_err());
        assert_eq!(level, ("overworld", 1));
        level.0 = "nether";
        assert_eq!(restored(&mut level, |l| l.0, swap_back), "nether");
        assert_eq!(level, ("overworld", 2));
    }
}
