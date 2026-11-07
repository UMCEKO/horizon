# W6: mods' commands, chat and toasts

Workstream W6 of COVERAGE-SPEC §3. Scope from the brief: `RegisterCommandsEvent` commands dispatched through Java's Brigadier from
Horizon's command path (server and client commands, suggestions, the tree clients get), the chat events (`ServerChatEvent`,
`ClientChatEvent`, `ClientChatReceivedEvent`, plus `CommandEvent`, which vanilla posts on the same path), and mods' toasts drawn
natively.

Base: `java-mods` at cc1b066b6. Paths as `docs/java-mods-map.md`: `B/` = `mods/horizon/src/java-mods-host/boot/dev/horizonlab/javamods/boot/`,
`G/` = `crates/horizon-client/src/game/`, `L/` = `crates/horizon-client/src/lan/`, `T/` = `tools/`, `J/` = `mods/horizon/src/java-mods-host/`.
Line numbers drift (other agents edit `hub.rs`, `advancements.rs` today): find code by the function names given.

## 1. The gap, with evidence

No build and no harness ran. The one allowed live boot (`live.sh start w6`, `LIVE_STATE`/`LIVE_JAR` under this folder, JFR
method trace on the entry points below) died at window creation for environment reasons: `winit ... Failed to load one of
xlib's shared libraries` (crash report `/tmp/hz-poc/p/live-w6/HORIZON_HOME/crash-reports/crash-2026-11-06_18.04.19-client.txt`;
`DISPLAY=:99 WAYLAND_DISPLAY=` pushes winit to X11, and `live.sh` puts no libX11 on the path). Per RULES it was not retried; the
game is gone. Everything below is from `javap` of the installs' jars (`install-rs/libraries/.../minecraft-client-patched-26.3.0.101.jar`,
`neoforge-26.3.0.101-universal.jar`, the mods in `agents/w12/install/mods`), the `replaces` reports in `agents/finish/rep-*`, and the code.

### 1.1 Server commands

