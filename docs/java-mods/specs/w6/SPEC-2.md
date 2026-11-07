# SPEC-2 (W6): mods' commands run in Java, and `CommandEvent`

Design: `DESIGN.md` §1.1, D1, D10. Depends on SPEC-1 (root table, `horizon_java_test` root). Read first: `RULES.md`,
`docs/java-mods-map.md` §3 ("Entry and views", "Firing events") and §6, `G/commands.rs` `run_command`,
`G/lan_actions.rs` `guest_command` and `run_guest_command`, `G/chat.rs` `as_console`, `G/dedicated.rs` `console_command`,
`B/GameBridge.java` `fill` and `use`, `G/java_mods.rs` `java_call_use`, `java_apply_use`, `java_level_call`.
`javap -c net.minecraft.commands.Commands` (the patched Minecraft class, allowed) for `performCommand`'s `CommandEvent` steps.
Comments in code: one line at most, only for a non-obvious why.

## Goal

- A player's or the console's command whose root is JAVA runs in Java's real `Commands.performPrefixedCommand` with that
  player's view (or the server's) as the source; its feedback reaches the player through the bridged `sendSystemMessage`, and
  what it does to the player's inventory and experience comes back as after a use.
- A MIXED root: Java parses; Java runs it when the executing node's code is Java's (SPEC-1's `javaNode`), else native does.
- Every player and console command posts `CommandEvent` once, as the patched `performCommand` does (DESIGN D10: listened or
  not, M1 counts it): a cancel stops the command (with the event's exception reported as vanilla reports it), a replaced parse
  runs its text. One crossing per command; commands are human-rate.

## Java

### J1. Op `command` (player) and `console_command` (server), in `B/ModCommands.java`

Refactor first, no behaviour change: move `GameBridge.use`'s writing of experience and changed slots (from
`out.writeInt(player.experienceLevel)` through the slot list) into `static void writeChanges(Crossing.Out out, Call call)`;
`use` calls it.

```java
/** `command`: the use request's header (`fill(in, false)`), then `text command`, `byte root` (SPEC-1 kind), `bool event`. */
static void command(Crossing.In in, Crossing.Out out) throws Exception
/** `console_command`: the level header `java_level_call` writes, then `text command`, `byte root`, `bool event`. */
static void console(Crossing.In in, Crossing.Out out) throws Exception
/** The shared decision on one source; returns the reply code and sets `replaced[0]` when a listener replaced the parse. */
static byte perform(CommandSourceStack source, String command, byte root, boolean event, String[] replaced)
```
`perform` (all inside `GameBridge.safely`; a throw answers 0, native runs):
1. `var commands = server.getCommands(); var dispatcher = commands.getDispatcher();`
   `var text = Commands.trimOptionalPrefix(command);`
2. `root == JAVA` → `commands.performPrefixedCommand(source, command)` → 1. (It posts `CommandEvent` itself.)
3. `var parse = dispatcher.parse(text, source);`
4. `root == MIXED` and the parse has no exceptions, read to the end, and the last `CommandContextBuilder` child's command is
   Java's (`ModCommands.javaNode` over the last parsed node, or `owner(command)` not vanilla) → `commands.performCommand(parse, text)` → 1.
5. `event` → `var e = new CommandEvent(parse); NeoForge.EVENT_BUS.post(e);` exactly as the patched `performCommand` does at
   offsets 11–83: if canceled, report `e.getException()` the way that bytecode does (copy its calls; when there is no exception,
   nothing is said) → 2; if `e.getParseResults() != parse` → `replaced[0] = e.getParseResults().getReader().getString()` → 0.
6. → 0.

`command`: `var call = GameBridge.fill(in, false)`, then read the tail, `perform(call.player.createCommandSourceStack(), ...)`,
then reply `byte code`, `text replaced` ("" when none), `GameBridge.writeChanges(out, call)`; effects follow as for `use`
(the `game` wrapper writes them at depth 1).
`console`: as `java_level_call`'s ops read their header (copy `level_tick`'s reading), source `server.createCommandSourceStack()`,
reply `byte code`, `text replaced`, then effects.
Register both in `GameBridge.register` (`Crossing.game("command", ModCommands::command)` and `"console_command"`).

### J2. Test mod (`TestMod.java`, the `RegisterCommandsEvent` listener of SPEC-1)

Under `horizon_java_test`:
- `ping` (no requirement): `Checks.check("mod_command_runs_in_java", () -> source.getEntity() instanceof ServerPlayer)` when the
  source is a player; `source.sendSuccess(() -> Component.literal("pong " + source.getTextName()), false)`.
- `op` with `.requires(Commands.hasPermission(Commands.LEVEL_GAMEMASTERS))`: says `op ok`.
- `refuse`: says `refuse ran`.
- `give_probe`: `source.getPlayerOrException().getInventory().add(new ItemStack(Items.DIAMOND))`.
And `literal("time").then(literal("horizon_java_test_sub").executes(... says "sub ran"))`, which makes vanilla's `time` MIXED.

A `CommandEvent` listener: cancels when the input is `horizon_java_test refuse`; when the input is `time query daytime`,
`Checks.check("command_event_for_native", () -> e.getParseResults().getContext().getSource().getEntity() instanceof ServerPlayer)`.

## Rust

