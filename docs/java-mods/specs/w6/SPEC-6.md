# SPEC-6 (W6): `ClientChatReceivedEvent` in native's receive path

Design: `DESIGN.md` §1.2, D6, D9. Depends on nothing. Read first: `G/remote.rs` (`C::SystemChat`, `C::PlayerChat`,
`C::DisguisedChat` arms; `QueuedChat`; `handle_player_chat`, `accept_chat`, `player_chat_line`, `chat_received`), `G/java_screens.rs`
(how a synchronous `client_*` op is made, `java_click_canceled`), `javap -c` of the patched `ChatListener` (Minecraft's class)
for the hook spots listed in DESIGN §1.2, `javap` of `ClientHooks` (`onClientSystemChat`, `onClientPlayerChat`, `onClientChat`) and
of `ClientChatReceivedEvent` and its `System`/`Player` subclasses. Comments in code: one line at most.

## Goal

Each chat message the window shows passes NeoForge's hook in its Java client, with vanilla's inputs at vanilla's moment, and
native shows what the hook returns, or nothing when a mod cancelled it:
- system message (`SystemChat`, `overlay` false): `onClientSystemChat(component, false)`;
- action bar (`SystemChat`, `overlay` true): `onClientSystemChat(component, true)`;
- player chat: `onClientPlayerChat(bound, decorated, message, sender)`;
- disguised chat: `onClientChat(bound, decorated, Util.NIL_UUID)`.
Native's `chat_received` is already at those spots for native mods (and is documented as this event): Java goes right after
native mods (DESIGN Q4). Validation-error lines are not told to mods (vanilla doesn't either; `player_chat_line` already skips them).

## Java (`B/ChatReceived.java`, op `client_chat_received` in `ClientBridge.OPS`)

Request: `byte kind` (0 system, 1 action bar, 2 player, 3 disguised), the component as a tag (`ComponentSerialization.CODEC`
over NBT, as `Flat` reads a `Component`); for 2 and 3: `text chatType` (registry name), the bound's name tag, `bool` + target
tag; for 2 also `long`×2 sender, `int index`, `bool` + `bytes signature` (256), `text content`, `long timestampMillis`,
`long salt`, `bool` + unsigned content tag, `byte filter` (0 pass-through, 1 fully filtered; native keeps no partial mask).

Build, on the Render thread, from the Java client's registries (`connection.registryAccess()`):
`ChatType.Bound(registry.getOrThrow(key), name, Optional.ofNullable(target))`; for kind 2
`new PlayerChatMessage(new SignedMessageLink(index, sender, sessionId), signature == null ? null : new MessageSignature(bytes), new SignedMessageBody(content, Instant.ofEpochMilli(t), salt, LastSeenMessages.EMPTY), unsigned, filter)`
where `sessionId` is that sender's from the Java client's `PlayerInfo.getChatSession()` when it has one, else `Util.NIL_UUID`
(the window keeps the session, the Java client may not; DESIGN §5 lists the empty last-seen). Call the hook in
`GameBridge.safely("ClientChatReceivedEvent", ...)`. Reply `byte` 0 unchanged (the returned component `equals` the input),
1 cancelled (`null`), 2 changed + the component's tag.

## Rust

### R1. `G/remote.rs`

- `QueuedChat` gains `bound: play::ChatTypeBound` (the packet's; `chat_type` stays) and `body: Option<ChatBody>` with
  `index`, `signature: Option<Vec<u8>>`, `content: String`, `timestamp: i64`, `salt: i64`, `unsigned: Option<Tag>`,
  `filtered: bool`, filled by the `C::PlayerChat` arm from the packet (it already reads all of these). `C::DisguisedChat` and
  `handle_chat_message_error` set `body: None`.
- `chat_received` takes `bound: Option<&play::ChatTypeBound>` and `body: Option<&ChatBody>` too, and after native mods:
  `if !self.java_hook_chat_received(system, overlay, sender, bound, body, line) { return false; }`.
  The two call sites pass what they have (`SystemChat`: `None, None`).

### R2. `G/java_chat.rs` (SPEC-5's module; create it here if SPEC-5 hasn't landed) and `G/java_hooks.rs`

```rust
/// NeoForge's `ClientChatReceivedEvent` in the Java client: false when a mod cancelled it; `line` holds what to show.
pub(super) fn java_chat_received(&mut self, system: bool, overlay: bool, sender: u128, bound: Option<&play::ChatTypeBound>, body: Option<&ChatBody>, line: &mut Text) -> bool;
```
- No live Java client (not joined yet, or none) → true, no crossing.
- Kind: `system && overlay` → 1, `system` → 0, `body.is_some()` → 2, else 3. The chat type's name comes from the remote's
  registries (`registries.name("minecraft:chat_type", id)`, as `chat_received` does for native mods). The component is
  `line.component()` written as NBT (`horizon_client::lan::data::component_nbt` or the helper `remote.rs` uses).
- Synchronous, as `java_click_canceled`. Reply 2 → `*line = remote_text(&tag)`.
- The off arm of `java_hook_chat_received` returns true.

## Tests

- **Rust:** a pure function `chat_received_kind(system, overlay, has_body) -> u8` with the four expected values; the request
  writer for kind 2 on a hand-built `QueuedChat` body produces bytes that the reply reader's twin (a test-only reader mirroring
  the Java layout) reads back field by field; reply 1 → false, 2 → the line's plain text is the new one.
- **Test mod** (client side, registered from `TestMod`): a `ClientChatReceivedEvent` listener:
  - `System` whose text contains `horizon recv cancel` → cancel; contains `horizon recv edit` → `setMessage(Component.literal("horizon recv edited"))`;
  - `System` with `isOverlay()` and text `horizon recv bar` → `Checks.check("chat_received_overlay", () -> true)`;
  - `Player` whose `getPlayerChatMessage().signedContent()` is `horizon recv player` →
    `Checks.check("chat_received_player", () -> p.getSender().equals(Minecraft.getInstance().player.getUUID()))`.
- **`java_mods.py bridges`** drive: `command tellraw @s "horizon recv cancel"`, `command tellraw @s "horizon recv edit"`,
  `command title @s actionbar "horizon recv bar"`, `chat horizon recv player`, `wait 5`, `messages 20`: expect no line with
  `horizon recv cancel`, a line `horizon recv edited` and none that is exactly `horizon recv edit`, a line ending in
  `horizon recv player`. Required CHECKs: `chat_received_overlay`, `chat_received_player`.

## Done bar

`java_mods.py build`, `unit`, `bridges` green; `cargo test -p horizon-client --features java-mods --lib java_` and the lib's
remote/chat tests (`--lib remote`, `--lib chat`) pass; clippy both ways; a session without a Java client is unchanged (no
crossing: the hook returns before asking).

## Live test for the lead

Play pack (JEI listens): chat, `/say hi`, `/tellraw @s "x"`, and a `/title @s actionbar "y"` each show exactly once, as before;
no `a Java mod threw during ClientChatReceivedEvent`. Showcase: the same. With `HORIZON_JAVA_DEBUG=1`, the click-wait log
shows the chat crossings under 5 ms with no tick job running. No user steps beyond reading the chat.