| Fact | Evidence |
|---|---|
| Mods' commands **are** registered in Java already: `GameBridge.prepare` loads the data packs through `WorldLoader` → `ReloadableServerResources` → `new Commands(selection, ctx)`, and the patched constructor posts `RegisterCommandsEvent` (`EventHooks.onCommandRegister`, `javap -c net.minecraft.commands.Commands`, `<init>` offset 806). STATUS's "posted only for a probe command (`Boot.java:246`)" is incomplete: `Boot.listen`/`event` is a second, probe-only post. | `B/GameBridge.java` `prepare` (`WorldLoader.InitConfig(packs, Commands.CommandSelection.DEDICATED, ...)`) |
| The selection is always `DEDICATED`, also on a client's integrated server; NeoForge's integrated server builds with `INTEGRATED`, and mods may register per selection (`RegisterCommandsEvent.getCommandSelection`). | same line |
| Nothing dispatches to that dispatcher. A typed command runs `Game::run_command` (`G/commands.rs`): native mods' `client_mod_command`, then `vanilla::game_dispatcher_with_features`. A guest's goes `guest_command` (`G/lan_actions.rs`) → native mods' levels → `run_guest_command` → `run_command`. `/mek ...` ends in `command.unknown.command`. | `G/commands.rs` `run_command`, `G/lan_actions.rs` `guest_command` |
| The tree players get is vanilla's report plus native mods' (`L/hub.rs` `commands_packet` → `L/commands.rs` `packet_with_features(..., &self.mod_commands.0, ...)`, filled by `G/native_server.rs` `command_tree`). No Java command is in it, so the chat box marks mod commands red and completes nothing. | `L/hub.rs` `commands_packet`, `set_mod_commands` |
| An unknown parser in an extra root would be written as argument type id 0 (`brigadier:bool`): `argument_types.and_then(|t| t.id(parser)).unwrap_or(0)`, and `properties()` knows vanilla parsers only. Mods' argument types can't go into the native tree as they are. | `L/commands.rs` `packet_with_features`, `properties` |
| Mods with their own argument types (`Registries.COMMAND_ARGUMENT_TYPE` writes): guideme (AE2's dependency, in the play pack), balm, creativecore, puzzleslib, sophisticatedbackpacks, and NeoForge itself (`neoforge:enum`, `neoforge:modid` for `/neoforge`). NeoForge strips these for a vanilla connection with `CommandTreeCleaner.cleanArgumentTypes(RootCommandNode, Predicate<ArgumentType<?>>)` (`javap net.neoforged.neoforge.network.filters.CommandTreeCleaner`). | constant-pool scan of every showcase jar (`/tmp/w6scan/scan.py`, rerunnable) |
| NeoForge's `sendCommands` (patched, `javap -c`): `CommandHelper.mergeCommandNode(dispatcher.getRoot(), root, map, player.createCommandSourceStack(), ctx -> 0, SuggestionProviders::cast)`, then `new ClientboundCommandsPacket(root, COMMAND_NODE_INSPECTOR)`; the inspector's `restricted` is `!requirement.test(createCompilationContext(NO_PERMISSIONS))` and a node's suggestions `SuggestionProviders.getName(getCustomSuggestions())`. | vanilla `Commands.java` 170–189, 417–423 and the patched bytecode |
| `CommandEvent` is posted by the patched `Commands.performCommand` (offsets 11–83) for every command run through it, vanilla's included; cancel and `getException` stop it, `getParseResults` may replace the parse. Horizon posts it never. | `javap -c net.minecraft.commands.Commands` |
| Permissions are already Horizon's: `MinecraftServer.getProfilePermissions` and `PlayerList.isOp` are bridged (`G/java_bridges/server.rs`), so `player.createCommandSourceStack()` on a view carries native's level. | `G/java_bridges/server.rs` |

### 1.2 Client commands and chat

| Fact | Evidence |
|---|---|
| The Java client gets the real tree: `Menus.login` calls `players.sendPlayerPermissionLevel(player)` whose last step is `server.getCommands().sendCommands(player)` (vanilla `PlayerList.java` 568–581), and the patched `ClientPacketListener.handleCommands` merges client commands (`ClientCommandHandler.mergeServerCommands`, offset 49). `ClientCommandHandler.handleClientPlayerLogin(LoggingIn)` exists and `LoggingIn` fires in the Java client's `handleLogin`, so `RegisterClientCommandsEvent` is expected to fire (to confirm by SPEC-5's first check: the live trace could not run). A permission change never re-sends it (no Java call at op/deop). | `B/Menus.java` `login`; `javap ClientCommandHandler` |
| Client commands never run: `ClientCommandHandler.runCommand` is called only from the patched `ClientPacketListener.sendCommand` and `sendUnattendedCommand`, and nothing in the host calls those. The window's chat goes `G/chat.rs` `handle_chat_input` → native mods' `client_mod_command` → `send_chat` (native packet). | `javap -c ClientPacketListener` (`sendCommand` offset 1); grep of `B/` |
| `ClientChatEvent` never fires: it is posted from the patched `ClientPacketListener.sendChat` (`ClientHooks.onClientSendMessage`, offset 1). | same |
| The Java client receives no chat packets: a mod's `ServerPlayer.sendSystemMessage` is bridged to native (`G/java_bridges/world.rs` `player_system_message`), and native relays chat. So `ClientChatReceivedEvent`, posted inside `ChatListener` (`onClientSystemChat` at `handleSystemMessage` offset 41 and `handleOverlay` 2; `onClientPlayerChat` at `showMessageToPlayer` 122 and 196; `onClientChat` in `handleDisguisedChatMessage`'s lambda 38), never fires. Native already has the seam: `G/remote.rs` `chat_received` (native mods' `native_client_chat_received`, documented as NeoForge's event) at the matching points. | `javap -c ChatListener` |
| Client-side chat lines mods print (`ClientCommandSourceStack.sendSuccess`, `LocalPlayer.displayClientMessage` from client code) land in the Java client's `ChatComponent`, which nobody draws. | Java client structure (`ClientBridge` cancels drawing) |
| `ServerChatEvent` never fires: the patched `ServerGamePacketListenerImpl.lambda$handleChat$0` takes `CommonHooks.getServerChatSubmittedDecorator()` in place of `server.getChatDecorator()` (vanilla `handleChat` 1559–1579: decorate after unpack, before the filter, `withUnsignedContent(decorated)`); Horizon's `L/hub/chat.rs` `handle_chat` calls only native mods' `native_player_chat`. | `javap -c ServerGamePacketListenerImpl` |

### 1.3 Toasts

