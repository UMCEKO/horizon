# SPEC-3 (W6): the command tree players get holds Java's roots

Design: `DESIGN.md` §1.1, D3. Depends on SPEC-1 (root table). SPEC-2 is not needed to land this, but without it the roots
complete and don't run. Read first: `L/commands.rs` (`CommandTree::packet_with_features`, `fill`, `properties`), `L/hub.rs`
(`commands_packet`, `send_commands`, `set_mod_commands`, `command_names`, `resend_commands`, the `Guest` struct),
`G/native_server.rs` (`native_filter_commands`, `resend_commands_filtered`, `command_tree`), `G/lan.rs` (the newcomer's
placement: the `native_filter_commands(host, Some(conn))` call and `host.hub.place_new_player`), `G/chat.rs`
(`native_mod_commands_join_the_chat_tree` test), vanilla `Commands.java` (`COMMAND_NODE_INSPECTOR`, `sendCommands`) and
`ArgumentUtils.serializeNodeToJson`. `javap` (signatures) of NeoForge's `CommandHelper` and `CommandTreeCleaner`.
Comments in code: one line at most, only for a non-obvious why.

## Goal

Each player's tree (the `ClientboundCommandsPacket` the hub sends) has, in place of vanilla's same-named roots, the JAVA and
MIXED roots as NeoForge would send them to a vanilla connection for that player: only what that player may use, mods' argument
types cleaned out, with each node's `restricted` flag and suggestion provider as vanilla's inspector gives them. It is
computed when the player is placed and again whenever the hub re-sends trees (a permission change). Then the Java client of that
player also gets vanilla's permission packet and its own full tree, as `PlayerList.sendPlayerPermissionLevel` sends them.

## Java

### J1. `B/mixin/CommandTreeCleanerAccess.java` (new, listed in `horizon_java.mixins.json` common)

`@Mixin(targets = "net.neoforged.neoforge.network.filters.CommandTreeCleaner")` interface with a static `@Invoker("cleanArgumentTypes")`
of `(RootCommandNode, Predicate<ArgumentType<?>>) RootCommandNode`. The class is package-private: an invoker, not reflection.

### J2. `B/ModCommands.java`

