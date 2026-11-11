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
mod player;
mod rules;
mod server;
mod values;
mod world;

/// Every family's table, for the class processor.
pub(super) fn bridges() -> Vec<horizon_java_host::bridge::Bridge> {
    horizon_java_host::bridge::after_native::<Game<'static>>(Game::java_push_stale);
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
