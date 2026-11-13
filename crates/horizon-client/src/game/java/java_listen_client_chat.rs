// game-module-cfg: feature java-mods
// java-listener-family
//! `ClientChatEvent` and `RegisterClientCommandsEvent` from the chat box: the Java client runs a typed line's `sendChat` or a client command, and its names join the chat box's tree.

use super::java_chat::{ChatSend, is_client_command};
use super::java_listener::{Family, listens, with_client_game};
use mod_api::{
    ClientChatEvent, ClientHost, CommandEvent, Flow, HookId, RegisterClientCommandsEvent,
};
use std::sync::OnceLock;

const HOOKS: &[HookId] = &[HookId::ClientChat, HookId::RegisterClientCommands];
static SUBSCRIBED: OnceLock<Vec<Option<HookId>>> = OnceLock::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(Chat),
    }
}

struct Chat;

impl mod_api::Hooks for Chat {
    // `command` has no event of its own: a Java mod's client commands come from `RegisterClientCommandsEvent`.
    fn subscribed(&self) -> &'static [Option<HookId>] {
        SUBSCRIBED.get_or_init(|| {
            let mut hooks = super::java_listener::subscribed(HOOKS, &SUBSCRIBED_BASE).to_vec();
            if hooks
                .iter()
                .any(|hook| matches!(hook, Some(HookId::RegisterClientCommands)))
            {
                hooks.push(Some(HookId::Command));
            }
            hooks
        })
    }

    // Vanilla's `sendChat` posts the event each time, so every line crosses while a Java mod listens.
    fn client_chat(&self, host: &mut dyn ClientHost, event: &mut ClientChatEvent<'_>) -> Flow {
        if !listens(HookId::ClientChat) {
            return Flow::Continue;
        }
        let line = event.message.clone();
        match with_client_game(host, |game| game.java_chat_send(&line)) {
            Some(ChatSend::Nothing) => Flow::Cancel,
            Some(ChatSend::Text(text)) => {
                *event.message = text;
                Flow::Continue
            }
            Some(ChatSend::AsTyped) | None => Flow::Continue,
        }
    }

    fn register_client_commands(
        &self,
        host: &mut dyn ClientHost,
        event: &mut RegisterClientCommandsEvent<'_>,
    ) {
        let names = with_client_game(host, |game| game.java_client_command_names());
        for name in names.into_iter().flatten() {
            event
                .commands
                .push(mod_api::Command::new(&format!("{name} <args:text>")));
        }
    }

    // Only a line whose first word is a root of the Java client's dispatcher crosses.
    fn command(&self, host: &mut dyn ClientHost, event: &mut CommandEvent<'_>) -> Flow {
        if !is_client_command(event.command) {
            return Flow::Continue;
        }
        let line = format!("/{}", event.command);
        match with_client_game(host, |game| game.java_chat_send(&line)) {
            Some(ChatSend::Nothing) => Flow::Cancel,
            _ => Flow::Continue,
        }
    }
}

static SUBSCRIBED_BASE: OnceLock<Vec<Option<HookId>>> = OnceLock::new();