| Fact | Evidence |
|---|---|
| Mods add toasts with the real `ToastManager.addToast` (patched: `ClientHooks.onToastAdd` → `ToastAddEvent`, then `queued.add`). Finish-set users: AE2 `FinishedJobToast` (crafting done), RS `TaskCompletedToast` and `ClientPlatformUtil` (`SystemToast`), Jade `JadeClient` (`SystemToast`); top-50: lootr `LootrToast`, veinminer `SystemToast`s, JEI (`ScreenHelper` reads the toast area), puzzleslib. None overrides `xPos`/`yPos` (`javap` of the three toast classes). | `/tmp/w6scan/toasts.py` over the install jars |
| `ToastManager.update()` (slots, slide, sounds) is called only by `Gui.update()` ← `Minecraft.runTick` (vanilla `Minecraft.java` 1272); the Java client runs `minecraft.tick()` alone (`B/ClientBridge.java` `tick`). So every mod toast sits in `queued` forever: never shown, and the deque only grows. | vanilla source; `ClientBridge.tick` |
| Native owns vanilla's toasts with one slot mask shared by every kind: `G/advancements.rs` `toast_slots_used` (advancement, recipe, pack, narrator, tutorial toasts), `draw_toasts`, `horizon_client::toasts::Slide`. | code |

### 1.4 Who listens (static, `agents/finish/rep-{rs,showcase}/replaces.json`)

