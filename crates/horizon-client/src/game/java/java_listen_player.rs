// game-module-cfg: feature java-mods
// java-listener-family
//! A player's break and attack (`BreakBlockEvent`, `AttackEntityEvent`), posted on the acting player's view.

use super::java_listener::{Family, Subscriptions, crossed, with_game};
use horizon_core::position::BlockPos;
use horizon_java_host::wire::Out;
use mod_api::{AttackEntityEvent, BreakBlockEvent, Flow, HookId, Listener, ServerHost};

const HOOKS: &[HookId] = &[HookId::BreakBlock, HookId::AttackEntity];
static SUBSCRIBED: Subscriptions = Subscriptions::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(PlayerActions),
    }
}

struct PlayerActions;

/// Posts `event` with `fields` on the acting player's view, cancelled already when an earlier native mod cancelled it; `cancelled` is what the listeners left.
fn post(
    host: &mut dyn ServerHost,
    hook: HookId,
    event: &str,
    cancelled: &mut bool,
    fields: Out,
) -> Flow {
    crossed(hook);
    let arrives = *cancelled;
    let posted = with_game(host, |game| {
        game.java_post_player_event(event, arrives, fields.0)
    });
    if let Some((left, _)) = posted.flatten() {
        *cancelled = left;
    }
    if *cancelled {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

impl mod_api::Hooks for PlayerActions {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED, true)
    }

    fn listeners(&self) -> &'static [Listener] {
        super::java_listener::listeners(HOOKS, &SUBSCRIBED, true)
    }

    fn break_block(&self, host: &mut dyn ServerHost, event: &mut BreakBlockEvent<'_>) -> Flow {
        let mut fields = Out::default();
        fields.long(BlockPos::new(event.pos[0], event.pos[1], event.pos[2]).pack());
        post(
            host,
            HookId::BreakBlock,
            "BreakBlockEvent",
            &mut event.cancelled,
            fields,
        )
    }

    fn attack_entity(&self, host: &mut dyn ServerHost, event: &mut AttackEntityEvent<'_>) -> Flow {
        let mut fields = Out::default();
        fields.int(event.entity);
        post(
            host,
            HookId::AttackEntity,
            "AttackEntityEvent",
            &mut event.cancelled,
            fields,
        )
    }
}