```java
/** `commands_for`: the use request's header, then `bool send`; replies `text json` ("" when the player may use none), then effects. */
static void commandsFor(Crossing.In in, Crossing.Out out) throws Exception
/** NeoForge's per-player copy of the roots in `names`, cleaned of mods' argument types, as the report's JSON with the inspector's answers. */
static JsonObject tree(CommandDispatcher<CommandSourceStack> dispatcher, CommandSourceStack source, List<String> names)
/** Adds `restricted` and `suggestions` to `json` for `copy`'s nodes, reading each answer off the original node; drops `permissions`. */
static void inspect(JsonObject json, CommandNode<?> copy, Map<CommandNode<?>, CommandNode<?>> originals, Predicate<CommandNode<?>> restricted, Function<CommandNode<?>, String> suggestions)
```
`commandsFor`: `var call = GameBridge.fill(in, true)`; `boolean send = in.readBoolean()`; if `send`,
`server.getPlayerList().sendPlayerPermissionLevel(call.player)` (vanilla's: the permission entity event and `sendCommands` reach
that player's Java client, as at an op change). Then `tree(..., roots names from SPEC-1)` and reply its JSON text.

`tree`, in this order, each step vanilla's or NeoForge's own code:
1. `map = new HashMap<>(); var copy = new RootCommandNode<>(); map.put(dispatcher.getRoot(), copy);`
   `CommandHelper.mergeCommandNode(dispatcher.getRoot(), copy, map, source, ctx -> 0, SuggestionProviders::cast)` (the patched
   `sendCommands`, DESIGN §1.1).
2. `var clean = CommandTreeCleanerAccess.clean(copy, type -> !modOwned(type))` where `modOwned` is
   `BootFacts.owns(BuiltInRegistries.COMMAND_ARGUMENT_TYPE, BuiltInRegistries.COMMAND_ARGUMENT_TYPE.getKey(ArgumentTypeInfos.byClass(type)))`.
3. `var d = new CommandDispatcher<>(clean)`; for each name in `names` whose `clean.getChild(name)` exists, in `names`' order,
   `children.add(name, ArgumentUtils.serializeNodeToJson(d, child))`. Redirects resolve against the whole cleaned tree, so a
   mod's node redirecting into vanilla's (`execute`) keeps its path.
4. `inspect` each: `restricted` = `!original.getRequirement().test(Commands.createCompilationContext(PermissionSet.NO_PERMISSIONS))`
   (vanilla's `isRestricted`), `suggestions` = `SuggestionProviders.getName(original.getCustomSuggestions())` for argument nodes
   with one (vanilla's `suggestionId`). Originals: invert step 1's `map` (copy → original); the cleaner's output nodes map to
   step 1's nodes by path (walk both trees by child name).
5. Return `{"type":"root","children":{...}}`.
Before writing step 2, check once on the test mod's `custom` node (J3) what the cleaner returns for a node whose type is
dropped (its subtree gone, or the node kept with another type) and write it in the report: the Rust side expects only vanilla parsers.

### J3. Test mod

Register an argument type of its own: `DeferredRegister.create(Registries.COMMAND_ARGUMENT_TYPE, "horizon_java_test")` with
`ArgumentTypeInfos.registerByClass(WordArgument.class, SingletonArgumentInfo.contextFree(WordArgument::new))`, `WordArgument`
a small `ArgumentType<String>` reading one word with `listSuggestions` offering `alpha`, `beta`. Under `horizon_java_test`:
`custom <word:WordArgument>` (says `custom <word>`), and `named <name:StringArgumentType.word()>` with
`.suggests((c, b) -> b.suggest("gamma").buildFuture())` (says `named <name>`). `op` keeps its level-2 requirement.

## Rust

### R1. `L/commands.rs`

- `packet_with_features` gains `java: &Value` (a root in the report's format, `Value::Null` for none). Callers pass `&Value::Null`
  except the hub. Vanilla's root children whose names `java["children"]` has are skipped; then `extra` (native mods) and
  `java` are filled after vanilla's, in that order.
- `fill`: a child with `"restricted"` (bool) uses it for `Node::restricted` instead of `required > 0`; a child with
  `"suggestions"` (string) uses it instead of the `SUGGESTIONS` lookup (the `&'static str` field becomes `Option<String>`, or
  intern the few names through a match on vanilla's four providers plus `ask_server`; a name not among them is `ask_server`, as
  `SuggestionProviders.getName` answers for an unregistered provider).
- An argument node whose parser `builtin.get("minecraft:command_argument_type")` doesn't know is not written (with its subtree)
  and logs once `Horizon: a command node with parser <p> was left out`. Today it would be written as id 0, a wrong tree.

### R2. `L/hub.rs`

- `Guest` gets `java_commands: serde_json::Value` (default `Null`); `pub fn set_java_commands(&mut self, conn: u32, tree: Value)`;
  `pub fn send_commands_to(&mut self, conn: u32)` (sends that placed guest's tree, with `hidden_commands` as `send_commands` does).
- `commands_packet(permission)` becomes `commands_packet(&self, i: usize)` reading the guest's permission and `java_commands`;
  `command_names` and `send_commands` use it.

### R3. `G/java_commands.rs`

```rust
/// Each listed guest's Java roots, as Java's dispatcher gives them to that player (`send`: also its Java client's permission packet and tree).
pub(super) fn java_player_commands(&mut self, host: &mut LanHost, only: Option<u32>, send: bool);
```
For each placed guest (or `only`): with no Java roots in the table, store `Null` and skip Java. Else, in that guest's swap
(`self.lan_acting == Some(conn)` → direct; else `self.as_guest(host, conn, ...)`, as `native_filter_commands` does), build
`java_use_request()` + `bool send`, `java_request("commands_for", ..)`, read `text json`, apply effects, parse the JSON
(`serde_json::from_str`; empty → `Null`), and `host.hub.set_java_commands(conn, tree)`.

### R4. Call sites

- `G/lan.rs`, the newcomer's placement: right after `host.hub.place_new_player(conn, &mut ctx)` (and the level restore after
  it), `self.java_player_commands(host, Some(conn), false)`; if the stored tree isn't `Null`, `host.hub.send_commands_to(conn)`.
  `send` is false: `Menus.login` already sent the Java client its tree. The newcomer's native client therefore gets a second
  tree right after the first when Java roots exist (the Java view only exists once the player is placed; say so in the report).
- `G/native_server.rs` `resend_commands_filtered`: after `native_filter_commands(&mut host, None)`,
  `self.java_player_commands(&mut host, None, true)`, then `resend_commands` as now.
- The two `host.hub.resend_commands()` calls in `G/integrated.rs`: read why each re-sends; if it is a permission or cheats change,
  route it through the same Java refresh (`java_player_commands(.., None, true)` before it); otherwise leave it.
- Each gameplay call goes through a `java_hooks.rs` pair (`java_hook_player_commands`), empty without the feature.

## Tests

- **Rust `L/commands.rs`** (hand-written expected values): a two-root vanilla report (`time` with `query`/`daytime`, `give`) and a
  Java root JSON with `time` (holding `query` and `horizon_java_test_sub`) and `mek` (literal `give`, argument `amount`
  `brigadier:integer` `{"min":1}` with `"suggestions":"minecraft:ask_server"`, literal `admin` with `"restricted":true`):
  the packet has exactly one `time` literal under the root and it has both children; `mek`'s argument has parser id of
  `brigadier:integer` and `suggestions == Some("minecraft:ask_server")`; `admin` is restricted; `give` is untouched. With
  `java = Null` the packet equals today's `packet_with` output for the same report (regression guard). An argument with parser
  `horizon_java_test:word` is absent from the packet.
- **Rust `G/chat.rs`** beside `native_mod_commands_join_the_chat_tree`: decode such a packet into the chat dispatcher
  (the path `remote.rs` uses for `C::Commands`) and check `mek give 3` parses without error and the completions of `"mek "`
  are `admin`, `give`.
- **Java unit `ModCommandsTreeCheck`**: `inspect` on a hand-built literal tree with a copy map: a node whose original's
  requirement refuses a no-permission source gets `"restricted":true`, others `false`, and `permissions` keys are gone. Use
  plain lambdas for the two predicates (no bootstrap).
- **`java_mods.py bridges`**: after SPEC-2's commands, the test mod's client side checks its Java client's tree
  (`Minecraft.getInstance().getConnection().getCommands().getRoot().getChild("horizon_java_test")` has `custom`):
  `java_client_has_mod_tree`. No other new CHECK (the native tree is covered by the Rust tests above).

## Done bar

- `cargo test -p horizon-client --lib commands` (the lib's `lan::commands` tests) and `--features java-mods --lib java_` pass;
  the vanilla-tree harnesses that pin the commands packet still pass: `/home/umceko/hz-poc/verify/parity.sh` with the
  harnesses `tools/parity.py --changed-files <L/commands.rs, L/hub.rs> --list` names (at most 5).
- `java_mods.py unit`, `bridges` green; clippy clean with and without the feature.

## Live test for the lead (play pack, single player, cheats on)

1. Open chat, type `/` : Mekanism's, AE2's and NeoForge's roots are in the list with the vanilla ones, white (known), not red.
2. Type `/neoforge ` : its subcommands complete; `/neoforge generate` shows its arguments up to the first NeoForge enum argument,
   where the line goes on as unknown (cleaned as for a vanilla client; SPEC-8 lifts this in the window).
3. `/deop` yourself on a LAN guest or the dedicated server: that player's level-2 Java subcommands disappear from their completions.
User steps: the same three, with screenshots of the chat box after typing `/`, `/neoforge ` and `/mek `.
