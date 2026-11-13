// game-module-cfg: feature java-mods
// java-listener-family
//! `ClientChatReceivedEvent` on `client_chat_received`: Java mods see and cancel what the chat is about to show.

use super::java_listener::{Family, Subscriptions, crossed, post_client};
use crate::game::api_values::{TryIntoApi, TryIntoEngine};
use crate::game::remote::{ChatBody, event_chat};
use horizon_client::text::Text;
use horizon_core::nbt::{self, Limits, Tag};
use horizon_java_host::wire::{In, Out};
use horizon_protocol::packets::play::ChatTypeBound;
use mod_api::{ClientChatReceivedEvent, ClientHost, Flow, HookId, Listener};

const HOOKS: &[HookId] = &[HookId::ClientChatReceived];
static SUBSCRIBED: Subscriptions = Subscriptions::new();

/// The reply's first byte, as `ChatReceived.java` writes it.
const CANCEL: u8 = 1;
const CHANGED: u8 = 2;

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

    fn client_chat_received(
        &self,
        _: &mut dyn ClientHost,
        event: &mut ClientChatReceivedEvent<'_>,
    ) -> Flow {
        crossed(HookId::ClientChatReceived);
        let Ok(line) = event.message.try_into_engine(&()) else {
            return Flow::Continue;
        };
        let chat = event_chat();
        let has_body = chat.as_ref().is_some_and(|(_, body)| body.is_some());
        let kind = chat_received_kind(event.system, event.overlay, has_body);
        let name = event.chat_type;
        let answer = post_client(
            HookId::ClientChatReceived,
            "ClientChatReceivedEvent",
            |out| write_request(out, kind, &line, event.sender, name, chat.as_ref()),
            read_answer,
        );
        match answer {
            Some(Answer::Cancel) => Flow::Cancel,
            Some(Answer::Changed(text)) => {
                if let Ok(api) = text.try_into_api() {
                    *event.message = api;
                }
                Flow::Continue
            }
            _ => Flow::Continue,
        }
    }
}

/// The kind byte `ChatReceived.java` reads: 0 system, 1 action bar, 2 player, 3 disguised.
fn chat_received_kind(system: bool, overlay: bool, has_body: bool) -> u8 {
    match (system, overlay, has_body) {
        (true, true, _) => 1,
        (true, false, _) => 0,
        (false, _, true) => 2,
        (false, _, false) => 3,
    }
}

fn tag_bytes(tag: &Tag) -> Vec<u8> {
    nbt::write_network(tag, Limits::default()).unwrap_or_default()
}

fn text_tag(text: &Text) -> Tag {
    horizon_client::lan::data::component_nbt(&text.component())
}

/// The request as `ChatReceived.java` reads it; a chat type the registry has no name for is told as unnamed, and Java leaves the line alone.
fn write_request(
    out: &mut Out,
    kind: u8,
    line: &Text,
    sender: u128,
    chat_type: Option<&str>,
    chat: Option<&(ChatTypeBound, Option<ChatBody>)>,
) {
    out.0.push(kind);
    out.bytes(&tag_bytes(&text_tag(line)));
    if kind < 2 {
        return;
    }
    out.text(chat_type.unwrap_or(""));
    let Some((bound, body)) = chat else {
        return;
    };
    out.bytes(&tag_bytes(&bound.name));
    out.bool(bound.target.is_some());
    if let Some(target) = &bound.target {
        out.bytes(&tag_bytes(target));
    }
    if kind == 2 {
        out.long((sender >> 64) as i64);
        out.long(sender as i64);
        if let Some(body) = body {
            write_body(out, body);
        }
    }
}

fn write_body(out: &mut Out, body: &ChatBody) {
    out.int(body.index);
    out.bool(body.signature.is_some());
    if let Some(signature) = &body.signature {
        out.bytes(signature);
    }
    out.text(&body.content);
    out.long(body.timestamp);
    out.long(body.salt);
    out.bool(body.unsigned.is_some());
    if let Some(unsigned) = &body.unsigned {
        out.bytes(&tag_bytes(unsigned));
    }
    out.0.push(u8::from(body.filtered));
}

