# SPEC-20: server start and reload (AE2, Balm, Puzzles Lib, Lithostitched)

Design: `DESIGN.md` §2.6 row 20. Needs SPEC-3 (`Recv::Server`), SPEC-7 §2 (Java-bodied route stubs with a body class), and **W10** for one outcome (§1: a mod dimension needs a native level, W10's mod dimensions). Size: ~450 changed lines; if §4 alone passes 200, split it into `SPEC-20b` before starting.

Paths as SPEC-2. Read first: vanilla `MinecraftServer.java` (`createLevels`, `loadLevel`, `reloadResources`), `ReloadableServerResources.loadResources`, `Eula.java`, `DedicatedServerSettings`, `DedicatedServer.initServer`, `IntegratedServer.initServer` (client sources), `WorldLoader`; `B/GameBridge.java` (`prepare`, `lifecycle`, `level`), `B/ServerView.java` (`openWorld`), `G/java_mods.rs` (the `server_start` call ~1027), `G/levels.rs` (~143, native's level creation), `G/functions.rs` (~597, `/reload`), `G/dedicated.rs` (~52 start, ~159), `G/integrated.rs`, `crates/horizon-server-files/src/eula.rs` (~29), and W10 `DESIGN.md` (mod dimensions, SPEC-6's real `ChunkMap`).

Comments in code: one line at most.

## Keys
| Key | Kind | Mod | Plan |
|---|---|---|---|
| `MinecraftServer#createLevels()V` | edge TAIL | ae2 (spatial storage level) | §1 |
| `MinecraftServer#reloadResources(Ljava/util/Collection;)Ljava/util/concurrent/CompletableFuture;` | edge RETURN | balm | §2 |
| `ReloadableServerResources#loadResources(…)Ljava/util/concurrent/CompletableFuture;` | edge RETURN, static | balm | at boot it already runs in Java (`GameBridge.prepare` uses the real `WorldLoader`): verdict `java-boot` for boot; at `/reload` through §2 |
| `Eula#readFile()Z` | edge HEAD | puzzleslib | §3 |
| `DedicatedServerSettings#<init>(Ljava/nio/file/Path;)V` | inject | puzzleslib | §3 |
| `DedicatedServer#initServer()Z`, `IntegratedServer#initServer()Z` | body (inject before `loadLevel`) | lithostitched | §4 |

## 1. `createLevels`: the level loop of `server_start` is the real method
Today `GameBridge.lifecycle` (start) makes a level view per vanilla dimension (`level(id)` for the three) and opens the world. Vanilla's `loadLevel` → `createLevels` makes every level of the level stem registry, mods' included, and AE2's TAIL handler adds its spatial storage level to `levels`.
- In `lifecycle`, replace the `for (String id : List.of(…)) level(id)` loop with `server.createLevels()` (the merged method, through an accessor or `MethodHandle`) on the server view. Its route stub is Java-bodied (SPEC-7 §2's body class, here `ServerRoutes`): `ServerRoutes.body("create_levels", server, [])` makes `level(id)` for each dimension native has (`server_start`'s request lists native's dimension ids; add them to its input), puts them in `levels` and returns.
- The `server_start` request needs no Rust gate for this: Rust always calls `server_start`, and Java always runs the merged `createLevels`. With no route on, the method is vanilla's own body: make sure the stub decision (Java-bodied stub only when routed) leaves the unrouted case running `ServerRoutes.body` too, by calling it directly when `Routing.patched(key)` is false.
- After `createLevels` returns, any key in `levels` that isn't a view `GameBridge` made is a level a mod created (AE2: `ae2:spatial_storage`). For each: take its dimension key and type, drop the mod-made object from `levels` and put `level(id)` (a view) in its place (the views are the only levels Java's server side uses; a real `ServerLevel` would run its own chunk system). Reply with `int n`, n × (`text dimension id`, `text dimension type id`) after the effects.
- Rust (`G/java_mods.rs` at the `server_start` call): for each added dimension, ask W10's mod-dimension creation for a native level. Until W10 lands it, log once `Horizon: <mod> made the dimension <id>; it has no native level yet (W10)` at error level. That is the dependency, visible; nothing is faked.

## 2. `/reload` reaches Java
Native's `/reload` (`G/functions.rs`) reloads native data only; Java's data (mods' recipes, tags, loot, data maps) isn't reloaded, and `MinecraftServer#reloadResources`/`ReloadableServerResources#loadResources` never run at `/reload`.
- `G/functions.rs`: `/reload` calls `self.reload_resources(packs)`, a `Game` method with the gate first (`Recv::Server`, `TextListArg(packs)`: `int n`, n × `text`; add to SPEC-3), body = today's native reload, then the Java data reload (next point).
- Java data reload, op `data_reload` (new, also the body bridge's second half): run `ReloadableServerResources.loadResources(…)` the way `GameBridge.prepare` runs it at boot (same pack repository, the selected packs), wait for it on the server thread (vanilla's `reloadResources` future completes on the server's executor), swap `GameBridge.resources` and the server view's resources, and post NeoForge's events vanilla's path posts (`TagsUpdatedEvent` with the server cause, `OnDatapackSyncEvent`). Data that reached native at boot from Java (`BootFacts`, recipes, tags) is sent again through the same boot paths that sent it first.
- The route body returns `CompletableFuture.completedFuture(null)` to Java once both halves are done, so RETURN handlers that chain on it run at once, on the server thread, as vanilla's `thenAccept` runs when the future completes.

## 3. `Eula` and `DedicatedServerSettings`: constructed in Java at native's moment
Both are constructed by vanilla's dedicated server start; the patched methods run inside the constructors.
- `G/dedicated.rs`: where native reads `eula.txt` and `server.properties` (`horizon-server-files`), first call op `server_files` (new): `text eula_path`, `text properties_path`. Java (`ServerRoutes.files`): `new DedicatedServerSettings(propertiesPath)` (its injection runs; keep the object on the server view so mods reading the server's properties get it), then `new Eula(eulaPath)` (its constructor calls the patched `readFile`; the route stub's native body reads the file natively through a bridge to `eula.rs`'s reader), and reply `bool hasAgreedToEULA()`. Native uses that answer instead of its own read when java-mods is on, so a mod changing the EULA check is obeyed, as on NeoForge.
- Single player has no `eula.txt` and no settings file: not called.

## 4. `initServer` (dedicated and integrated)
Lithostitched injects just before `loadLevel()` in both `initServer`s. Vanilla's body around it is server infrastructure (network listener, player list, key pair, game rules) that native already does.
- The body runs in Java on the server view, called by native at the start of its own server start (`G/dedicated.rs` ~52, `G/integrated.rs` where the server thread starts its world), through a Rust gate (`route!`, `Recv::Server`) whose route is body: Java invokes the merged `initServer`.
- Every callee of the body that is infrastructure native already did, or does next, is a bridge: the closure in `check --analysis` (11-06: `initializeKeyPair`, `loadLevel`, `onGameRuleChanged`, `saveEverything`, the `DedicatedPlayerList` load and save calls…, plus the network listener start, `setPlayerList`, `setUsesAuthentication`, the properties getters). Each bridge does exactly native's step for it (most are already done by native before this call: the bridge then does nothing and returns what vanilla's would, e.g. `true` for the network start). `loadLevel` bridges to native's level load followed by `server_start` (§1), so the order is: lithostitched's handler, then levels.
- Native's own server start is split at the same points so each step happens once (native steps that vanilla's `initServer` performs move behind their bridge).
- `IntegratedServer#initServer` is a client class: the Java server of a single-player world runs on the window's process with the client classes present; route it the same way from `G/integrated.rs`.

## 5. Tests
- Test mod mixins (`J/testmod/…/mixin/ServerRouteMixin.java`):
  - `MinecraftServer#createLevels` TAIL: records the level keys present;
  - `ReloadableServerResources#loadResources` RETURN: counts;
  - `MinecraftServer#reloadResources` RETURN: counts and records the pack list;
  - `Eula#readFile` HEAD: records the path;
  - `DedicatedServer#initServer` and `IntegratedServer#initServer`: inject before `loadLevel`, records that no level exists yet.
- `ChecksRouting`: `create_levels_saw_three` (the three vanilla keys at TAIL); `reload_runs_load_resources` (after the session's `/reload`, `loadResources` count 2: boot and reload; `reloadResources` count 1); `init_before_levels` (integrated: no level at the inject). Dedicated: a `dedicated_server` harness run with the test mod (`parity.sh dedicated_server` with `--extra-jars`, if the harness takes them; else the lead's live step) checks `Eula#readFile` recorded `eula.txt`.
- A test-mod `createLevels` TAIL that adds a level `horizon_java_test:extra` (a copy of the overworld's stem) shows the reply lists it and the error line appears until W10 is in.
- `python3 tools/parity.py dedicated_server` and the `/reload` harnesses `--changed-files … --list` names; `cargo test` of the touched modules; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run.

## Done bar
Checks pass; five gates plus the `java-boot` verdict row for boot-time `loadResources`; lint passes; `/reload` in the play install reloads mods' recipes (a datapack overriding a Mekanism recipe takes effect after `/reload`).

## For the lead and the user
- Lead: dedicated server with the showcase install, two clients; `/reload`; both clients still see JEI recipes and mods' items work.
- User (AE2): build a spatial IO setup (Spatial IO Port, Spatial Pylons, a Spatial Storage Cell), capture a small area. Works only once W10 gives `ae2:spatial_storage` a native level; until then the boot log names it. With W10 in: the captured area disappears and reappears when restored, as on NeoForge.
