// game-module-cfg: feature java-mods
// java-listener-family
//! A block's neighbour broadcast (`BlockEvent.NeighborNotifyEvent`), posted from inside the level cascade with level reads only.

use super::java_cascade::{post, sides_but};
use super::java_listener::{Family, listens};
use crate::game::api_values::TryIntoEngine;
use horizon_java_host::bridge::Ret;
use mod_api::{Flow, HookId, LevelHost, NeighborNotifyEvent};
use std::sync::OnceLock;

const HOOKS: &[HookId] = &[HookId::NeighborNotify];
static SUBSCRIBED: OnceLock<Vec<Option<HookId>>> = OnceLock::new();
const EVENT: &str = "BlockEvent.NeighborNotifyEvent";

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(NeighborNotify),
    }
}

struct NeighborNotify;

impl mod_api::Hooks for NeighborNotify {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED)
    }

    fn neighbor_notify(&self, level: &dyn LevelHost, event: &mut NeighborNotifyEvent) -> Flow {
        if !listens(HookId::NeighborNotify) {
            return Flow::Continue;
        }
        let state = TryIntoEngine::try_into_engine(&event.state, &()).map_or(
            horizon_java_host::bridge::UNMAPPED,
            |state: horizon_world::registry::BlockStateId| Ret::into_raw(state),
        );
        let canceled = post(level, event.dimension, EVENT, |out| {
            out.long(event.pos.pack());
            out.int(state);
            out.0.push(sides_but(event.skipped_side));
        });
        if canceled == Some(true) {
            Flow::Cancel
        } else {
            Flow::Continue
        }
    }
}
