//! Vanilla methods Java mods reach, served by Horizon's own ports (`docs/java-mods.md`).

use super::*;
pub(super) use horizon_java_host::bridge::{Bytes, Dir, Point, Pos, Skip, State, Text};
pub(super) use horizon_world::registry::BlockStateId;

mod block_entities;
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
    [
        block_entities::table(),
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

impl Game<'_> {
    /// What a level change from Java leaves for the game to answer: sounds, particles, updates to clients.
    pub(super) fn after_java_change<R>(&mut self, out: R) -> R {
        if let Err(e) = self.process_level_events() {
            log::warn!("Horizon: level events after a Java mod's change: {e}");
        }
        out
    }
}
