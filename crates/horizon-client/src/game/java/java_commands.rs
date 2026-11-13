// game-module-cfg: feature java-mods
//! Which root commands the Java server runs, or holds Java's nodes in, as it told at the server's start.

use super::*;
use horizon_core::nbt::{self, Tag};
use horizon_java_host::wire::{In, Out};
use mod_api::HookId;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

/// A root command Java runs, or whose tree has Java's nodes in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum JavaRoot {
    Java,
    Mixed,
}

/// `ModCommands`' root kind on the wire: 0 for a line no Java root holds.
fn root_byte(root: Option<JavaRoot>) -> u8 {
    match root {
        None => 0,
        Some(JavaRoot::Java) => 1,
        Some(JavaRoot::Mixed) => 2,
    }
}

/// What Java decided for a command line: `None` when native runs it as typed.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum JavaVerdict {
    Ran,
    Stopped,
    Replaced(String),
}

/// The head of a `command` or `console_command` reply: its code and replaced text; `None` inside means native runs the line.
fn read_verdict(reply: &mut In<'_>) -> Option<Option<JavaVerdict>> {
    let code = reply.byte()?;
    let replaced = reply.text()?;
    Some(match code {
        1 => Some(JavaVerdict::Ran),
        2 => Some(JavaVerdict::Stopped),
        0 if replaced.is_empty() => None,
        0 => Some(JavaVerdict::Replaced(replaced.to_owned())),
        _ => return None,
    })
}

/// Logs once that a command reply could not be read.
fn answered_in_pieces() {
    thread_local!(static SAID: Cell<bool> = const { Cell::new(false) });
    if !SAID.replace(true) {
        log::error!("Horizon: a Java command answered in pieces");
    }
}

thread_local! {
    /// The text a `CommandEvent` listener replaced a player's native command with, for the line `guest_command` is reading.
    static REPLACED: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Takes the replacement a `CommandEvent` listener left for the command just heard.
pub(super) fn java_take_replaced_command() -> Option<String> {
    REPLACED.take()
}

impl Game<'_> {
    /// A line from a player or the console for the Java server: `None` when native runs it as it is.
    pub(super) fn java_command(&mut self, command: &str, quiet: bool) -> Option<JavaVerdict> {
        if !super::java_mods::java_server_started() || !self.exec.functions.is_empty() {
            return None;
        }
        if self
            .command_source
            .is_some_and(|source| source.block.is_some() || source.cart.is_some())
        {
            return None;
        }
        let root = java_commands(|commands| commands.root(command));
        if self.console_broadcast.is_some() {
            // A native console line crosses only for `CommandEvent`; players' lines post it from `player_command`.
            if root.is_none() && !super::java_listener::listens(HookId::PlayerCommand) {
                return None;
            }
            return self.java_console_command(command, root);
        }
        if self.lan_acting.is_some() && root.is_some() {
            return self.java_player_command(command, root, quiet);
        }
        None
    }

    /// `CommandEvent` for a player's native command, posted on their view; `player_command` hears it outside their turn.
    pub(super) fn java_command_event(&mut self, player: u128, command: &str) -> Option<JavaVerdict> {
        if !super::java_mods::java_server_started()
            || java_commands(|commands| commands.root(command)).is_some()
        {
            return None;
        }
        let mut verdict = None;
        self.native_as_player(player, |game| {
            verdict = game.java_player_command(command, None, false);
        });
        if let Some(JavaVerdict::Replaced(text)) = &verdict {
            REPLACED.set(Some(text.clone()));
        }
        verdict
    }

    /// `command` on the acting player's view; what it did to their inventory and experience comes back as after a use.
    fn java_player_command(
        &mut self,
        command: &str,
        root: Option<JavaRoot>,
        quiet: bool,
    ) -> Option<JavaVerdict> {
        self.java_bind_states();
        let mut request = self.java_use_request()?;
        let mut tail = Out::default();
        tail.text(command);
        tail.0.push(root_byte(root));
        tail.bool(true);
        tail.bool(quiet);
        request.extend(tail.0);
        let reply = self.java_player_request("command", &request)?;
        let mut reply = In(&reply);
        let read = (|| {
            let verdict = read_verdict(&mut reply)?;
            let level = reply.int()?;
            self.java_apply_changes(level, &mut reply)?;
            self.java_apply_effects(&mut reply)?;
            Some(verdict)
        })();
        self.java_drain_block_events();
        read.unwrap_or_else(|| {
            answered_in_pieces();
            None
        })
    }

    /// `console_command` with the server as the source, in the level the console runs in.
    fn java_console_command(&mut self, command: &str, root: Option<JavaRoot>) -> Option<JavaVerdict> {
        let reply = self.java_level_request("console_command", |out| {
            out.text(command);
            out.0.push(root_byte(root));
            out.bool(true);
        })?;
        let mut reply = In(&reply);
        let read = (|| {
            let verdict = read_verdict(&mut reply)?;
            self.java_apply_effects(&mut reply)?;
            Some(verdict)
        })();
        read.unwrap_or_else(|| {
            answered_in_pieces();
            None
        })
    }

    /// Whether `line`'s root is a Java mod's own command, which any player may send whatever the world allows.
    pub(super) fn java_owns_command(line: &str) -> bool {
        super::java_mods::java_server_started()
            && java_commands(|commands| commands.root(line)) == Some(JavaRoot::Java)
    }
}