enum Answer {
    Cancel,
    Changed(Text),
}

/// `None` when Java left the line as it was.
fn read_answer(reply: &mut In) -> Option<Answer> {
    match reply.byte()? {
        CANCEL => Some(Answer::Cancel),
        CHANGED => {
            let (tag, _) = nbt::read_network(reply.bytes()?, Limits::default()).ok()?;
            Some(Answer::Changed(crate::game::remote::remote_text(
                &tag,
            )))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(text: &str) -> Tag {
        Tag::String(text.into())
    }

    #[test]
    fn the_kind_tells_the_four_messages_apart() {
        assert_eq!(chat_received_kind(true, false, false), 0);
        assert_eq!(chat_received_kind(true, true, false), 1);
        assert_eq!(chat_received_kind(false, false, true), 2);
        assert_eq!(chat_received_kind(false, false, false), 3);
    }

    #[test]
    fn a_player_message_is_written_field_by_field_as_java_reads_it() {
        let bound = ChatTypeBound {
            chat_type: 0,
            name: string("Steve"),
            target: Some(string("Alex")),
        };
        let body = ChatBody {
            index: 7,
            signature: Some(vec![9; 256]),
            content: "hello".into(),
            timestamp: 1234,
            salt: -5,
            unsigned: Some(string("hi")),
            filtered: true,
        };
        let sender: u128 = (1u128 << 64) | 2;
        let line = Text::untrusted("<Steve> hello");
        let mut out = Out::default();
        write_request(
            &mut out,
            2,
            &line,
            sender,
            Some("minecraft:chat"),
            Some(&(bound, Some(body))),
        );
        let mut read = In(&out.0);
        assert_eq!(read.byte(), Some(2));
        let tag = |bytes: &[u8]| nbt::read_network(bytes, Limits::default()).unwrap().0;
        assert_eq!(tag(read.bytes().unwrap()), string("<Steve> hello"));
        assert_eq!(read.text(), Some("minecraft:chat"));
        assert_eq!(tag(read.bytes().unwrap()), string("Steve"));
        assert_eq!(read.bool(), Some(true));
        assert_eq!(tag(read.bytes().unwrap()), string("Alex"));
        assert_eq!((read.long(), read.long()), (Some(1), Some(2)));
        assert_eq!(read.int(), Some(7));
        assert_eq!(read.bool(), Some(true));
        assert_eq!(read.bytes(), Some(&[9u8; 256][..]));
        assert_eq!(read.text(), Some("hello"));
        assert_eq!((read.long(), read.long()), (Some(1234), Some(-5)));
        assert_eq!(read.bool(), Some(true));
        assert_eq!(tag(read.bytes().unwrap()), string("hi"));
        assert_eq!(read.byte(), Some(1));
        assert!(read.0.is_empty());
    }

    #[test]
    fn a_system_message_is_its_kind_and_line_alone() {
        let mut out = Out::default();
        write_request(&mut out, 1, &Text::untrusted("bar"), 0, None, None);
        let mut read = In(&out.0);
        assert_eq!(read.byte(), Some(1));
        assert!(read.bytes().is_some());
        assert!(read.0.is_empty());
    }

    #[test]
    fn a_cancel_stops_the_line_and_a_change_replaces_it() {
        assert!(matches!(read_answer(&mut In(&[1])), Some(Answer::Cancel)));
        assert!(read_answer(&mut In(&[0])).is_none());
        assert!(read_answer(&mut In(&[])).is_none());
        let mut out = Out(vec![2]);
        out.bytes(&tag_bytes(&string("horizon recv edited")));
        let Some(Answer::Changed(text)) = read_answer(&mut In(&out.0)) else {
            panic!("expected a changed line");
        };
        assert_eq!(text.plain(), "horizon recv edited");
    }
}
