# SPEC-5 (W6): the window's chat input through the Java client (client commands, `ClientChatEvent`)

Design: `DESIGN.md` §1.2, D5, D9. Depends on nothing (client side only). Read first: `G/chat.rs` `handle_chat_input`,
`chat_tree_for`, `with_native_commands`, `system_message`; `G/remote.rs` `send_chat`, the `C::Commands` arm (where the chat tree is
built); `G/java_screens.rs` (`live` module: `java_client_status`, `java_client_apply`, how a synchronous `client_*` op is
called, e.g. `java_click_canceled`); `B/ClientBridge.java` `send`, `sentByWindow`, `OPS`, `status`; vanilla
`ClientPacketListener.sendChat`/`sendCommand`, `ChatComponent.addMessage`, `Hud.setOverlayMessage`. `javap` of NeoForge's
`ClientCommandHandler` and `ClientChatEvent` (signatures). Comments in code: one line at most.

## Goal

1. Enter in the window's chat box runs, in the Java client, the real `ClientPacketListener.sendCommand(cmd)` (so
   `ClientCommandHandler.runCommand` runs a client command) or `sendChat(msg)` (so `ClientChatEvent` may change or cancel it),
   with the packet they would send captured instead. No packet → native sends nothing. A chat packet with other text → native
   sends that text. Otherwise native sends as typed. Native keeps history, normalizing, signing and sending.
2. Everything the Java client adds to its chat or action bar (`ChatComponent.addMessage`, `Hud.setOverlayMessage`: a client
   command's feedback, `displayClientMessage` from mods' client code) shows in native's chat or action bar, and nothing stays in Java's.
3. Client commands' names are in the chat box's tree (white, completed by name, the rest of the line free), as native mods'
   names are. SPEC-8 adds their arguments.
4. A command crosses only when its first word is a client command's root (or W12 routes `ClientPacketListener#sendCommand`);
   every chat line crosses while a Java client is live, since `sendChat` posts `ClientChatEvent` each time (DESIGN D10, M1).

## Java

### J1. `B/ChatSend.java` (new)

```java
/** Packets `ClientBridge.send` keeps instead of sending, while a chat send runs. */
static @Nullable List<Packet<?>> capturing;
/** `client_chat_send`: `text line` (normalized, with its slash for a command); replies `byte verdict` (0 as typed, 1 nothing to send, 2 send `text`), then the chat lines this call produced. */
static byte[] send(DataInputStream in) throws IOException
static boolean capture(Packet<?> packet)   // true when kept
```
`send`, on the Render thread (as every `client_*` op): `capturing = new ArrayList<>()`; `line.startsWith("/")` →
`connection.sendCommand(line.substring(1))`, else `connection.sendChat(line)`; `capturing = null` in `finally`. Then:
- a command: any `ServerboundChatCommandPacket` or `ServerboundChatCommandSignedPacket` captured → 0; none → 1;
- chat: a `ServerboundChatPacket p` captured → `p.message().equals(line)` ? 0 : 2 with `p.message()`; none → 1.
Then `ChatLines.write(out)` (J2) for what the call produced. `ClientBridge.send` starts with `if (ChatSend.capture(packet)) return;`.
Register `client_chat_send` in `ClientBridge.OPS`.

### J2. `B/ChatLines.java` and two mixins

- `B/mixin/ChatComponentMixin.java` (client mixins): `@Inject(method = "addMessage(Lnet/minecraft/network/chat/Component;Lnet/minecraft/network/chat/MessageSignature;Lnet/minecraft/client/multiplayer/chat/GuiMessageSource;Lnet/minecraft/client/multiplayer/chat/GuiMessageTag;)V", at = @At("HEAD"), cancellable = true)`
  → `ChatLines.chat(contents, source)`, cancel. (Get the exact descriptor with `javap -p` of the patched `ChatComponent`.)
- `B/mixin/HudOverlayMixin.java`: `setOverlayMessage(Component, boolean)` HEAD → `ChatLines.overlay(component, animate)`, cancel.
- `ChatLines`: a list of `(byte kind, boolean animate, Component)`; kind 0 client system (`SYSTEM_CLIENT`), 1 server system or
  player, 2 action bar. `write(out)`: `int n` x (`byte kind`, `bool animate`, the component as `GameBridge.writeTag` writes a
  `ComponentSerialization.CODEC` tag); clears.
- `ClientBridge.status` appends `ChatLines.write` (lines from ticks) after its last field.

### J3. Detection, in status

Append to `ClientBridge.status`: `bool namesChanged` + when true `bool routed` (`Routing` has a client route on
`ClientPacketListener#sendCommand`) and `int n` x `text name` (the root children of `ClientCommandHandler.getDispatcher()`).
`namesChanged` is set by a TAIL inject on `ClientPacketListener.handleCommands` (the merge just ran) and at login.

### J4. Test mod (client side, `ClientKeys.java` or a new `ClientChat.java` registered from `TestMod`)

- `RegisterClientCommandsEvent`: `Checks.check("client_commands_registered", () -> true)` and register
  `horizon_java_test_client` → `Checks.check("client_command_runs", () -> ctx.getSource() instanceof ClientCommandSourceStack)`,
  `ctx.getSource().sendSuccess(() -> Component.literal("client pong"), false)`.
