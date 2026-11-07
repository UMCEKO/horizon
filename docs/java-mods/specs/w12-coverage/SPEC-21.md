# SPEC-21: player join and travel (Creative Core, GlitchCore)

Design: `DESIGN.md` §2.6 row 21. Needs SPEC-2 (player fill), SPEC-3 (`TransitionArg`, `Recv::Level`), W9 SPEC-1d (player views join and leave levels with their events: `player_level`). Size: ~400 changed lines.

Paths as SPEC-2. Read first: vanilla `PlayerList.placeNewPlayer`, `ServerLevel.addPlayer` (and `addNewPlayer`, `addRespawnedPlayer`, `addDuringTeleport`), `ServerPlayer.teleport(TeleportTransition)`; `B/Menus.java` `login` (it sends `placeNewPlayer`'s packets in its order today), `B/GameBridge.java` `fill` (the `joined` branch), W9 `SPEC-1d.md` (the `player_level` op, its native sites: login, respawn, dimension change); `G/lan/hub.rs` (~851 the player's placement, ~3149 a player's teleport), `G/dedicated.rs` (~159), `G/server_travel.rs`, `G/levels.rs`.

Comments in code: one line at most.

## Keys
| Key | Kind | Mod | Seam |
|---|---|---|---|
| `PlayerList#placeNewPlayer(Lnet/minecraft/network/Connection;Lnet/minecraft/server/level/ServerPlayer;Lnet/minecraft/server/network/CommonListenerCookie;)V` | body (wrap `UnboundProtocol.bind`) | creativecore | §1 |
| `ServerLevel#addPlayer(Lnet/minecraft/server/level/ServerPlayer;)V` | edge HEAD | glitchcore | §2 |
| `ServerPlayer#teleport(Lnet/minecraft/world/level/portal/TeleportTransition;)Lnet/minecraft/server/level/ServerPlayer;` | edge TAIL | glitchcore | §3 |

## 1. `placeNewPlayer`: `Menus.login` runs the real method
`Menus.login` replays `placeNewPlayer`'s packets for the player's Java client. Creative Core wraps the play protocol's `bind` inside it (its packets' codecs on that connection), which the replay never reaches.
- Replace the replay with the real `PlayerList.placeNewPlayer(connection, player, cookie)` on the server view's player list, for every player native places (login, as SPEC-1d's join site): the connection is the player's Java connection (`Clients`/`Wire`) when it has a JVM client, else a `Connection` view whose `send` drops (today's outcome for a client without a JVM, `ConnectionMixin`); the cookie from the player's profile and the client information native has.
- The body's native-owned callees are bridges (closure in the 11-06 report, 22: `createCommonSpawnInfo`, `initInventoryMenu`, `sendServerStatus`, `broadcastAll`, `broadcastSystemMessage` (the join message: native sends it; the bridge does nothing more), `getMaxPlayers`, `sendActivePlayerEffects`, `sendLevelInfo`, `levelKeys`, `enforceSecureProfile`…). For each packet the body sends to the joining player's connection, the Java client gets it as it gets `Menus.login`'s today; the native client gets nothing from Java (native sent its own login sequence), so packet sends to a native-only connection stay dropped.
- `addNewPlayer` inside the body → `ServerLevel#addPlayer` (§2) through its route, as vanilla's body calls it.
- The Rust side: native's placement site calls op `player_placed` (the old `Menus.login` trigger, renamed if needed) once, in the same place; no gate (Java always runs the real method now; with no route it is vanilla's own body). The order vs `PlayerLoggedIn` stays `placeNewPlayer`'s (`firePlayerLoggedIn` is inside it in NeoForge's patched body: check with javap; if so, drop `GameBridge.fill`'s separate post of it).

## 2. `addPlayer` at native's level sites
SPEC-1d posts the player's level join from `player_level` at native's sites (login, respawn, dimension change). Vanilla's `addPlayer` (HEAD: GlitchCore) is the method those sites call.
- Each site calls `self.level_add_player(dim)`, a `Game` method with the gate first (`Recv::Level`, `PlayerArg`), body = native's add of the player to the level, then SPEC-1d's `player_level` join (nested crossing: it posts `EntityJoinLevelEvent` and `onAddedToLevel` in Java).
- With the route on, Java's merged `addPlayer` runs: GlitchCore's HEAD, then the stub's native body above.

## 3. `teleport`
- `G/lan/hub.rs` ~3149 and every native teleport of a player that vanilla does through `ServerPlayer.teleport(TeleportTransition)` (`/tp` across or within dimensions, portals, `/spawnpoint` respawns go elsewhere: check each in vanilla) call `self.player_teleport(transition)`, gate first (`Recv::Player`, `TransitionArg` from `Flat.raw(TeleportTransition)`'s layout), body = native's teleport.
- The return value is the player (vanilla returns `this` or null when it didn't happen): `RouteRet` for `bool` (Java writes `result != null`), the body returns whether native teleported.
- TAIL with several returns in vanilla's `teleport`: mark the body fn's early returns `route::early`; `java_routing.py lint` checks it.

## 4. Tests
- Test mod mixins (`J/testmod/…/mixin/JoinRouteMixin.java`):
  - `PlayerList#placeNewPlayer` `@WrapOperation` on `UnboundProtocol.bind`: counts and passes through;
  - `ServerLevel#addPlayer` HEAD: records the level and the player;
  - `ServerPlayer#teleport` TAIL: records the target level.
- `ChecksRouting`: `place_new_player_bound` (one bind per join of the session's player); `add_player_head_on_login` (overworld, the player); `teleport_tail_nether` (`/execute in minecraft:the_nether run tp @s 0 100 0`: one TAIL with `minecraft:the_nether`, and the player is there natively); the Java client still joins (the existing `bridges` client checks pass, since `Menus.login`'s replacement must keep them green).
- `lan.sh` login and dimension checks; `python3 tools/parity.py dedicated_server server_entities`.
- `cargo test` of the touched modules; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run.

## Done bar
Checks pass; three gates (placeNewPlayer is "always Java": record its row as `java-served` with evidence `Menus.login runs the real method` once the replay is gone); lint passes.

## For the lead and the user
- Lead: dedicated server, showcase install, two clients joining and leaving twice; each sees the other; JEI and mod screens work on both; no exception in either log.
- User: join a single-player world with the showcase pack, go through a nether portal and back, die and respawn: nothing differs from before.