#[derive(Default)]
pub(super) struct JavaCommands {
    roots: HashMap<String, JavaRoot>,
    order: Vec<String>,
    pending: Vec<PendingSuggestions>,
}

/// A completion request Java answers at a later tick, by player and request id.
struct PendingSuggestions {
    uuid: u128,
    id: i32,
    text: String,
}

/// Java's completions of a line: the replaced UTF-16 range of the line as sent, and each text with its tooltip component.
#[derive(Debug, PartialEq)]
pub(super) struct JavaSuggestions {
    start: i32,
    length: i32,
    list: Vec<(String, Option<Tag>)>,
}

impl JavaSuggestions {
    fn read(input: &mut In<'_>) -> Option<Self> {
        let start = input.int()?;
        let length = input.int()?;
        let count = usize::try_from(input.int()?).ok()?;
        let mut list = Vec::new();
        for _ in 0..count {
            let text = input.text()?.to_owned();
            let tooltip = if input.bool()? {
                let bytes = input.bytes()?;
                Some(nbt::read_network(bytes, nbt::Limits::default()).ok()?.0)
            } else {
                None
            };
            list.push((text, tooltip));
        }
        Some(Self {
            start,
            length,
            list,
        })
    }
}

/// `command_suggest`'s state byte, with the suggestions of state 1.
#[derive(Debug, PartialEq)]
pub(super) enum SuggestReply {
    Native,
    Now(JavaSuggestions),
    Later,
}

impl SuggestReply {
    fn read(input: &mut In<'_>) -> Option<Self> {
        match input.byte()? {
            0 => Some(Self::Native),
            1 => JavaSuggestions::read(input).map(Self::Now),
            2 => Some(Self::Later),
            _ => None,
        }
    }
}

/// The UTF-16 offset `at` of `text` as a byte offset, clamped to its ends.
fn byte_at(text: &str, at: i32) -> usize {
    let mut units = 0;
    for (byte, c) in text.char_indices() {
        if units >= at {
            return byte;
        }
        units += c.len_utf16() as i32;
    }
    text.len()
}

// Read by the command specs that follow (W6 SPEC-2..4).
#[allow(dead_code)]
impl JavaCommands {
    /// `commands_roots`' reply: a count, then each root's name and kind.
    pub(super) fn read(reply: &[u8]) -> Option<Self> {
        let mut input = In(reply);
        let count = usize::try_from(input.int()?).ok()?;
        let mut commands = Self::default();
        for _ in 0..count {
            let name = input.text()?.to_owned();
            let kind = match input.byte()? {
                1 => JavaRoot::Java,
                2 => JavaRoot::Mixed,
                _ => return None,
            };
            if commands.roots.insert(name.clone(), kind).is_none() {
                commands.order.push(name);
            }
        }
        Some(commands)
    }