| Event | Play pack (install-rs) | Showcase |
|---|---|---|
| `RegisterCommandsEvent` | ae2, buildinggadgets2, guideme, jade, mekanism | + architectury, balm, carryon, creativecore, glitchcore, lootr, sophisticatedbackpacks, travelersbackpack, veinminer |
| `RegisterClientCommandsEvent` | ae2, guideme, jade (and JEI's `JeiInternalShowCommand`, registered by bus call) | + architectury, balm, creativecore, travelersbackpack |
| `CommandEvent` | none | architectury, balm |
| `ServerChatEvent` | none | architectury |
| `ClientChatEvent` | none | architectury |
| `ClientChatReceivedEvent` | JEI (`JeiChatEventHandler`) | + architectury, puzzleslib |
| `ToastAddEvent` | none | puzzleslib |

Mixins on W6 targets (`routing.tsv`, both packs): puzzleslib `EnchantCommand.register` (HEAD, cancellable: runs in Java when
`Commands` is built, so it already executes; its effect is which `/enchant` node exists, handled by D1), puzzleslib
`ClientSuggestionProvider.getSelectedEntities` (client; runs once SPEC-8 asks the Java client), balm
`ReloadableServerResources.loadResources` (RETURN; runs in `prepare`). Nothing patches `ChatComponent`, `ChatScreen`,
`CommandSuggestions`, `ChatListener`, `ToastManager` or a command's execute method. M2 for W6 is therefore met once D1 and SPEC-8 land.

## 2. The approach

Horizon already has a native-mod hook at every place this needs one (`client_mod_command`, `native_player_command`,
`native_tab_complete`, `native_player_chat`, `chat_received`, `with_mod_commands`, `native_filter_commands`). Java goes beside each,
in vanilla's order, gated by what the boot or the bus says is there. Native keeps vanilla's commands, the chat box, chat display,
signing, and toast placement; Java keeps the mods' code and the vanilla code that mods' code is called through.

```
window chat box ── Enter ──> SPEC-5 Java client: real sendCommand/sendChat (client commands, ClientChatEvent), packet captured
        │                     └─ not consumed ──> native send (signed)
        ├─ typing a Java-parsed root ──> SPEC-8 Java client: parse, usage, completions (its own ask_server via horizon:java)
        └─ typing anything else ──> native parse over the hub's tree (SPEC-3 adds Java roots) ; ask_server ──> SPEC-4
hub (server) ── command ──> run_command ──> SPEC-2: Java root or CommandEvent listened? ──> Java performPrefixedCommand
             ── chat ──> SPEC-7 ServerChatEvent at decoration ──> native broadcast
window receive ── SystemChat/PlayerChat/Disguised ──> chat_received ──> SPEC-6 ClientChatReceivedEvent ──> native chat
Java client ── ToastManager.addToast ──> SPEC-9 native slot + slide; Java update()+drawing recorded
```

## 3. Decisions (with reasons)

- **D1. Who runs a command: the code at the node that executes.** At server start Java classifies each root child of the real
  dispatcher. A node's code is its `getCommand()`, `getRequirement()`, `getRedirectModifier()`, an argument node's
  `getCustomSuggestions()` and `getType()`; its owner is the class that declares it (a lambda's hidden class name up to `$$Lambda`).
  Vanilla = `net.minecraft.` or `com.mojang.brigadier.`; anything else (a mod, NeoForge, Horizon's test mod) is Java's.
  Roots: **JAVA** (the root's own node is Java's, e.g. `/mek`, `/neoforge`, or puzzleslib's replacement `/enchant`), **MIXED**
  (a vanilla root with some Java node under it, a mod's subcommand merged into a vanilla literal), others native. A JAVA root
  always runs in Java; a MIXED one is parsed by Java and runs in Java only if the parse's executing command is Java's. Reason:
  decided from registries and classes, never names (rule 4); vanilla commands stay native even though Java's dispatcher has
  them (rule 0), because Java's views don't own vanilla state (game rules and the border are Java's defaults).
- **D2. `CommandSelection` follows the dist**: `INTEGRATED` on the client dist, `DEDICATED` on the server dist, as NeoForge builds them.
- **D3. Native clients get Java roots as NeoForge sends them to a vanilla connection.** Per player, at vanilla's moments (placement,
  a permission change, Java's roots changing), Java runs NeoForge's own merge (`CommandHelper.mergeCommandNode` with the player
  view's source) over the Java and MIXED roots, then `CommandTreeCleaner.cleanArgumentTypes` keeping only argument types whose
  registry entry is not mod-owned (`BootFacts.owns`), and writes the result in the commands report's JSON (vanilla's
  `ArgumentUtils.serializeNodeToJson`) plus the inspector's `restricted` and `suggestions`. The hub replaces those roots in its
  tree with them. Reason: the native tree only speaks vanilla parsers (§1.1), and that is exactly what a vanilla client of a
  NeoForge server receives; the full tree reaches the window's Java client as today (SPEC-8 uses it).
- **D4. Server suggestions for Java nodes come from Java's real `getCompletionSuggestions`**, asked from the hub's
  `guest_suggestions` for a JAVA/MIXED root, as the guest. A future that isn't done when the call returns is answered at a later
  server tick (`ServerCommandSuggestionsProvider` answers asynchronously too).
- **D5. The window's chat input crosses into the Java client's real `ClientPacketListener.sendCommand` / `sendChat`** while
  `ClientBridge.send` captures instead of sending. A client command or a cancelled `ClientChatEvent` produces no packet:
  native sends nothing. Otherwise native signs and sends its own packet, with the text `ClientChatEvent` left. One owner per
  packet (native), one owner per decision (Java). The Java client's own chat additions (`ChatComponent.addMessage`,
  `Hud.setOverlayMessage`) are forwarded to native's chat and never kept in Java.
- **D6. `ClientChatReceivedEvent` runs only its hook in Java**, at the spot `ChatListener` calls it, inside native's receive path
  (`chat_received`); native keeps blocking, trust, delay, visibility and display. Reason: no duplicate chat state (rule 0); the event
  sees the same decorated component at the same moment.
- **D7. `ServerChatEvent` at vanilla's decoration step** (after unpack and before Horizon's chat filter and native mods). While a
  Java server runs, the hub defers each message to the gameplay layer as an event, which runs NeoForge's decorator as the
  guest and hands it back; a changed message becomes the unsigned content, a cancel drops it.
- **D8. Toasts: native owns the slots, the slide, the sounds and the timer; Java owns each mod toast's `update` and drawing.** A
  mod toast stays in Java's `queued` (so `getToast` and `SystemToast.addOrUpdate` work) until native finishes it, then Java removes it and
  calls `onFinishedRendering`. Its recording is taken per tick, and per frame only while it changes (the user's HUD rule). Its wanted
  visibility is read per tick (vanilla reads it per frame: up to 50 ms later; open question Q2).
- **D9. Synchronous crossings on the window thread** only for input-rate work: Enter in the chat box, a received chat message
  while a mod listens, the chat box's info for a Java-parsed root on each edit. As clicks and shell input are.
- **D10. The chat and command events are posted as often as NeoForge posts them, listened or not.** `CommandEvent` for every
  player and console command, `ServerChatEvent` for every chat message, `ClientChatEvent` for every chat line sent,
  `ClientChatReceivedEvent` for every message shown. NeoForge constructs them unconditionally and M1 counts constructions
  (COVERAGE-SPEC §2), so a listener gate would fail M1 on the play pack, which has no `CommandEvent` or `ServerChatEvent` listener.
  They are human-rate (one crossing per command or chat line), so the cost is negligible. What stays gated is work no event
  stands for: a command crosses for *running* only when its root is JAVA/MIXED, a typed command crosses to the Java client only
  when its root is a client command, a toast costs nothing until a mod adds one.

## 4. Specs

| Spec | Scope | Depends on | Size |
|---|---|---|---|
| [SPEC-1](SPEC-1.md) | `CommandSelection` by dist; Java classifies roots (D1); op `commands_roots`; Rust keeps the table | none | ~250 |
| [SPEC-2](SPEC-2.md) | Mod commands run in Java from `run_command` (players, console); `CommandEvent` for native commands (D10) | SPEC-1 | ~400 |
| [SPEC-3](SPEC-3.md) | The tree players get: per-player Java roots in the hub's tree (D3), re-sent at vanilla's moments, the Java client's permission packets | SPEC-1 | ~450 |
| [SPEC-4](SPEC-4.md) | `ask_server` suggestions for Java nodes from Java (D4) | SPEC-1, SPEC-3 | ~250 |
| [SPEC-5](SPEC-5.md) | Chat input through the Java client: client commands, `ClientChatEvent`, captured packets, the Java client's chat lines to native, client command names in the chat tree (D5) | none | ~400 |
| [SPEC-6](SPEC-6.md) | `ClientChatReceivedEvent` in native's receive path (D6) | none | ~250 |
| [SPEC-7](SPEC-7.md) | `ServerChatEvent` at decoration (D7) | none | ~250 |
| [SPEC-8](SPEC-8.md) | The chat box's parse, usage and completions from the Java client for Java-parsed roots | SPEC-5 (SPEC-3 for server roots to be typed) | ~450 |
| [SPEC-9](SPEC-9.md) | Mods' toasts drawn natively (D8) | none | ~450 |
| [SPEC-10](SPEC-10.md) | M1 scenario (chat and commands on Forge and Horizon), hz_scenario acceptance, map and docs | all | ~200 |

Order: SPEC-1 → SPEC-2 → SPEC-3 → SPEC-4 on the server; SPEC-5, SPEC-6, SPEC-7, SPEC-9 in parallel (independent files);
SPEC-8 after SPEC-5; SPEC-10 last. Each lands on its own and keeps the default build (`--features java-mods` off) compiling.

### Wire allocations

- Server ops (`Crossing.game` in `GameBridge.register`): `commands_roots`, `command`, `console_command`, `commands_for`,
  `command_suggest`, `command_suggestions_take`, `server_chat`. The player ops read `java_use_request`'s header with `GameBridge.fill(in, true)` first.
- Client ops (`ClientBridge.OPS`): `client_chat_send`, `client_chat_received`, `client_command_info`, `client_toast_frame`.
- Client tick `Input` kind 26 `Toasts` (W5 took 16–25). New status fields are appended after the last field present when the
  spec starts; update the status row of `java-mods-map.md` §5 in the same change.
- Root kinds: 0 native, 1 JAVA, 2 MIXED. `command` reply codes: 0 native runs it, 1 Java ran it, 2 stopped.

## 5. Out of scope (beyond the finish bar), with why

- `ToastAddEvent` for native's own vanilla toasts (advancement, recipe, system): only puzzleslib listens (its generic callback
  bridge), no finish-set mod does, and the Java client has no `AdvancementHolder` to build an `AdvancementToast` from. (Q1)
- Mod commands inside `execute ... run`, in functions (`.mcfunction`), in command blocks and command minecarts: they need native
  sources (entity, position, anchor) carried into Java and selectors over other players' views (W9/W14 have no other-player views).
  The finish mods' commands are typed by players and admins. (Q3)
- `/reload` re-running `RegisterCommandsEvent`: Java's data packs don't reload with native's (W13's data reload).
- Admin broadcast of a Java command's `sendSuccess(..., true)` to other operators: other players have no views (W9/W14).
- `ClientChatReceivedEvent.Player.getPlayerChatMessage()` carries an empty last-seen list (native doesn't keep the unpacked one at that point).
- Mixins into vanilla command execute methods: none in either pack (§1.4); W12's routing would add them.
- Clients without a JVM (LAN guests): they get D3's cleaned tree and can run mods' server commands, but no client commands, chat
  events or toasts, as a vanilla client on a NeoForge server.
- Narration of mod toasts (`Toast.getNarration` is not in 26.3's interface; vanilla narrates its own kinds natively).

## 6. Open questions for the user

- **Q1.** Fire `ToastAddEvent` (and honour its cancel) for native's vanilla toasts too? Needs a Java `Toast` per native toast; only puzzleslib listens.
- **Q2.** Toast visibility and drawing per tick plus per frame while changing (HUD rule), or ask Java each frame while a mod toast is on screen (vanilla's rate, ~0.1 ms a frame)?
- **Q3.** Mod commands under `execute`, in functions and in command blocks: leave for after the finish bar?
- **Q4.** Native mods' hooks and Java's on the same event (both may cancel a chat line or a command): native mods first, then Java, as written in the specs?
