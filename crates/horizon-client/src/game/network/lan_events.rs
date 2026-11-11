//! The hub's gameplay events of a LAN tick: joins, departures, chat and guest commands.
use super::lan::{
    LanHost, client_info, describe_burst, event_kind, event_timings::EventTimings, note_event,
};
use super::Game;
use crate::game::api_values::{TryIntoApi, TryIntoEngine};
use horizon_client::{
    lan::{entity::EntityState, hub::HostEvent},
    text::Text,
};
use horizon_protocol::packets::play::{self, Clientbound as C};
use std::time::Instant;

impl Game<'_> {
    /// Apply gameplay events produced while the hub handles guest packets.
    ///
    /// Both the pre-block packet phase and the normal LAN tick receive the
    /// same event stream. Keep their side effects in one place so joins,
    /// departures, chat, and queued guest commands cannot drift apart. None when a native hook's
    /// command closed the hub.
    pub(super) fn lan_process_events(
        &mut self,
        host: LanHost,
        events: impl IntoIterator<Item = HostEvent>,
        entities: &[EntityState],
        spawn: [f64; 3],
    ) -> Option<LanHost> {
        // A slow tick's line says which event took it, and whose (`spans::note`): each event's
        // time is noted when the next one starts (an event's arm may `continue`), the last's
        // after the loop.
        let timed = horizon_client::spans::recording_here();
        let mut previous = None;
        // The arms below borrow the hub through `self.lan` (their helpers take no host:
        // the fork's borrow shape), so the host is put back for the match and taken out
        // again on the way through.
        self.lan = Some(host);
        let mut timings = EventTimings::new(horizon_client::spans::NOTE_MS);
        for event in events {
            if timed && let Some(host) = self.lan.as_ref() {
                note_event(host, previous.take(), &mut timings);
            }
            if timed {
                let detail = self
                    .lan
                    .as_ref()
                    .and_then(|host| self.event_detail(host, &event));
                previous = Some((Instant::now(), event_kind(&event), detail));
            }
            match event {
                event @ (HostEvent::Chat { .. } | HostEvent::Warn(_) | HostEvent::Error(_)) => {
                    self.lan_log_event(event);
                }
                HostEvent::Ban { uuid, name, reason } => {
                    self.anticheat_ban(uuid, &name, &reason);
                }
                HostEvent::ChatFiltered {
                    sender,
                    name,
                    place,
                    text,
                    count,
                    action,
                } => self.chat_filtered(sender, &name, place, &text, count, action),
                HostEvent::PlayerChat { sender, text } => {
                    self.lan_player_chatted(sender, &text);
                }
                HostEvent::ChatSubmitted {
                    message,
                    bound,
                    name,
                } => {
                    let host = self.lan.as_ref()?;
                    let Some((id, conn)) = host
                        .hub
                        .guests
                        .iter()
                        .find(|g| g.placed && g.participates_online() && g.uuid == message.sender)
                        .map(|g| (g.entity_id, g.conn))
                    else {
                        continue;
                    };
                    let mut message = *message;
                    let sender = message.sender;
                    let raw = message.content.clone();
                    // `PlayerChatMessage.decoratedContent`: the text as a literal.
                    let original = Text::untrusted(raw.clone());
                    let mut text = original.clone();
                    let mut cancel = false;
                    let mut replies = Vec::new();
                    let mut engine_replies = Vec::new();
                    if mod_api::__private::listened(mod_api::HookId::ServerChat)
                        && let Ok(mut api_text) = text.try_into_api()
                    {
                        self.native_server_chat(mod_api::__private::ServerChatCall {
                            player_id: id,
                            player: sender,
                            username: &name,
                            raw_text: &raw,
                            message: &mut api_text,
                            replies: &mut replies,
                            cancel: &mut cancel,
                        });
                        // Validate all fields before applying any event writes.
                        let converted: std::result::Result<_, mod_api::text::TextFormError> =
                            (|| {
                                Ok((
                                    api_text.try_into_engine(&())?,
                                    replies.try_into_engine(&())?,
                                ))
                            })();
                        match converted {
                            Ok((message, answers)) => {
                                text = message;
                                engine_replies = answers;
                            }
                            Err(_) => {
                                cancel = false;
                            }
                        }
                    }
                    // The mods' replies reach the sender alone, as a
                    // command's do, before the message (if any) goes out.
                    if !engine_replies.is_empty() {
                        let host = self.lan.as_ref()?;
                        for line in engine_replies {
                            host.hub.send_to(
                                conn,
                                &C::SystemChat(play::SystemChat {
                                    content: horizon_client::lan::data::component_nbt(
                                        &line.component(),
                                    ),
                                    overlay: false,
                                }),
                            );
                        }
                    }
                    // Canceled: stop before `detectRateSpam`.
                    if cancel {
                        continue;
                    }
                    let mut line = format!("<{name}> {raw}");
                    if text != original {
                        // `withUnsignedContent`: clients show the mod's text,
                        // the signature still covers what was typed.
                        message.unsigned_content =
                            Some(horizon_client::lan::data::component_nbt(&text.component()));
                        line = format!("<{name}> {}", text.plain());
                    }
                    let mut chat = Vec::new();
                    let host = self.lan.as_mut()?;
                    host.hub.broadcast_chat(&message, &bound, line, &mut chat);
                    for event in chat {
                        self.lan_log_event(event);
                    }
                    self.lan_player_chatted(sender, &raw);
                }
                HostEvent::ChatSessionStarted(update) => {
                    let host = self.lan.as_mut()?;
                    host.hub.broadcast(&C::PlayerInfoUpdate(*update));
                }
                HostEvent::Joined(conn) => {
                    // The mods may refuse the login before the player is placed.
                    if let Some((uuid, name)) = self
                        .lan
                        .as_ref()
                        .and_then(|host| host.hub.guest(conn))
                        .map(|g| (g.uuid, g.name.clone()))
                    {
                        let mut cancel = false;
                        let mut reason = None;
                        self.native_player_login(uuid, &name, &mut cancel, &mut reason);
                        if cancel {
                            let reason = match reason {
                                Some(text) => {
                                    horizon_client::lan::data::component_nbt(&text.component())
                                }
                                None => horizon_protocol::text::translatable(
                                    "multiplayer.disconnect.generic",
                                    &[],
                                ),
                            };
                            let host = self.lan.as_mut()?;
                            host.hub.kick(conn, reason);
                            continue;
                        }
                    }
                    let host = self.lan.as_mut()?;
                    // PlayerList.placeNewPlayer: the name the user cache knew,
                    // before it learns this one, for a renamed player's join line.
                    let mut renamed = None;
                    if self.options.dedicated.is_some()
                        && let Some((uuid, name)) =
                            host.hub.guest(conn).map(|g| (g.uuid, g.name.clone()))
                        && let Some(old) = super::dedicated::cached_user_by_id(uuid)
                    {
                        if !old.name.eq_ignore_ascii_case(&name) {
                            renamed = Some(old.name.clone());
                        }
                        host.hub.former_names.insert(uuid, old.name);
                    }
                    self.lan_guest_joined(conn, entities, spawn);
                    let host = self.lan.as_ref()?;
                    if self.options.dedicated.is_some()
                        && let Some(g) = host.hub.guest(conn)
                    {
                        // PlayerList.placeNewPlayer, then the broadcast join message.
                        super::dedicated::player_joined(g.uuid, &g.name);
                        super::dedicated::events::record(
                            "join",
                            &g.name,
                            g.uuid,
                            serde_json::json!({ "address": g.address.ip().to_string() }),
                        );
                        let [x, y, z] = g.position;
                        super::dedicated::log(
                            "INFO",
                            &format!(
                                "{}[/{}] logged in with entity id {} at ({}, {}, {})",
                                g.name,
                                g.address,
                                g.entity_id,
                                super::dedicated::java_double(x),
                                super::dedicated::java_double(y),
                                super::dedicated::java_double(z)
                            ),
                        );
                        // A held player is not announced: it is not in the world yet.
                        if g.held {
                            super::dedicated::log(
                                "INFO",
                                &horizon_client::lan::arrivals::held_line(&g.name),
                            );
                        } else if let Some(old) = renamed {
                            let line = Text::translate(
                                "multiplayer.player.joined.renamed",
                                &[Text::literal(g.name.clone()), Text::literal(old)],
                            );
                            super::dedicated::log(
                                "INFO",
                                &format!("System chat: {}", line.plain()),
                            );
                        } else {
                            super::dedicated::log(
                                "INFO",
                                &format!("System chat: {} joined the game", g.name),
                            );
                        }
                    } else if self.player.is_some()
                        && let Some(g) = host.hub.guest(conn)
                        && g.placed
                    {
                        // The host's chat hears of the newcomer as the guests do.
                        let (uuid, name) = (g.uuid, g.name.clone());
                        let who = self.player_text(&name, uuid);
                        self.system_message(
                            Text::translate("multiplayer.player.joined", &[who])
                                .with_color(horizon_client::text::YELLOW),
                        );
                    }
                    if let Some((uuid, name)) = self
                        .lan
                        .as_ref()
                        .and_then(|host| host.hub.guest(conn))
                        .filter(|g| g.placed)
                        .map(|g| (g.uuid, g.name.clone()))
                    {
                        self.native_player_logged_in(uuid, &name);
                    }
                }
                HostEvent::Left(mut guest) => {
                    if !guest.participates_online() {
                        let mut host = self.lan.take()?;
                        self.cleanup_fake_player(&mut host, &guest);
                        self.lan = Some(host);
                        continue;
                    }
                    // The menu they had open closes, in the level they were in
                    // (`ServerPlayer.disconnect` -> `doCloseContainer`).
                    if guest.placed {
                        let back = self.dim;
                        let theirs = super::Dim::from_id(&guest.dimension);
                        if self.enter_level(theirs).is_ok() {
                            let conn = guest.conn;
                            let host = self.lan.as_mut()?;
                            let at = host.hub.guests.len();
                            host.hub.guests.push(*guest);
                            self.guest_disconnected(conn);
                            let Some(host) = self.lan.as_mut() else {
                                unreachable!("LanHost dropped inside a tick; closes are deferred")
                            };
                            assert_eq!(host.hub.guests[at].conn, conn);
                            *guest = host.hub.guests.remove(at);
                        }
                        if let Err(e) = self.enter_level(back) {
                            log::error!("could not reopen {}: {e}", back.id());
                        }
                    }
                    self.lan_crafting_left(&mut guest);
                    if guest.placed {
                        self.award_leave_game(guest.conn, guest.uuid, &guest.name);
                    }
                    // PlayerList.remove: the vehicle it alone rides goes too.
                    self.take_parent_vehicle(&mut guest);
                    self.save_guest(&mut guest);
                    let host = self.lan.as_mut()?;
                    host.play.remove(&guest.conn);
                    self.guest_combat.remove(&guest.conn);
                    // A held player was recorded as joined at its login: it leaves too.
                    if self.options.dedicated.is_some() && (guest.placed || guest.held) {
                        super::dedicated::events::record(
                            "leave",
                            &guest.name,
                            guest.uuid,
                            serde_json::json!({ "reason": guest.left_reason }),
                        );
                        if guest.held && !guest.placed {
                            // Never announced, so no "left the game" either.
                            super::dedicated::log(
                                "INFO",
                                &horizon_client::lan::arrivals::dropped_line(
                                    &guest.name,
                                    &guest.left_reason,
                                ),
                            );
                        } else {
                            super::dedicated::log(
                                "INFO",
                                &format!("{} lost connection: {}", guest.name, guest.left_reason),
                            );
                        }
                        // A player moved to another server without `announce` was not
                        // announced as leaving, so the line is not logged either; a held
                        // player was never announced at all.
                        if guest.placed && !guest.quiet_leave {
                            super::dedicated::log(
                                "INFO",
                                &format!("System chat: {} left the game", guest.name),
                            );
                        }
                    } else if self.player.is_some() && guest.placed && !guest.quiet_leave {
                        // ServerGamePacketListenerImpl.removePlayerFromWorld.
                        let who = self.player_text(&guest.name, guest.uuid);
                        self.system_message(
                            Text::translate("multiplayer.player.left", &[who])
                                .with_color(horizon_client::text::YELLOW),
                        );
                    }
                    if guest.placed
                        && self
                            .lan
                            .as_ref()
                            .is_some_and(|host| !host.players_told_left)
                    {
                        self.native_player_logged_out(guest.uuid, &guest.name);
                    }
                }
                HostEvent::Flight { conn, flying } => {
                    self.lan_flight_toggled(conn, flying);
                }
                HostEvent::Sneak { conn, sneaking } => {
                    if !mod_api::__private::listened(mod_api::HookId::PlayerToggleSneak) {
                        continue;
                    }
                    if let Some(g) = self.lan.as_ref().and_then(|host| host.hub.guest(conn)) {
                        let (uuid, name) = (g.uuid, g.name.clone());
                        self.as_guest(conn, |game| {
                            game.native_player_toggle_sneak(uuid, &name, sneaking);
                        });
                    }
                }
                HostEvent::Kicked {
                    conn,
                    player,
                    name,
                    reason,
                } => {
                    if !mod_api::__private::listened(mod_api::HookId::PlayerKicked) {
                        continue;
                    }
                    // The same poll can have removed the guest (their `Left`): the mods hear the
                    // kick anyway, without the guest's state swapped in.
                    if self
                        .lan
                        .as_ref()
                        .is_some_and(|host| host.hub.guest(conn).is_some())
                    {
                        self.as_guest(conn, |game| {
                            game.native_player_kicked(player, &name, &reason);
                        });
                    } else {
                        self.native_player_kicked(player, &name, &reason);
                    }
                }
                HostEvent::Input { conn, flags } => {
                    if !mod_api::__private::listened(mod_api::HookId::PlayerInput) {
                        continue;
                    }
                    if let Some(g) = self.lan.as_ref().and_then(|host| host.hub.guest(conn)) {
                        let (uuid, name) = (g.uuid, g.name.clone());
                        self.as_guest(conn, |game| {
                            game.native_player_input(uuid, &name, flags);
                        });
                    }
                }
                HostEvent::ClientInformation(conn, old, new) => {
                    if let Some(g) = self.lan.as_ref().and_then(|host| host.hub.guest(conn)) {
                        let (uuid, entity_id, name) = (g.uuid, g.entity_id, g.name.clone());
                        // The packet's raw ids become the enums once, here.
                        let old = client_info(&old);
                        let new = client_info(&new);
                        self.native_client_information_updated(uuid, entity_id, &name, &old, &new);
                    }
                }
                HostEvent::PlayerLoaded(conn) => {
                    if let Some(g) = self.lan.as_ref().and_then(|host| host.hub.guest(conn)) {
                        let (uuid, name) = (g.uuid, g.name.clone());
                        self.native_player_client_loaded_world(uuid, &name);
                    }
                }
                HostEvent::RecipeBookSettings {
                    conn,
                    book,
                    open,
                    filtering,
                } => {
                    if let Some(g) = self.lan.as_ref().and_then(|host| host.hub.guest(conn)) {
                        let (uuid, name) = (g.uuid, g.name.clone());
                        self.native_player_recipe_book_settings_change(
                            uuid, &name, book, open, filtering,
                        );
                    }
                }
                HostEvent::Packet(conn, packet) => {
                    self.lan_guest_packet(conn, *packet);
                    if self
                        .lan
                        .as_ref()
                        .is_some_and(|host| !host.commands.is_empty())
                    {
                        self.run_guest_commands();
                    }
                }
                HostEvent::Jump { conn, from, yaw } => {
                    self.guest_jump(conn, from, yaw);
                }
            }
        }
        if timed && let Some(host) = self.lan.as_ref() {
            note_event(host, previous, &mut timings);
            for burst in timings.bursts() {
                horizon_client::spans::note(burst.ms, || describe_burst(host, &burst));
            }
        }
        self.lan.take()
    }
}