### R1. `G/java_mods.rs`

- `fn java_level_request(&mut self, op: &str, body: impl FnOnce(&mut Out)) -> Option<Vec<u8>>`: `java_level_call`'s header and
  request without applying effects; `java_level_call` becomes a call of it plus `java_apply_effects`.
- Split `java_apply_use`'s experience and slot reading into `fn java_apply_changes(&mut self, reply: &mut In<'_>) -> Option<()>`
  (the mirror of `writeChanges`); `java_apply_use` calls it.

### R2. `G/java_commands.rs`

New:
```rust
/// What Java decided for a command line: `None` when native runs it as typed.
pub(super) enum JavaVerdict { Ran, Stopped, Replaced(String) }
impl Game<'_> {
    pub(super) fn java_command(&mut self, command: &str) -> Option<JavaVerdict>;
}
```
- No Java server running (`JAVA_SERVER.started` false) → `None`. Look up `root = java_commands(|c| c.root(command))`; the
  request's `event` is always true (D10).
- Source: `self.command_source` with a `block` or `cart` → `None` (out of scope, DESIGN §5). `self.console_broadcast.is_some()` →
  `console_command` through `java_level_request`. `self.lan_acting.is_some()` → `command`: `java_use_request()` + tail through
  `java_request`, then `java_apply_changes`, then `java_apply_effects`, then `java_drain_block_events`. Anything else → `None`.
- Reply code 1 → `Ran`, 2 → `Stopped`, 0 with a non-empty text → `Replaced`, 0 → `None`. An unreadable reply logs once
  (`Horizon: a Java command answered in pieces`) and is `None`.

### R3. `G/java_hooks.rs`

```rust
/// A Java mod's command, or `CommandEvent` for a native one: `Some(ok)` when Java ran or stopped it.
pub(super) fn java_hook_command(&mut self, command: &str) -> Result<Option<bool>, String>
```
`Err(text)` carries a replaced command; the `cfg(not(feature = "java-mods"))` arm returns `Ok(None)`.

### R4. `G/commands.rs` `run_command`

After `client_mod_command`, before the native dispatcher:
```rust
let replaced;
let command = match self.java_hook_command(command) {
    Ok(Some(ok)) => return ok,
    Ok(None) => command,
    Err(text) => { replaced = text; replaced.as_str() }
};
```
(`Ran` → `Some(true)`, `Stopped` → `Some(false)`.) Then the existing body, on `command`.

### R5. `G/lan_actions.rs` `guest_command`

Before the "Allow Commands" refusal, as native mods' commands do: a line whose root is in the Java table is queued as a
`GuestCommand` at the guest's own permission whatever the world allows (vanilla checks the node's requirement, not the cheat
setting, for a non-op). Keep `native_player_command` first (DESIGN Q4).

## Tests

- **Java unit** `ModCommandsPerformCheck` is not possible without a server; the decision logic of steps 3–4 lives in a static
  `static boolean javaRuns(ParseResults<?> parse, Predicate<String> vanilla)`; check it on hand-built dispatchers as SPEC-1's check
  does: a parse ending in a check-class command → true; ending in a `Vanilla` command under the same root → false; a parse with
  an error → false. `ModCommandsPerform ok`.
- **Rust unit** (`java_commands.rs`): the reply reader maps codes 0/1/2 and a replaced text as above, on hand-written bytes;
  `java_apply_changes` reads what `java_apply_use` read before (move its existing test or add one with two changed slots).
- **`java_mods.py bridges`** drive, after the wand checks (horizon-ctl `command` goes through the window's chat path, so this
  is the real network route): `command horizon_java_test ping`, `command horizon_java_test op`, `command horizon_java_test refuse`,
  `command time horizon_java_test_sub`, `command time query daytime`, `command horizon_java_test give_probe`, `wait 5`, then
  `messages 20` and `inventory`. Expect, independently of the code: `pong Player...` (the player's name from `state`), `op ok`
  (the bridges world allows commands, level 2+), no `refuse ran`, `sub ran`, vanilla's `commands.time.query` line from native
  (`The time is <n>`), and one more `minecraft:diamond` in the inventory than before. Required CHECKs:
  `mod_command_runs_in_java`, `command_event_for_native`, `command_event_once` (add to the list in `bridges`).

## Done bar

- `java_mods.py build`, `unit` (both checks ok), `check` (no new problems), `bridges` all green through the gate, private jars.
- `cargo test -p horizon-client --features java-mods --lib java_` passes; clippy clean with and without the feature.
- Exactly one `CommandEvent` per command: the test mod's listener counts the `time query daytime` posts and checks 1
  (`command_event_once`, required in `bridges`).

## Live test for the lead

Play pack, single player with cheats:
1. `/mek` then Tab in the chat (completion comes with SPEC-3; here only run): `/mek debug` (or another root from SPEC-1's debug
   line) runs and its feedback shows in chat; no "Unknown or incomplete command".
2. `/ae2` subcommands from the same line run and answer in chat.
3. `/neoforge tps` answers (numbers may be zero: the view's tick times are Java's).
4. `/give @s minecraft:stone` still behaves exactly as before (native).
Dedicated server (play pack, `java_mods.py launch --dedicated`): type `neoforge mods` in the server console: the list is printed
in the server's log. A joined client runs `/mek ...` as above.
