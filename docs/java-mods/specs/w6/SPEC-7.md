# SPEC-7 (W6): `ServerChatEvent` at vanilla's decoration step

Design: `DESIGN.md` §1.2, D7. Depends on nothing. Read first: `L/hub/chat.rs` (`handle_chat`, `broadcast_chat`, `ChatMessage`,
`native_player_chat`), `L/hub.rs` (`HostEvent`, where `S::Chat` reaches `handle_chat`), `G/lan.rs` (the loop that handles the
hub's `HostEvent`s), `G/native_server.rs` (the function that calls `host.hub.set_mod_commands(0, tree)`: the hub is ready there),
vanilla `ServerGamePacketListenerImpl.handleChat` (1559–1579) and `PlayerChatMessage.withUnsignedContent`, `javap -c` of the
patched `ServerGamePacketListenerImpl.lambda$handleChat$0`, `javap` of `CommonHooks.getServerChatSubmittedDecorator` and
`ServerChatEvent`. Comments in code: one line at most.

## Goal

A player's chat message, after the secure-chat chain took it and before Horizon's chat filter and native mods, goes through
NeoForge's chat decorator in Java (which posts `ServerChatEvent` with the player's view, the raw text and the component), when
a Java server runs (posted listened or not, DESIGN D10): a cancel drops the message for everyone (vanilla: no broadcast, no
chat spam count); a changed component becomes the message's unsigned content (vanilla's `withUnsignedContent(decorated)`, which
keeps it only if it differs from the literal of the signed text). Without Java mods nothing changes and nothing crosses.

## Java

- Op `server_chat` (`B/ModCommands.java` or a new `B/ServerChat.java`): the use request's header (`GameBridge.fill(in, true)`),
  then `text raw`. `var decorated = CommonHooks.getServerChatSubmittedDecorator().decorate(player, Component.literal(raw))` in
  `GameBridge.safely("ServerChatEvent", ...)` (a throw answers 0). Reply `byte` 0 when `decorated` equals `Component.literal(raw)`,
  1 when null, 2 + the component tag; then effects.

## Rust

### R1. The hub (`L/hub.rs`, `L/hub/chat.rs`)

- `Hub` gets `java_chat: bool` and `pub fn set_java_chat(&mut self, on: bool)`.
- `HostEvent::ChatDecorate { conn: u32, message: ChatMessage, bound: play::ChatTypeBound }` (new variant; the `HostEvent`
  `match`es that list every variant get an arm: logging says `chat decoration`).
- Split `handle_chat` after `apply_last_seen`/`try_handle_chat`/unpack/expiry warning: if `self.java_chat`, push
  `ChatDecorate` and return; else continue in `pub fn finish_chat(&mut self, i: usize, message: ChatMessage, events: &mut Vec<HostEvent>)`,
  which holds today's tail (chat filter, native mods, `broadcast_chat`, the `PlayerChat` event).
- `pub fn finish_decorated_chat(&mut self, conn: u32, mut message: ChatMessage, decorated: Option<Option<Tag>>, events: &mut Vec<HostEvent>)`:
  the guest gone → drop; `None` (cancelled) → drop; `Some(Some(tag))` → `message.unsigned_content = Some(tag)`; then `finish_chat`.

### R2. Gameplay (`G/java_chat.rs`, through `java_hooks.rs` pairs)

```rust
/// NeoForge's chat decorator for a guest's message: `None` when cancelled, `Some(None)` unchanged, `Some(Some(tag))` changed.
pub(super) fn java_server_chat(&mut self, host: &mut LanHost, conn: u32, raw: &str) -> Option<Option<Tag>>;
```
In that guest's swap (`as_guest`), `java_use_request()` + `text raw` → `server_chat`, apply effects.
- `G/lan.rs`'s event loop: `HostEvent::ChatDecorate` → `let verdict = self.java_hook_server_chat(host, conn, &message.content);`
  then `host.hub.finish_decorated_chat(conn, message, verdict, &mut more)` and handle `more` as the loop handles events (the
  `HostEvent::Chat` and `PlayerChat` it produces). The off arm returns `Some(None)`.
- Where `host.hub.set_mod_commands(0, tree)` runs (`G/native_server.rs`), also
  `host.hub.set_java_chat(self.java_hook_java_server())` (a `java_hooks.rs` pair: true while the Java server runs, false off).

## Tests

- **Rust** (`L/hub/chat.rs` tests): with `java_chat` on, a `play::Chat` from a guest produces exactly one `ChatDecorate` and no
  broadcast; `finish_decorated_chat` with `Some(Some(tag))` broadcasts a `PlayerChat` whose `unsigned_content` is that tag, with
  `None` broadcasts nothing; with `java_chat` off the packets equal today's (the existing tests keep passing unchanged).
- **Test mod:** a `ServerChatEvent` listener: raw `horizon server edit` → `setMessage(Component.literal("horizon server edited"))`;
  raw `horizon server cancel` → cancel; on every event `Checks.check("server_chat_event_player", () -> e.getUsername().equals(e.getPlayer().getGameProfile().name()))`.
- **`java_mods.py bridges`** drive: `chat horizon server edit`, `chat horizon server cancel`, `wait 5`, `messages 20`: a line ending
  in `horizon server edited` (the window shows the unsigned content, as vanilla's client does), none containing
  `horizon server cancel`. Required CHECK: `server_chat_event_player`.

## Done bar

`cargo test -p horizon-client --lib hub` (and `chat`) pass; `--features java-mods --lib java_` passes; `java_mods.py bridges`
green; clippy both ways; the chat-related harnesses `parity.py --changed-files <L/hub/chat.rs> --list` names still pass
(`verify/parity.sh`, at most 5) and `/home/umceko/hz-poc/verify/lan.sh` with the chat LAN check (`lan_e2e.py --check chat` if it exists).

## Live test for the lead

Showcase (architectury listens) and play pack: two clients on the dedicated server chat both ways; each line shows once on
both, with the sender's name; secure-chat markers unchanged for unchanged messages; `java-ticks.log` (`HORIZON_PROFILE`) shows one
`server_chat` request per message. No user steps.
