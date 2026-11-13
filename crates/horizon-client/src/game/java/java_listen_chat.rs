// game-module-cfg: feature java-mods
// java-listener-family
//! `ServerChatEvent` on `server_chat`: NeoForge's chat decorator edits or cancels a guest's message.

use super::java_listener::{Family, Subscriptions, crossed, with_game};
use super::java_mods::ChatVerdict;
use crate::game::api_values::TryIntoApi;
use horizon_client::scoreboard::{comp_text, tag_to_json};
use mod_api::{Flow, HookId, Listener, ServerChatEvent, ServerHost};

const HOOKS: &[HookId] = &[HookId::ServerChat];
static SUBSCRIBED: Subscriptions = Subscriptions::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(Chat),
    }
}

struct Chat;

impl mod_api::Hooks for Chat {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED, false)
    }

    fn listeners(&self) -> &'static [Listener] {
        super::java_listener::listeners(HOOKS, &SUBSCRIBED, false)
    }

    fn server_chat(&self, host: &mut dyn ServerHost, event: &mut ServerChatEvent<'_>) -> Flow {
        crossed(HookId::ServerChat);
        let (sender, raw) = (event.player, event.raw_text);
        let verdict = with_game(host, |game| {
            let conn = game
                .lan
                .as_ref()?
                .hub
                .guests
                .iter()
                .find(|guest| guest.uuid == sender)?
                .conn;
            game.as_guest(conn, |game| game.java_server_chat(raw))
        })
        .flatten()
        .unwrap_or(ChatVerdict::Unchanged);
        match verdict {
            ChatVerdict::Unchanged => Flow::Continue,
            ChatVerdict::Cancelled => Flow::Cancel,
            ChatVerdict::Changed(tag) => {
                match comp_text(&tag_to_json(&tag)).try_into_api() {
                    Ok(text) => *event.message = text,
                    Err(_) => log::warn!("Horizon: a Java mod's edited chat message is too deep to keep"),
                }
                Flow::Continue
            }
        }
    }
}