- `ClientChatEvent`: message `horizon chat edit` → `setMessage("horizon chat edited")`; `horizon chat cancel` → cancel.

## Rust

### R1. `G/java_chat.rs` (new, `cfg(feature = "java-mods")`)

```rust
pub(super) enum ChatSend { AsTyped, Nothing, Text(String) }
impl Game<'_> {
    /// The Java client's say on a line typed in the window, and the chat lines its code printed meanwhile.
    pub(super) fn java_chat_send(&mut self, line: &str) -> ChatSend;
    /// Chat and action bar lines the Java client printed (status and replies).
    pub(super) fn java_chat_lines(&mut self, lines: Vec<JavaChatLine>);
}
```
- No live Java client, or the gate (R3) says no → `AsTyped` without crossing.
- The crossing is synchronous on the window thread like `java_click_canceled` (wait for a running tick job; log waits over 5 ms).
- `java_chat_lines`: kind 0 → the line native shows for a client system message (find what `ChatComponent.addClientSystemMessage`
  maps to in `G/chat.rs`: `local_chat_line(text, Some(SYSTEM_INDICATOR), ChatSource::Server)` or its remote equivalent); kind 1 →
  `chat_line(text, ChatSource::Server)`; kind 2 → `self.titles.set_overlay(text, animate)` and `say_system_queued`, as
  `C::SystemChat` with `overlay` does in `remote.rs`. Text via `remote::remote_text` of the tag.

### R2. `G/java_hooks.rs` pairs

`java_hook_chat_send(&mut self, line: &str) -> Option<String>` (`None` = send nothing; `Some(text)` = send it; the off arm
returns `Some(line.to_owned())`) and `java_hook_client_command_names(&self, names: &mut Vec<String>)`.

### R3. Gate state (`G/java_screens.rs`, the `CLIENT` thread-local)

Keep `routed: bool` and `client_commands: Vec<String>` from the status. When the names change, rebuild the chat tree the way
the `C::Commands` arm builds it (from the server tree it keeps), so new names join it.

### R4. `G/chat.rs`

- `handle_chat_input`, remote branch, after native mods' `client_mod_command`:
  ```rust
  match self.java_hook_chat_send(&message) { Some(text) => self.send_chat(&text), None => {} }
  return;
  ```
  (replacing the plain `self.send_chat(&message)`). The world-less branch (no `remote`) is unchanged: the window always plays through a server.
- `chat_tree_for`: after `self.native_commands(&mut names)`, `self.java_hook_client_command_names(&mut names)`; they go through
  `with_native_commands`, so a name the server's tree has keeps its server node (vanilla's merge gives the client command
  precedence on run, not on display: note it in the report if SPEC-8 is not there yet).

## Tests

- **Rust** (`java_chat.rs`): the reply reader on hand-written bytes: verdict 0/1/2 and two chat lines (one action bar)
  map to `AsTyped`/`Nothing`/`Text` and the lines' kinds; `java_chat_lines` with an action bar line sets the overlay
  (`self.titles` overlay text equals the line's plain text) and a kind-1 line lands in `self.ui.chat` (use the test `Game`
  builder the other `java_*` tests use, if one exists; else test the line routing as a pure function returning where each goes).
- **Java unit** `ChatSendCheck`: the verdict decision as a static `verdict(String line, List<Packet<?>> captured)` over hand-made
  `ServerboundChatPacket`/`ServerboundChatCommandPacket` objects (constructors need no bootstrap; if one does, test with a stub
  list of types): command + command packet → 0; command + nothing → 1; chat `a` + chat packet `b` → 2/`b`.
- **`java_mods.py bridges`** drive (ctl `chat` must reach `handle_chat_input`; check `G/control.rs`, else use `type` then `key enter`):
  `chat /horizon_java_test_client`, `chat horizon chat edit`, `chat horizon chat cancel`, `wait 5`, `messages 20`. Expected, from the
  test mod's own strings: `client pong` present, no `Unknown or incomplete command` after it, `<name> horizon chat edited` present,
  no line containing `horizon chat cancel`. Required CHECKs: `client_commands_registered`, `client_command_runs`.

## Done bar

- `java_mods.py build`, `unit`, `bridges` green (private jars, gate); `cargo test -p horizon-client --features java-mods --lib java_`;
  clippy with and without the feature; the default build compiles (`java_hooks.rs` off arms).
- A typed vanilla command (`/time query daytime`) makes no `client_chat_send` request; each chat line makes exactly one
  (`HORIZON_JAVA_DEBUG` click-wait log).

## Live test for the lead

Play pack, single player: AE2's, Jade's and guideme's client commands (names from the test mod's absence: type `/` and look for
roots not in SPEC-1's server list; JEI's `jei_internal_show` too) run and print their feedback in chat; normal chat still shows
`<name> text` once. Dedicated server with a joined client: the same client commands run on the client without reaching the server
(the server log shows no `Unknown command` from that player). User steps: type `/` and screenshot the list; run one client
command of AE2 and of Jade; screenshot the chat.
