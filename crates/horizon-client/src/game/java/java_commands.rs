// game-module-cfg: feature java-mods
//! Which root commands the Java server runs, or holds Java's nodes in, as it told at the server's start.

use super::*;
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

    #[test]
    fn a_short_reply_is_unknown() {
        let mut out = Out::default();
        out.int(2);
        out.text("mek");
        out.0.push(1);
        assert!(JavaCommands::read(&out.0).is_none());
    }
}