    /// The first word of a command line, its slash already stripped.
    pub(super) fn root(&self, line: &str) -> Option<JavaRoot> {
        let word = line.split_ascii_whitespace().next()?;
        self.roots.get(word).copied()
    }

    pub(super) fn names(&self) -> &[String] {
        &self.order
    }

    /// `command_suggestions_take`'s reply: each finished answer's player and request id, with its suggestions.
    fn read_ready(reply: &[u8]) -> Option<Vec<(u128, i32, JavaSuggestions)>> {
        let mut input = In(reply);
        let count = usize::try_from(input.int()?).ok()?;
        let mut ready = Vec::new();
        for _ in 0..count {
            let high = input.long()? as u64;
            let low = input.long()? as u64;
            let id = input.int()?;
            ready.push((
                (u128::from(high) << 64) | u128::from(low),
                id,
                JavaSuggestions::read(&mut input)?,
            ));
        }
        Some(ready)
    }
}

thread_local! {
    static JAVA_COMMANDS: RefCell<JavaCommands> = RefCell::default();
}

pub(super) fn java_commands<T>(f: impl FnOnce(&JavaCommands) -> T) -> T {
    JAVA_COMMANDS.with_borrow(f)
}

pub(super) fn java_commands_reset() {
    JAVA_COMMANDS.with_borrow_mut(|commands| *commands = JavaCommands::default());
}

