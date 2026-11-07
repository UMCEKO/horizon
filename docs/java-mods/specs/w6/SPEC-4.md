# SPEC-4 (W6): `ask_server` completions for Java's nodes

Design: `DESIGN.md` D4. Depends on SPEC-1 (roots) and SPEC-3 (the client's tree marks Java nodes `ask_server`, so clients ask).
Read first: `G/lan_actions.rs` `guest_try_suggestions`, `guest_suggestions` (the packet it writes), vanilla
`ServerCommandSuggestionsProvider` (`tryProcessRequest`, `limitSuggestionCount`) and `ServerGamePacketListenerImpl.handleCustomCommandSuggestions`,
`ClientboundCommandSuggestionsPacket` (its fields and how a tooltip is written). Comments in code: one line at most.

## Goal

When a client asks for completions (`ServerboundCommandSuggestionPacket`) of a line whose root is JAVA, or MIXED and the
completions come from a Java node, the hub answers with Java's real `dispatcher.getCompletionSuggestions(parse)` for that
player's view, limited to 1000 as vanilla does, in vanilla's packet; native's throttle (one answer per tick per player) stays.

## Java (`B/ModCommands.java`)

```java
/** `command_suggest`: the use request's header, `int id`, `text command` (as the client sent it, slash included); replies `byte state` (0 native answers, 1 here, 2 later), then for 1 the suggestions, then effects. */
static void suggest(Crossing.In in, Crossing.Out out) throws Exception
/** `command_suggestions_take`: the answers finished since; `int n` x (`long uuidHigh`, `long uuidLow`, `int id`, suggestions). */
static byte[] take(byte[] input) throws IOException
/** Whether the completions at the end of this parse come from a node whose code is Java's (D1's owner rule). */
static boolean javaSuggests(ParseResults<?> parse, int cursor, Predicate<String> vanilla)
```
`suggest`: `fill(in, true)`; build the reader exactly as `tryProcessRequest` does (skip one leading `/`); `root` from SPEC-1's
table by the first word; NATIVE → 0. Parse with `player.createCommandSourceStack()`. MIXED and not
`javaSuggests(parse, command.length(), vanilla)` → 0 (`findSuggestionContext(cursor).parent`'s children: any Java child → true).
Else `var f = dispatcher.getCompletionSuggestions(parse)`; done → 1 and the suggestions; not done → keep
`(uuid, id)` and `f.thenAccept(s -> READY.add(...))`, reply 2. Exceptions: log as vanilla does (`Failed to resolve command suggestions for <name>`), reply 0.

Suggestions on the wire: `int start`, `int length` (`Suggestions.getRange()`, UTF-16 indexes of the line as sent, which is what
the packet carries), `int n` (after `limitSuggestionCount`'s 1000) x (`text text`, `bool hasTooltip`, then the tooltip as a
component tag, `GameBridge.writeTag` of `ComponentSerialization.CODEC` over `ComponentUtils.fromMessage(tooltip)`).

`READY` is drained only by `take`, which `Crossing.game("command_suggestions_take", ...)` registers. The `suggest` reply's state 2
tells Rust to ask `take` at the next server ticks until it has answered for that id.

## Rust

### R1. `G/lan_actions.rs`

Split `guest_suggestions`' packet writing into `fn send_suggestions(host: &mut LanHost, conn: u32, id: i32, start: i32, length: i32, list: &[(String, Option<Tag>)])`
(today's code writes text and tooltip; keep its exact bytes). Native's path calls it with its own `utf16(start) + skip` values.

### R2. `G/java_commands.rs`

```rust
/// Java's completions for this request; `None` when native answers.
pub(super) fn java_suggestions(&mut self, host: &mut LanHost, conn: u32, id: i32, text: &str) -> Option<()>;
/// The answers Java finished later, sent to their players.
pub(super) fn java_suggestions_ready(&mut self, host: &mut LanHost);
```
- `java_suggestions`: no Java root for the first word (after an optional `/`) → `None`. Else in the guest's swap
  (`as_guest`), `java_use_request()` + `int id` + `text text` → `command_suggest`. State 0 → `None`; 1 → `send_suggestions` with
  Java's numbers, then `Some(())`; 2 → `JavaCommands.pending += 1`, `Some(())`. Apply effects.
- `java_suggestions_ready`: when `pending > 0`, `command_suggestions_take`; for each answer find the guest by uuid
  (`host.hub.guests`), `send_suggestions`, `pending -= 1`. A guest gone meanwhile: drop it, still count it.
- Native mods' `native_tab_complete` runs on Java's list too, before sending (native mods first, DESIGN Q4): call it with the
  same arguments `guest_suggestions` uses.

### R3. Call sites (through `java_hooks.rs` pairs)

- `guest_suggestions`: first thing after reading `id` and `text`, `if self.java_hook_suggestions(host, conn, id, &text) { return; }`.
- Each server tick after the guests' packets are handled (where `guest_try_suggestions` runs for every guest):
  `self.java_hook_suggestions_ready(host)`.

## Tests

- **Rust** (`java_commands.rs`): the reply reader on hand-written bytes (state 1 with two suggestions, one with a tooltip tag;
  state 2; state 0) gives the expected `(start, length, list)` and pending count. `send_suggestions`' output for
  `id 7, start 1, length 3, ["give", "gamma"(tooltip "g")]` equals bytes written by hand from `ClientboundCommandSuggestionsPacket`'s
  layout (VarInt id, VarInt start, VarInt length, VarInt count, each String text, bool, component) — compare with the packet type
  `horizon_protocol` decodes if it has one.
- **Java unit** (`ModCommandsTreeCheck` or a new `ModCommandsSuggestCheck`): `javaSuggests` on hand-built dispatchers: cursor after
  `c ` where `c` (vanilla) has one Java literal child → true; after `a ` (vanilla, vanilla children only) → false.
- **Test mod / bridges:** the test mod's client tick, once after login, calls its own Java client's
  `getConnection().getSuggestionsProvider().customSuggestion(...)` path for `horizon_java_test named ` (sends
  `ServerboundCommandSuggestionPacket` to the Java server view) and checks the answer lists `gamma`:
  `java_server_suggests_for_mod_node`. This proves Java's provider on a view; the hub path is the Rust tests' and the lead's.

## Done bar

- `cargo test -p horizon-client --features java-mods --lib java_` and `--lib` (the split `send_suggestions`) pass; clippy both ways.
- `java_mods.py unit`, `bridges` green with the new CHECK.
- The vanilla suggestions harness still passes (`parity.py --changed-files ... --list` on `G/lan_actions.rs`, via `verify/parity.sh`, at most 5).

## Live test for the lead (play pack, single player)

1. Type `/horizon_java_test named ` with the test mod, or with the play pack a Mekanism or AE2 subcommand that takes a player or
   a registry value: the completion list shows what that mod suggests (`gamma` for the test mod).
2. Type `/give @s ` : vanilla's item list as before (native).
User steps: screenshots of both completion lists.
