// game-module-cfg: feature java-mods
// java-listener-family
//! `PistonEvent.Pre` and `PistonEvent.Post` on `piston_pre` and `piston_post`: posted from the stepped block-event queue (`run_block_events_stepped`), where the game is at hand.

use super::java_listener::Family;
use mod_api::HookId;

const HOOKS: &[HookId] = &[HookId::PistonPre, HookId::PistonPost];

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(Pistons),
    }
}

struct Pistons;

impl mod_api::Hooks for Pistons {
    // Never subscribed: the hooks fire with no game to lend Java, so the stepped queue posts instead.
    fn subscribed(&self) -> &'static [Option<HookId>] {
        &[]
    }
}
