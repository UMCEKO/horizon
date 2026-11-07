# SPEC-8 (W6): the chat box's parse, usage and completions from the Java client for Java-parsed roots

Design: `DESIGN.md` D3 (what the native tree can't carry), D5, D9. Depends on SPEC-5 (the names in status, the chat send path).
SPEC-3 should be in so server roots exist in the native tree; not required to land. Read first:
`crates/horizon-client-render/src/chat.rs` (`ChatScreen`: `type_text`, `update_command_info`, `server_suggestions`,
`take_request`, `key`, the `Commands` struct passed as `c`), `G/chat.rs` (the `Commands { dispatcher, registries }` sites and
`chat_server_suggestions`), vanilla `CommandSuggestions` (`updateCommandInfo`, `formatText`, `fillNodeUsage`, `updateUsageInfo`)
and `ClientSuggestionProvider.customSuggestion`, `B/Recorder.java`'s TEXT runs writer. Comments in code: one line at most.

## Goal

When the line in the chat box starts with a **Java-parsed root**, the chat box's colours, usage line or error, and completions
come from the Java client's real `CommandSuggestions` over its real dispatcher (the server's full tree merged with client
commands, mods' argument types included), and native draws and handles keys exactly as it does for its own data. A root is
Java-parsed when it is a client command, or its node in the Java client's tree has, anywhere below, an argument whose type's
registry entry is mod-owned (`BootFacts.owns(COMMAND_ARGUMENT_TYPE, ...)`). Every other line stays fully native.
`ask_server` completions for those roots go from the Java client to its server through its own connection (the Java server
answers with SPEC-4's `getCompletionSuggestions` on its view as vanilla's handler does), so puzzleslib's
`ClientSuggestionProvider` patch runs where it should.

## Java

### J1. `B/ChatBox.java` (new) and `B/mixin/CommandSuggestionsAccess.java`

- Accessor/invoker mixin on `CommandSuggestions` (client mixins): `currentParse`, `pendingSuggestions`, `commandUsage`,
  `commandUsagePosition`, `commandUsageWidth` getters, and the static `formatText(ParseResults, String, int)` invoker
  (descriptors from `javap -p` of the patched class).
- `ChatBox` keeps one real `EditBox` (the Java client's font, width from the window's size) and one real
  `CommandSuggestions(minecraft, chatScreen, editBox, font, false, false, 1, 10, true, -805306368)` (the arguments `ChatScreen.init`
  passes; `chatScreen` is a `new ChatScreen("", false)` never opened, for its `width`/`height`). Made at login, dropped at leave.
- Op `client_command_info` (`ClientBridge.OPS`): `text value`, `int cursor` (chars), `int width`, `int height`. Sets the screen
  size and the box's value and cursor (`setValue`, `setCursorPosition`; the responder is not set, so nothing else runs), then
  `suggestions.setAllowSuggestions(true)` and `updateCommandInfo()`. Reply:
  - `int runs` x (`int start`, `int end`, `int rgb`): `formatText(currentParse, value, 0)` read back as spans (walk the
    `FormattedCharSequence` and merge same-style neighbours);
  - `int n` x usage line as TEXT runs (`Recorder`'s run writer: `text`, `int rgb or -1`, `byte flags`, `text font`), then
    `int usagePosition`, `int usageWidth`;
  - `byte suggestions`: 0 none, 1 done + `int start`, `int end` (chars) + `int n` x (`text text`, `bool` + tooltip tag), 2 pending.
- When a pending future completes (vanilla's `thenRun` in `updateCommandInfo`), `ChatBox.ready = true`; status appends
  `bool commandInfoReady` (set once, cleared when written).

### J2. Which roots (status)

Extend SPEC-5's names list: `int n` x (`text name`, `byte why`: 1 client command, 2 mod argument type below it). Computed at
login and in `handleCommands`' TAIL (SPEC-5's inject), over `connection.getCommands().getRoot().getChildren()`.

## Rust

### R1. `horizon-client-render/src/chat.rs`

```rust
/// What another parser (the Java client's) says about the line, in the chat box's own terms.
pub struct ForeignInfo { pub spans: Vec<(usize, usize, u32)>, pub usage: Vec<Text>, pub usage_position: i32, pub usage_width: i32, pub suggestions: ForeignSuggestions }
pub enum ForeignSuggestions { None, Done { start: usize, end: usize, list: Vec<Suggestion> }, Pending }
pub trait ForeignCommands { fn owns(&self, root: &str) -> bool; fn info(&self, value: &str, cursor: usize, width: i32, height: i32) -> Option<ForeignInfo>; }
```
`Commands` gets `pub foreign: Option<&'a dyn ForeignCommands>` (all existing constructions pass `None`). In
`update_command_info`, when the value is a command and `foreign.owns(first word)`: ask `info`; a `None` answer clears the
usage and suggestions; otherwise store the spans (the text colouring uses them instead of the native parse's), `usage`,
`usage_position`, `usage_width`, and `pending` from `Done` (bytes from chars with `byte_at`), or mark it waiting (no native
`request`: the Java client asks its server). Native's own `parse` is `None` for such a line. `pub fn foreign_ready(&mut self, m, c)`
re-runs `update_command_info` when waiting. Keys, the suggestion list, Tab and drawing are unchanged: they read the same fields.

### R2. Gameplay

- `G/java_chat.rs`: `struct JavaCommandInfo` implementing `ForeignCommands` over the `CLIENT` state's Java-parsed names; `info`
  makes the synchronous `client_command_info` crossing (as SPEC-5's send) and converts runs to `Text` with `remote`'s text
  helpers.
- `G/chat.rs`: every `Commands { .. }` built for the chat screen gets `foreign: self.java_hook_foreign_commands()` (a
  `java_hooks.rs` pair returning `Option<&dyn ForeignCommands>`; `None` off or with no Java client).
- When the status says `commandInfoReady`, call `screen.foreign_ready(&m, &c)` on the open chat screen.
- SPEC-5's chat-tree names: only `why == 1` names are added as greedy literals (client commands); `why == 2` roots are already
  in the server tree.

## Tests

- **Rust (render crate)**: a fake `ForeignCommands` owning `mek`: typing `/mek give ` stores its spans and usage and the
  completions from `Done`; typing `/give ` uses the native parse (the fake's `info` is never called: count calls); `Pending` then
  `foreign_ready` with the fake now answering `Done` fills the list. Expected values are the fake's.
- **Java unit** `ChatBoxCheck`: the span reader over a hand-built `FormattedCharSequence` (three styled pieces, two with the same
  colour) gives two merged spans with the expected char ranges and colours.
- **`java_mods.py bridges`**: the test mod's client checks after login that its `custom` root is listed with `why == 2` and
  `horizon_java_test_client` with `why == 1` (`java_parsed_roots`): read from `ChatBox`'s list in a client tick.

## Done bar

`cargo test -p horizon-client-render chat` and `-p horizon-client --features java-mods --lib java_` pass; `java_mods.py unit`,
`bridges` green; clippy both ways (render crate too); with no Java client the chat box makes no crossing (`foreign` is `None`).

## Live test for the lead (user steps: it is input and visual)

Test mod or showcase (puzzleslib, balm, sophisticatedbackpacks have their own argument types; NeoForge's `/neoforge`):
1. Type `/horizon_java_test custom ` (test mod) or `/neoforge generate ` : the usage line shows the mod's argument, and Tab offers
   `alpha`/`beta` (test mod) or the enum's values; colours follow as you type.
2. Type `/horizon_java_test_client` : white, no red error; Enter prints `client pong`.
3. Type `/give @s ` : vanilla's native completions, exactly as before.
Screenshots of 1 and 3.
