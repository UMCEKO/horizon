// game-module-cfg: feature java-mods
//! The window's chat box through the Java client: its client commands and `ClientChatEvent` run there, and the chat lines its code prints show in native's chat.

use super::Game;
use super::java_screens::live::java_client_joined;
use horizon_client::chat::{ForeignCommands, ForeignInfo, ForeignSuggestions};
use horizon_client::command::Suggestion;
use horizon_client::text::{Style, Text};
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

/// Why the Java client's parser reads a root: 1 it is a client command, 2 a mod's argument type sits below it.
const CLIENT_COMMAND: u8 = 1;

thread_local! {
    /// The roots the Java client's parser reads and why, as last asked.
    static NAMES: RefCell<Vec<(String, u8)>> = const { RefCell::new(Vec::new()) };
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

/// The roots `client_command_names` answers: `int n` x (`text name`, `byte why`).
fn read_names(reply: &mut In) -> Option<Vec<(String, u8)>> {
    (0..reply.int()?)
        .map(|_| Some((reply.text()?.to_owned(), reply.byte()?)))
        .collect()
}

fn has_root(command: &str, why: impl Fn(u8) -> bool) -> bool {
    let root = command.split_whitespace().next().unwrap_or("");
    NAMES.with_borrow(|names| names.iter().any(|(name, kind)| name == root && why(*kind)))
}

/// Whether `command`'s first word is a client command of the Java client.
pub(super) fn is_client_command(command: &str) -> bool {
    has_root(command, |why| why == CLIENT_COMMAND)
}

/// `char`s of `text` that make up its first `units` UTF-16 units.
fn chars_of_units(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (chars, c) in text.chars().enumerate() {
        if seen >= units {
            return chars;
        }
        seen += c.len_utf16();
    }
    text.chars().count()
}

/// The byte offset of char `chars` of `text`.
fn byte_of_chars(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map_or(text.len(), |(at, _)| at)
}

/// `int n` x (`text`, `int rgb or -1`, `byte flags`, `text font`), as `ChatBox.runs` writes one usage line.
fn read_line(reply: &mut In) -> Option<Text> {
    let mut line = Text::empty();
    for _ in 0..reply.int()? {
        let piece = reply.text()?;
        let rgb = reply.int()?;
        let flags = reply.byte()?;
        let font = reply.text()?;
        let style = Style {
            color: u32::try_from(rgb).ok(),
            bold: (flags & 1 != 0).then_some(true),
            italic: (flags & 2 != 0).then_some(true),
            underlined: (flags & 4 != 0).then_some(true),
            strikethrough: (flags & 8 != 0).then_some(true),
            obfuscated: (flags & 16 != 0).then_some(true),
            font: (!font.is_empty()).then(|| font.to_owned()),
            ..Style::default()
        };
        line.push(Text::styled(piece, style));
    }
    Some(line)
}

/// What `client_command_info` answers for `value`, as `ChatBox.info` writes it; None when empty (no player yet).
fn read_info(value: &str, reply: &mut In) -> Option<ForeignInfo> {
    let spans = (0..reply.int()?)
        .map(|_| {
            let (start, end) = (reply.int()?, reply.int()?);
            let rgb = u32::try_from(reply.int()?).unwrap_or(0xFFFFFF);
            Some((
                chars_of_units(value, usize::try_from(start).ok()?),
                chars_of_units(value, usize::try_from(end).ok()?),
                rgb,
            ))
        })
        .collect::<Option<Vec<_>>>()?;
    let usage = (0..reply.int()?)
        .map(|_| read_line(reply))
        .collect::<Option<Vec<_>>>()?;
    let usage_position = reply.int()?;
    let usage_width = reply.int()?;
    let suggestions = match reply.byte()? {
        0 => ForeignSuggestions::None,
        1 => {
            let start = chars_of_units(value, usize::try_from(reply.int()?).ok()?);
            let end = chars_of_units(value, usize::try_from(reply.int()?).ok()?);
            let range = (byte_of_chars(value, start), byte_of_chars(value, end));
            let list = (0..reply.int()?)
                .map(|_| {
                    let text = reply.text()?.to_owned();
                    let tooltip = if reply.bool()? {
                        Some(reply.text()?.to_owned())
                    } else {
                        None
                    };
                    Some(Suggestion {
                        range,
                        text,
                        tooltip,
                        int_value: None,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            ForeignSuggestions::Done { start, end, list }
        }
        2 => ForeignSuggestions::Pending,
        _ => return None,
    };
    Some(ForeignInfo {
        spans,
        usage,
        usage_position,
        usage_width,
        suggestions,
    })
}

/// The Java client's parser for the chat box's lines: its client commands and the roots a mod's argument type sits below.
pub(super) struct JavaCommandInfo;

static JAVA_COMMAND_INFO: JavaCommandInfo = JavaCommandInfo;

/// The chat box's other parser while a Java client is joined.
pub(super) fn foreign_commands() -> Option<&'static dyn ForeignCommands> {
    java_client_joined().then_some(&JAVA_COMMAND_INFO as &dyn ForeignCommands)
}

impl ForeignCommands for JavaCommandInfo {
    fn owns(&self, root: &str) -> bool {
        has_root(root, |_| true)
    }

    fn info(&self, value: &str, cursor: usize, width: i32, height: i32) -> Option<ForeignInfo> {
        let host = horizon_java_host::get()?;
        let units = |chars: usize| value.chars().take(chars).map(char::len_utf16).sum::<usize>();
        let mut out = Out::default();
        out.text(value);
        out.int(i32::try_from(units(cursor)).ok()?);
        out.int(width);
        out.int(height);
        // The chat screen holds no game here, so a native the Java client reaches finds none.
        let reply = horizon_java_host::bridge::without_active(|| {
            host.request("client_command_info", &out.0)
        })?;
        read_info(value, &mut In(&reply))
    }
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

    /// The roots the Java client's parser reads, asked of it; empty before it joined.
    fn java_refresh_roots(&mut self) {
        let names = java_client_joined()
            .then(horizon_java_host::get)
            .flatten()
            .and_then(|host| host.request("client_command_names", &[]))
            .and_then(|reply| read_names(&mut In(&reply)))
            .unwrap_or_default();
        NAMES.set(names);
    }

    /// The names of the client commands the Java client holds, asked of it; empty before it joined.
    pub(super) fn java_client_command_names(&mut self) -> Vec<String> {
        self.java_refresh_roots();
        NAMES.with_borrow(|names| {
            names
                .iter()
                .filter(|(_, why)| *why == CLIENT_COMMAND)
                .map(|(name, _)| name.clone())
                .collect()
        })
    }

    /// The status tail `ChatBox.writeStatus` writes: the Java client's tree changed (its roots are asked again), and the completions the open chat box waited for came.
    pub(super) fn java_chat_box_status(&mut self, reply: &mut In) -> Option<()> {
        let ready = reply.bool()?;
        if reply.bool()? {
            self.java_refresh_roots();
        }
        if ready {
            let m = horizon_client::screen_draw::Measure(&self.gui.atlas.font);
            let c = horizon_client::chat::Commands {
                dispatcher: super::chat::chat_tree(&self.remote),
                registries: &self.ui.registries,
                foreign: foreign_commands(),
            };
            if let Some(screen) = self.ui.chat_screen.as_mut() {
                screen.foreign_ready(&m, &c);
            }
        }
        Some(())
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
        NAMES.set(vec![
            ("horizon_java_test_client".into(), CLIENT_COMMAND),
            ("horizon_java_test".into(), 2),
        ]);
        assert!(is_client_command("horizon_java_test_client"));
        assert!(is_client_command("horizon_java_test_client arg"));
        assert!(!is_client_command("horizon_java_test custom"));
        assert!(!is_client_command("time query daytime"));
        assert!(!is_client_command(""));
        assert!(JAVA_COMMAND_INFO.owns("horizon_java_test"));
        assert!(JAVA_COMMAND_INFO.owns("horizon_java_test_client"));
        assert!(!JAVA_COMMAND_INFO.owns("give"));
        NAMES.set(Vec::new());
    }

    #[test]
    fn the_info_reply_reads_into_chars_bytes_and_text_runs() {
        let value = "/a\u{1F600}b c";
        let mut out = Out::default();
        out.int(2);
        for (start, end, rgb) in [(0, 4, 0xAAAAAA), (4, 6, 0x55FFFF)] {
            out.int(start);
            out.int(end);
            out.int(rgb);
        }
        out.int(1);
        out.int(2);
        out.text("<word>");
        out.int(0x555555);
        out.0.push(2);
        out.text("");
        out.text("!");
        out.int(-1);
        out.0.push(0);
        out.text("minecraft:alt");
        out.int(7);
        out.int(40);
        out.0.push(1);
        out.int(5);
        out.int(6);
        out.int(1);
        out.text("b");
        out.bool(true);
        out.text("tip");
        let info = read_info(value, &mut In(&out.0)).unwrap();
        assert_eq!(info.spans, [(0, 3, 0xAAAAAA), (3, 5, 0x55FFFF)]);
        assert_eq!(info.usage.len(), 1);
        assert_eq!(info.usage[0].plain(), "<word>!");
        assert_eq!((info.usage_position, info.usage_width), (7, 40));
        let ForeignSuggestions::Done { start, end, list } = info.suggestions else {
            panic!("done expected");
        };
        assert_eq!((start, end), (4, 5));
        assert_eq!(list[0].range, (value.len() - 2, value.len() - 1));
        assert_eq!(list[0].tooltip.as_deref(), Some("tip"));
        assert!(read_info(value, &mut In(&out.0[..out.0.len() - 1])).is_none());
        assert!(read_info(value, &mut In(&[])).is_none());
    }

    #[test]
    fn units_count_astral_chars_as_two() {
        assert_eq!(chars_of_units("a\u{1F600}b", 3), 2);
        assert_eq!(chars_of_units("a\u{1F600}b", 9), 3);
        assert_eq!(byte_of_chars("a\u{1F600}b", 2), 5);
    }
}
