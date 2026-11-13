// game-module-cfg: feature java-mods
//! The window's chat box through the Java client: its client commands and `ClientChatEvent` run there, and the chat lines its code prints show in native's chat.

use super::Game;
use super::java_screens::live::java_client_joined;
use horizon_client::text::Text;
use horizon_java_host::wire::{In, Out};
use std::cell::RefCell;

/// What native sends for a line typed in the window, after the Java client ran its `sendChat` or `sendCommand`.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ChatSend {
    AsTyped,
    Nothing,
    Text(String),
}

/// Where a line the Java client printed goes in native.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LineKind {
    /// `ChatComponent.addClientSystemMessage`.
    SystemClient,
    /// A server system or player line.
    Server,
    ActionBar,
}

/// A chat or action bar line the Java client printed.
pub(super) struct JavaChatLine {
    pub(super) kind: LineKind,
    pub(super) animate: bool,
    pub(super) text: Text,
}

thread_local! {
    /// The roots of the Java client's command dispatcher, as last asked.
    static NAMES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The wire values of `ChatSend.java`'s verdict.
const AS_TYPED: u8 = 0;
const NOTHING: u8 = 1;
const TEXT: u8 = 2;

/// `byte verdict` (and the `text` to send when it is 2), as `ChatSend.send` writes them.
fn read_verdict(reply: &mut In) -> Option<ChatSend> {
    match reply.byte()? {
        AS_TYPED => Some(ChatSend::AsTyped),
        NOTHING => Some(ChatSend::Nothing),
        TEXT => Some(ChatSend::Text(reply.text()?.to_owned())),
        _ => None,
    }
}

/// `int n` x (`byte kind`, `bool animate`, a component tag), as `ChatLines.write` writes them.
pub(super) fn read_lines(reply: &mut In) -> Option<Vec<JavaChatLine>> {
    let mut lines = Vec::new();
    for _ in 0..reply.int()? {
        let kind = match reply.byte()? {
            0 => LineKind::SystemClient,
            1 => LineKind::Server,
            2 => LineKind::ActionBar,
            _ => return None,
        };
        let animate = reply.bool()?;
        let tag = super::java_mods::read_tag(reply.bytes()?)?;
        let text = horizon_client::component::name_text(tag.get("v")?);
        lines.push(JavaChatLine {
            kind,
            animate,
            text,
        });
    }
    Some(lines)
}

/// The names `client_command_names` answers.
fn read_names(reply: &mut In) -> Option<Vec<String>> {
    (0..reply.int()?)
        .map(|_| reply.text().map(str::to_owned))
        .collect()
}

/// Whether `command`'s first word is a root of the Java client's command dispatcher.
pub(super) fn is_client_command(command: &str) -> bool {
    let root = command.split_whitespace().next().unwrap_or("");
    NAMES.with_borrow(|names| names.iter().any(|name| name == root))
}

impl Game<'_> {
    /// The Java client's say on a line typed in the window (with its slash for a command), and the chat lines its code printed meanwhile.
    pub(super) fn java_chat_send(&mut self, line: &str) -> ChatSend {
        if !java_client_joined() {
            return ChatSend::AsTyped;
        }
        let Some(host) = horizon_java_host::get() else {
            return ChatSend::AsTyped;
        };
        self.java_publish_client_world();
        let mut out = Out::default();
        out.text(line);
        let began = std::time::Instant::now();
        let reply = host.request("client_chat_send", &out.0);
        let waited = began.elapsed();
        if waited > std::time::Duration::from_millis(5)
            || std::env::var_os("HORIZON_JAVA_DEBUG").is_some()
        {
            log::info!(
                "Horizon: a chat line waited {:.2} ms on the Java client",
                waited.as_secs_f64() * 1000.0
            );
        }
        let Some(reply) = reply else {
            log::warn!("Horizon: the Java client's chat send failed; the window sends the line as typed");
            return ChatSend::AsTyped;
        };
        let mut reply = In(&reply);
        let Some((verdict, lines)) = read_verdict(&mut reply)
            .and_then(|verdict| Some((verdict, read_lines(&mut reply)?)))
        else {
            log::warn!("Horizon: the Java client's chat send answered in pieces; the window sends the line as typed");
            return ChatSend::AsTyped;
        };
        self.java_chat_lines(lines);
        verdict
    }

    /// Chat and action bar lines the Java client printed (a send's reply, or a tick's status).
    pub(super) fn java_chat_lines(&mut self, lines: Vec<JavaChatLine>) {
        for line in lines {
            match line.kind {
                LineKind::SystemClient => self.chat_line(line.text),
                LineKind::Server => self.system_message(line.text),
                LineKind::ActionBar => {
                    self.say_system_queued(&line.text);
                    self.titles.set_overlay(line.text, line.animate);
                }
            }
        }
    }

    /// The names of the client commands the Java client holds, asked of it; empty before it joined.
    pub(super) fn java_client_command_names(&mut self) -> Vec<String> {
        let names = java_client_joined()
            .then(horizon_java_host::get)
            .flatten()
            .and_then(|host| host.request("client_command_names", &[]))
            .and_then(|reply| read_names(&mut In(&reply)))
            .unwrap_or_default();
        NAMES.set(names.clone());
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(out: &mut Out, kind: u8, animate: bool, literal: &str) {
        use horizon_core::nbt::Tag;
        out.0.push(kind);
        out.0.push(u8::from(animate));
        let tag = super::super::java_mods::write_tag(Tag::compound([(
            "v",
            Tag::String(literal.into()),
        )]))
        .unwrap();
        out.bytes(&tag);
    }

    #[test]
    fn the_verdict_maps_to_what_native_sends() {
        let mut out = Out::default();
        out.0.push(AS_TYPED);
        assert_eq!(read_verdict(&mut In(&out.0)), Some(ChatSend::AsTyped));
        assert_eq!(read_verdict(&mut In(&[NOTHING])), Some(ChatSend::Nothing));
        let mut out = Out::default();
        out.0.push(TEXT);
        out.text("edited");
        assert_eq!(
            read_verdict(&mut In(&out.0)),
            Some(ChatSend::Text("edited".into()))
        );
        assert_eq!(read_verdict(&mut In(&[9])), None);
    }

    #[test]
    fn lines_keep_their_kind_and_text() {
        let mut out = Out::default();
        out.int(2);
        line(&mut out, 0, false, "client pong");
        line(&mut out, 2, true, "bar");
        let lines = read_lines(&mut In(&out.0)).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].kind, LineKind::SystemClient);
        assert_eq!(lines[0].text.plain(), "client pong");
        assert_eq!(lines[1].kind, LineKind::ActionBar);
        assert!(lines[1].animate);
        assert_eq!(lines[1].text.plain(), "bar");
        assert!(read_lines(&mut In(&out.0[..out.0.len() - 1])).is_none());
    }

    #[test]
    fn a_command_crosses_only_for_a_client_root() {
        NAMES.set(vec!["horizon_java_test_client".into()]);
        assert!(is_client_command("horizon_java_test_client"));
        assert!(is_client_command("horizon_java_test_client arg"));
        assert!(!is_client_command("time query daytime"));
        assert!(!is_client_command(""));
        NAMES.set(Vec::new());
    }
}
