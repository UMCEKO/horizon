// game-module-cfg: feature java-mods
// java-listener-family
//! `CommandEvent` for a player's native command; a Java root's line posts it in Java's own `performCommand`.

use super::java_commands::JavaVerdict;
use super::java_listener::{Family, listens, with_game};
use mod_api::{Flow, HookId, PlayerCommandEvent, ServerHost};
use std::sync::OnceLock;

const HOOKS: &[HookId] = &[HookId::PlayerCommand];
static SUBSCRIBED: OnceLock<Vec<Option<HookId>>> = OnceLock::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(PlayerCommands),
    }
}

struct PlayerCommands;

impl mod_api::Hooks for PlayerCommands {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED)
    }

    fn player_command(&self, host: &mut dyn ServerHost, event: &mut PlayerCommandEvent<'_>) -> Flow {
        if !listens(HookId::PlayerCommand) {
            return Flow::Continue;
        }
        let (player, command) = (event.player, event.command);
        match with_game(host, |game| game.java_command_event(player, command)).flatten() {
            Some(JavaVerdict::Stopped) => Flow::Cancel,
            _ => Flow::Continue,
        }
    }
}