impl Game<'_> {
    /// Each placed guest's Java roots (or `only` that guest's), as Java's dispatcher gives them to that player; `send` also sends its Java client the permission packet and tree.
    pub(super) fn java_player_commands(&mut self, only: Option<u32>, send: bool) {
        let conns: Vec<u32> = match self.lan.as_ref() {
            Some(host) => host
                .hub
                .guests
                .iter()
                .filter(|g| g.placed && only.is_none_or(|conn| g.conn == conn))
                .map(|g| g.conn)
                .collect(),
            None => return,
        };
        let any = java_commands(|commands| !commands.names().is_empty());
        for conn in conns {
            let tree = if !any {
                None
            } else if self.lan_acting == Some(conn) {
                self.java_commands_for(send)
            } else {
                self.as_guest(conn, |g| g.java_commands_for(send)).flatten()
            };
            if let Some(host) = self.lan.as_mut() {
                host.hub
                    .set_java_commands(conn, tree.unwrap_or(serde_json::Value::Null));
            }
        }
    }

    /// `commands_for` for the acting player: the tree Java gives it, `None` when Java has none or the call failed.
    fn java_commands_for(&mut self, send: bool) -> Option<serde_json::Value> {
        let mut request = Out(self.java_use_request()?);
        request.bool(send);
        let reply = self.java_request("commands_for", &request.0)?;
        let mut reply = In(&reply);
        let text = reply.text()?.to_owned();
        self.java_apply_effects(&mut reply)?;
        if text.is_empty() {
            return None;
        }
        match serde_json::from_str(&text) {
            Ok(tree) => Some(tree),
            Err(e) => {
                eprintln!("Horizon: a player's Java command tree is unreadable: {e}");
                None
            }
        }
    }

    /// Java answers this guest's completion request when its root is Java's: true when it did (now or at a later tick), false leaves it to native.
    pub(super) fn java_suggestions(&mut self, conn: u32, id: i32, text: &str) -> bool {
        let line = text.strip_prefix('/').unwrap_or(text);
        if java_commands(|commands| commands.root(line).is_none()) {
            return false;
        }
        let reply = self
            .as_guest(conn, |g| g.java_suggest_call(id, text))
            .flatten();
        let Some(uuid) = self
            .lan
            .as_ref()
            .and_then(|h| h.hub.guest(conn))
            .map(|g| g.uuid)
        else {
            return false;
        };
        match reply {
            None | Some(SuggestReply::Native) => false,
            Some(SuggestReply::Later) => {
                JAVA_COMMANDS.with_borrow_mut(|commands| {
                    commands.pending.push(PendingSuggestions {
                        uuid,
                        id,
                        text: text.to_owned(),
                    })
                });
                true
            }
            Some(SuggestReply::Now(suggestions)) => {
                self.java_send_suggestions(conn, id, text, suggestions);
                true
            }
        }
    }

    /// `command_suggest` for the acting player; its effects are applied.
    fn java_suggest_call(&mut self, id: i32, text: &str) -> Option<SuggestReply> {
        let mut request = Out(self.java_use_request()?);
        request.int(id);
        request.text(text);
        let reply = self.java_request("command_suggest", &request.0)?;
        let mut reply = In(&reply);
        let state = SuggestReply::read(&mut reply)?;
        self.java_apply_effects(&mut reply)?;
        Some(state)
    }

    /// Native mods see Java's list first, then the packet goes out with Java's own numbers and tooltips.
    fn java_send_suggestions(&mut self, conn: u32, id: i32, text: &str, java: JavaSuggestions) {
        let Some((uuid, name)) = self
            .lan
            .as_ref()
            .and_then(|h| h.hub.guest(conn))
            .map(|g| (g.uuid, g.name.clone()))
        else {
            return;
        };
        let skip = usize::from(text.starts_with('/'));
        let command = &text[skip..];
        let at = |units: i32| byte_at(command, units - skip as i32);
        let range = (at(java.start), at(java.start + java.length));
        let plain = |tag: &Tag| horizon_client::component::name_text(tag).plain();
        let mut suggestions: Vec<_> = java
            .list
            .iter()
            .map(
                |(text, tooltip)| horizon_client_core::suggestion::Suggestion {
                    range,
                    text: text.clone(),
                    tooltip: tooltip.as_ref().map(plain),
                    int_value: None,
                },
            )
            .collect();
        self.native_tab_complete(uuid, &name, command, [range.0, range.1], &mut suggestions);
        suggestions.truncate(1000);
        let list: Vec<_> = suggestions
            .into_iter()
            .map(|s| {
                let kept = java.list.iter().find_map(|(text, tooltip)| {
                    let tooltip = tooltip.as_ref()?;
                    (*text == s.text && s.tooltip.as_deref() == Some(plain(tooltip).as_str()))
                        .then(|| tooltip.clone())
                });
                let tooltip =
                    kept.or_else(|| s.tooltip.as_deref().map(horizon_protocol::text::literal));
                (s.text, tooltip)
            })
            .collect();
        if let Some(host) = self.lan.as_ref() {
            lan_actions::send_suggestions(host, conn, id, java.start, java.length, &list);
        }
    }

    /// The answers Java finished at a later tick, sent to their players.
    pub(super) fn java_suggestions_ready(&mut self) {
        if java_commands(|commands| commands.pending.is_empty()) {
            return;
        }
        let Some(ready) = self
            .java_request("command_suggestions_take", &[])
            .and_then(|reply| JavaCommands::read_ready(&reply))
        else {
            return;
        };
        for (uuid, id, suggestions) in ready {
            let pending = JAVA_COMMANDS.with_borrow_mut(|commands| {
                let at = commands
                    .pending
                    .iter()
                    .position(|p| p.uuid == uuid && p.id == id)?;
                Some(commands.pending.remove(at))
            });
            let conn = self
                .lan
                .as_ref()
                .and_then(|h| h.hub.guests.iter().find(|g| g.uuid == uuid))
                .map(|g| g.conn);
            if let (Some(pending), Some(conn)) = (pending, conn) {
                self.java_send_suggestions(conn, id, &pending.text, suggestions);
            }
        }
    }

    /// Asks the Java server which root commands are its own, once per server start.
    pub(super) fn java_read_commands(&mut self) {
        let table = self
            .java_request("commands_roots", &[])
            .and_then(|reply| JavaCommands::read(&reply));
        if table.is_none() {
            eprintln!("Horizon: the Java server's command roots are unknown");
        }
        JAVA_COMMANDS.with_borrow_mut(|commands| *commands = table.unwrap_or_default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_java_host::wire::Out;

    #[test]
    fn a_reply_names_the_java_roots() {
        let mut out = Out::default();
        out.int(2);
        out.text("mek");
        out.0.push(1);
        out.text("enchant");
        out.0.push(2);
        let commands = JavaCommands::read(&out.0).unwrap();
        assert_eq!(commands.root("mek give x"), Some(JavaRoot::Java));
        assert_eq!(commands.root("enchant @s"), Some(JavaRoot::Mixed));
        assert_eq!(commands.root("give"), None);
        assert_eq!(commands.names(), ["mek", "enchant"]);
    }

    fn verdict(code: u8, replaced: &str) -> Option<Option<JavaVerdict>> {
        let mut out = Out::default();
        out.0.push(code);
        out.text(replaced);
        out.int(7);
        let mut reply = In(&out.0);
        let verdict = read_verdict(&mut reply);
        assert!(verdict.is_none() || reply.int() == Some(7));
        verdict
    }

    #[test]
    fn a_command_reply_tells_what_java_decided() {
        assert_eq!(verdict(1, ""), Some(Some(JavaVerdict::Ran)));
        assert_eq!(verdict(2, ""), Some(Some(JavaVerdict::Stopped)));
        assert_eq!(verdict(0, ""), Some(None));
        assert_eq!(
            verdict(0, "time query day"),
            Some(Some(JavaVerdict::Replaced("time query day".into())))
        );
        assert_eq!(verdict(9, ""), None);
        assert_eq!(read_verdict(&mut In(&[1])), None);
    }

    fn suggestions(out: &mut Out) {
        out.int(1);
        out.int(3);
        out.int(2);
        out.text("give");
        out.bool(false);
        out.text("gamma");
        out.bool(true);
        out.bytes(&[8, 0, 1, b'g']);
    }

    #[test]
    fn a_reply_with_suggestions_reads_range_texts_and_tooltips() {
        let mut out = Out::default();
        out.0.push(1);
        suggestions(&mut out);
        let SuggestReply::Now(read) = SuggestReply::read(&mut In(&out.0)).unwrap() else {
            panic!("state 1 carries the suggestions");
        };
        assert_eq!((read.start, read.length), (1, 3));
        assert_eq!(
            read.list,
            [
                ("give".to_owned(), None),
                (
                    "gamma".to_owned(),
                    Some(horizon_protocol::text::literal("g"))
                )
            ]
        );
    }

    #[test]
    fn a_reply_state_says_who_answers() {
        assert_eq!(
            SuggestReply::read(&mut In(&[0])),
            Some(SuggestReply::Native)
        );
        assert_eq!(SuggestReply::read(&mut In(&[2])), Some(SuggestReply::Later));
        assert_eq!(SuggestReply::read(&mut In(&[3])), None);
    }

    #[test]
    fn finished_answers_name_their_player_and_request() {
        let mut out = Out::default();
        out.int(1);
        out.long(7);
        out.long(-1);
        out.int(9);
        suggestions(&mut out);
        let ready = JavaCommands::read_ready(&out.0).unwrap();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].0, (7u128 << 64) | u128::from(u64::MAX));
        assert_eq!(ready[0].1, 9);
        assert_eq!(ready[0].2.list.len(), 2);
    }

    #[test]
    fn utf16_offsets_map_to_byte_offsets() {
        assert_eq!(byte_at("a\u{1F600}b", 0), 0);
        assert_eq!(byte_at("a\u{1F600}b", 3), 5);
        assert_eq!(byte_at("a\u{1F600}b", 99), 6);
    }

    #[test]
    fn a_short_reply_is_unknown() {
        let mut out = Out::default();
        out.int(2);
        out.text("mek");
        out.0.push(1);
        assert!(JavaCommands::read(&out.0).is_none());
    }
}
