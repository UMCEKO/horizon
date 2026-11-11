// game-module-cfg: feature java-mods
// java-listener-family
//! A player's break and attack (`BreakBlockEvent`, `AttackEntityEvent`), posted on the acting player's view.

use super::java_listener::{Family, listens, with_game};
use horizon_core::position::BlockPos;
use horizon_java_host::wire::Out;
use mod_api::{AttackEntityEvent, BreakBlockEvent, Flow, HookId, ServerHost};
use std::sync::OnceLock;

const HOOKS: &[HookId] = &[HookId::BreakBlock, HookId::AttackEntity];
static SUBSCRIBED: OnceLock<Vec<Option<HookId>>> = OnceLock::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(PlayerActions),
    }
}

struct PlayerActions;

/// Posts `event` with `fields` on the acting player's view: `Cancel` when a listener cancelled it.
fn post(host: &mut dyn ServerHost, hook: HookId, event: &str, fields: Out) -> Flow {
    if !listens(hook) {
        return Flow::Continue;
    }
    let cancelled = with_game(host, |game| {
        let mut cancelled = false;
        game.java_post_server_event(event, fields.0, &mut cancelled);
        cancelled
    });
    if cancelled == Some(true) {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

impl mod_api::Hooks for PlayerActions {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED)
    }

    fn break_block(&self, host: &mut dyn ServerHost, event: &mut BreakBlockEvent<'_>) -> Flow {
        let mut fields = Out::default();
        fields.long(BlockPos::new(event.pos[0], event.pos[1], event.pos[2]).pack());
        post(host, HookId::BreakBlock, "BreakBlockEvent", fields)
    }

    fn attack_entity(&self, host: &mut dyn ServerHost, event: &mut AttackEntityEvent<'_>) -> Flow {
        let mut fields = Out::default();
        fields.int(event.entity);
        post(host, HookId::AttackEntity, "AttackEntityEvent", fields)
    }
}
