# W12: routing mod mixins into native-owned vanilla methods (design, revision 1)

Status: design only, nothing implemented. Specs, in order: `SPEC-1.md` (analysis), `SPEC-2.md` (boot fact and route stubs), `SPEC-3.md` (the gate), `SPEC-4.md` (lint and report), `SPEC-5.md` (first target: Carry On's inventory). Binding: COVERAGE-SPEC rule 0 and W12, `entities/DESIGN.md` §0, RULES.md.

## 0. The rule, restated
- A vanilla method M that native owns runs in Java **only** when a loaded mod's mixin changes M, and only for receivers on which vanilla would run the patched code. Everything else stays native.
- When M runs in Java, it runs **the real merged method** (Mixin already applied it), on views. Java never re-implements Mixin's semantics: ordering, cancellation, `CallbackInfoReturnable`, `@At("TAIL")`, `@WrapMethod`, MixinExtras locals all stay Mixin's own.
- One owner per effect: a routed call is the only execution of M for that call. Native doesn't also run its port's body, except as M's own body (§2, edge routes).

## 1. Evidence (showcase pack, `/home/umceko/hz-poc/showcase-pack/replaces/replaces.json`)
- 326 mixin changes; 172 vanilla methods routed `java`: 76 gameplay, 57 client, 39 worldgen (by package).
- Of the routed changes: `inject` 127 (HEAD 63, RETURN 24, TAIL 14, INVOKE 24, INVOKE_ASSIGN 2), `wrap_operation` 46, `modify_return_value` 8, `redirect` 7, `modify_expression_value` 7, `wrap_method` 5, `modify_arg` 2, `modify_variable` 2, `wrap_with_condition` 1.
- So about 55% can keep native's body (edge routes, §2); the rest need the vanilla body in Java (body routes).
- Gameplay routes by mod: lithostitched 48 (mostly worldgen), architectury 17, lootr 16, jade 14, balm 13, carryon 11, glitchcore 11, ae2 8, clumps 8, waystones 6, travelersbackpack 4, sophisticatedcore 3, refinedstorage 2. **Mekanism has no `java`-routed mixin in this pack**: its changes are overrides, listeners and data.
- **The `java-already` verdict is unsound for weak ledger statuses.** 85 mixin changes route `java-already` because the ledger says `mentioned`, `inferred`, `absent`, `none` or `unknown`, yet native clearly runs several of them: `Entity#tick` (waystones TAIL), `Entity#isInvulnerable`, `ItemEntity#setItem`, `ExperienceOrb#playerTouch`, `ItemStack#finishUsingItem`, `CropBlock#mayPlaceOn`, `ChestBlock#isBlockedChestByBlock`, `AnvilMenu#createResultInternal`, `Scoreboard#removePlayerFromTeam` (while `addPlayerToTeam` routes `java`). Lambdas (`StructureTemplate#lambda$placeEntities$0`, `ChunkMap#lambda$scheduleChunkLoad$3`) are reported as their own methods, not as their enclosing method. SPEC-1 fixes both.
- **The ledger's `fns` aren't gate sites.** Examples:
  - `Inventory#setSelectedSlot` maps to `horizon-world/src/player_inventory.rs:151`, which the game doesn't use; the server's real site is `game/lan_actions.rs:1333` (`ServerboundSetCarriedItem`).
  - `getFreeSlot` is inlined in at least five `Inventory.add` ports (`game.rs:2856`, `menu.rs:1189`, `menu.rs:1829`, `cauldron.rs:20`, `native_server.rs:442`).
  - `RandomizableContainer#unpackLootTable` (`commands.rs:2301` `unpack_opened_loot`) restores a roll made at generation, where vanilla rolls at open with the opening player in the loot context.

  Routing a method therefore starts with a **seam**: one Rust fn whose boundary is the vanilla method's (arguments in, return out, same return points), which every native caller goes through.
- **Existing hand routes:** W0's `animateTick` (`Animate.java`, `java_animate_route`), item clicks (`ItemCode`, `Host::item_code_runs`), `PlaySoundEvent` (`NativeSounds`). The entities design's §3 routing hook (phase 3) isn't implemented: no Rust caller of `EntityFacts::route` exists. W12 replaces it (decision U4).

## 2. The mechanism
### 2.1 Two route kinds, decided statically per method (SPEC-1)
- **edge**: every change to M is one of these:
  - `inject` at HEAD, RETURN or TAIL without captured locals;
  - `modify_return_value`;
  - `wrap_method`;
  - `modify_variable` at HEAD with `argsOnly` (it changes M's own parameters). `modify_arg` always targets a call inside the body, so it is never edge.

  The mods' code surrounds M's body but never reaches inside it, so **M's original body stays native**.
- **body**: anything that targets a point inside the body (INVOKE, FIELD, NEW, expressions, `redirect`, `wrap_operation`, `modify_expression_value`, captured locals, `overwrite`). The vanilla body runs in Java.
- If a pack has both kinds on M, M is `body`. Edge changes still run, because they are inside the Java method.

### 2.2 The call
```text
native caller → port fn of M (Rust)
   route!(…)  ── not routed, or receiver not admitted ──→ native body (fast path: one atomic load, one bit test)
      │ routed and admitted
      ▼
   one crossing: op `route` (route id, receiver ref, arguments)
      Java: Routing.call → refresh the receiver's and arguments' views → invoke the merged Java M on them
         edge:  HEAD handlers (Mixin) → route stub → bridge to native body (nested crossing back to Rust; the gate is bypassed)
                → RETURN/TAIL handlers (Mixin) → return
         body:  the vanilla body runs in Java; its callees that touch native state are bridged natives;
                callees that are routed are just Java calls (their own stubs bridge their bodies)
      reply: status, return value (Flat), effects (as every reply)
   route! returns Java's value from the port fn
```
- **The route stub (SPEC-2)** is how "only the handlers run in Java" stays exact.
  - For each edge-routed M, `BridgeProcessor` (before Mixin) inserts at M's head: `if (receiver is backed) { r = native(args); if (Routing.early()) return r; goto tail; }`. `Routing.early()` is Java: a flag a Java body sets, or the native body's mark.
  - It rewrites the original body's **last** return into `goto tail`, and appends `tail: return r` as the method's last instruction.
  - Mixin then puts HEAD handlers before the stub, RETURN handlers before every return, and TAIL handlers only before `tail`. So a native early return skips TAIL handlers exactly as vanilla's would.
  - Non-backed receivers (Java-only objects, mod types) take the original body as before.
- **Body-routed methods lose their bridge in that install.** Today a bridged method's Java body never runs, so a mod's redirect inside it is silently lost (map, Gotchas). The class processor skips every bridge whose method is body-routed by `routing.tsv`, and logs it once.
- **Bypass:** the port fn is split into the gate fn (`route!` first) and its body fn. The route's body bridge calls the body fn, so the nested call never re-enters the gate.
- **Exceptions (U6, user): they propagate as in vanilla.** Java doesn't catch them in a route. The reply says "threw", with the message and the mod (the handler's mixin owner). Rust panics on the server thread with `Horizon: a Java mod threw in <Class.method> (<mod>): <message>`, which is vanilla's "Exception ticking world" moment. A throw in a nested native body crosses back as a Java exception (`bridge::enter` already turns panics into Java exceptions), so it propagates up through every frame.
- **GUI bridges (W5, `w5/SPEC-7.md` §1): Java-bodied stubs, accepted.**
  - Some client methods have a native counterpart that isn't a Rust fn: a shell's draw steps, the hovered-slot check, HUD layers, item drawing. W5 lists them in `gui_bridges.tsv` (`key \t seam`).
  - The class processor gives each row the same stub as §2.2, with two substitutions:
    - the backed test is `GuiRoutes.backed(this)`;
    - the body call is the Java `GuiRoutes.body(seam, this, Object[] args)`, not a JNI native.
  - Rows are handled like any route:
    - a row not routed gets the plain bridge head;
    - an edge-routed row gets the exact-TAIL stub;
    - a body-routed row gets no stub.
  - The native site that serves the key carries `gui_route!("key")`, a marker the gate list counts.
  - Window context only, with no nested call into native (§2.5).
  - If the native body hasn't run yet, native runs it.
  - If it has, its recorded return stands.


### 2.3 Receivers: who is admitted (boot, SPEC-2)
- A mixin into `D#M` changes M only for receivers whose virtual call reaches D's body:
  - **resolve(T, M)**: the first class in T's superclass chain that declares M.
  - T is admitted if resolve is D. It is also admitted if resolve is a subclass S whose M calls `super.M` down to D; that super-call table is read from the vanilla jar (SPEC-1 `overriders`).
- At boot, Java computes **one bitset per route over each receiver family's registry**:
  - entity types: the class chain from `entity-types.bin`;
  - blocks and block states: the block's class;
  - items: the item's class;
  - block entity types: the class of one instance.

  Static methods and singleton receivers (level, scoreboard, server) have no bitset.
- **Overriders in native:** where vanilla subclass S overrides M and calls `super.M`, S's port must call the gated D port at the super-call point. SPEC-4's lint checks it from `overriders`.

### 2.4 Views (state access)
- The receiver and every argument cross as **references or values**, never as copies to write back. Values are primitives, `BlockPos`, `BlockState`, `ItemStack` as a tag, `Vec3`, resource keys and enums, all through `Flat`.
- References are refreshed views, one per family:

| Family | Exists | Receiver ref on the wire | Notes |
|---|---|---|---|
| Acting player (`ServerPlayer`, its `Inventory`, its menu) | yes: `GameBridge.fill` through `java_use_request` | the use header | `Inventory` methods use the acting player's view; guests through `as_guest` |
| Level (`ServerLevel`) | yes | dimension id + game time (`java_level_call`) | |
| Mod block entities | yes (`BlockEntities`) | pos | |
| Native entities | phase 1c of the entities work (in progress) | entity id | routes on entities wait for 1c |
| Vanilla block entities (chest, barrel, hopper, brushable) | **new**, Lootr target | pos | write-through bridges for container slots and loot-table fields |
| Scoreboard and teams | **new**, Waystones target | none (singleton) | `PlayerTeam` args by name |
| Save streams (`ValueInput`/`ValueOutput`) | **new**, Waystones target | a tag in; written keys out | `ValueOutput` writes become keys merged into the native save at that moment; `ValueInput` reads the native compound |

- Writes made by handler or body code through a view go through bridged natives (write-through). `java_mods.py check` already requires a bridge for every entity writer mod code calls. SPEC-1 extends the same closure check to every view family a route touches.

### 2.5 Contexts and reentrancy
A crossing needs `&mut Game` with no entity vector taken and no borrow alive. Gate contexts:

| Context | Where | Rule |
|---|---|---|
| **Game** | a `Game` method, `route!` as its first statement | allowed now. The macro takes `self: &mut Game`, so a gate in a lib fn doesn't compile |
| **Lent loop** | a port run per entity inside one of SPEC-1a's 14 loops (the vector is taken) | the port is called at the loop's crossing point through `lend` (`route_lent!`). The receiver is looked up in `Game`, so a gate crossing with the vector taken fails a debug assertion naming the loop |
| **Level cascade** | inside `with_level` / `GameLevel` (horizon-world level code) | no gate. The seam lifts the gate to the `Game` method that starts the cascade, or the route is deferred with reason `seam: lift` |
| **Worldgen** | streaming workers | W10 (its views are chunk regions). The 39 worldgen routes are deferred `worldgen: W10` and use the same gate kind once W10 defines its context |
| **Window** | client, window thread | phase C (§5). Only on synchronous input paths, as `java_click_canceled`. Per-frame client routes become recordings, as the HUD |

- **Nesting:** the native body called back from a route stub is itself a `Game` method. Gates inside it cross again, with Host nesting (already supported: `bench_nested`).
  - Effects are written at depth 1 only (existing rule).
  - `GameBridge.LOCK` is already held on the server thread.
- **SPEC-1a crossing points are reused, not duplicated.** For entity tick methods (`ItemEntity#tick` RETURN, ae2), the port of one entity's tick becomes a `Game` fn by id that the loop calls through `lend` at its crossing point. That is how the entities phase 3 rows land.

### 2.6 Cost model (per call)

| Case | Cost |
|---|---|
| Built without `java-mods`, or no pack routes M | `route!` expands to a `static` and one relaxed `AtomicU32` load: ~1 ns, no allocation |
| Routed, receiver not admitted | + one bitset test: ~1–2 ns |
| Routed edge, admitted | one crossing (~0.1 µs empty, measured by `java_mods.py crossing`) + argument marshal + view refresh (player fill is the heavy one; measured in SPEC-3) + handlers + one nested body bridge (~0.1–0.3 µs JNI + `Flat`) |
| Routed edge, HEAD cancels | no body bridge |
| Routed body | one crossing + view refresh + the Java body + one bridge call per native callee |

- `HORIZON_PROFILE` gets one op per route (`route:<Class#method>`), with count and mean, in `java-ticks.log` (SPEC-3).
- **Hot routes** are methods native calls more than once per entity per tick, like `Entity#isInWater` (travelersbackpack HEAD). SPEC-1 flags them from the vanilla call graph (callers in tick paths), and the boot report lists them.
- Their per-type bitset is the first filter. A provable handler guard (`if (!(this instanceof X)) return;`, the same guard analysis as entity event listeners) narrows it further, in a later spec. Never by skipping calls.

### 2.7 The boot fact
- **Install time:** `tools/mod_replaces.py` → `INSTALL/analysis/routing.json` (format 1) and `routing.tsv`, the flat list the class processor reads. Per routed key:
  - kind;
  - the descriptor;
  - the changes, with conditional flags;
  - receiver family;
  - argument plan;
  - overriders;
  - closure verdicts (`needs-bridge`);
  - hot flag;
  - status (`gated` / `deferred <reason>` / `ungated`).
- **Boot confirmation (`Routing.confirm`, after `BootFacts.seal`):**
  - Mixin's applied list (`ClassInfo.forName(target).getAppliedMixins()`) and the merged handlers (`@MixinMerged`, as `EntityFacts.mergedName`) decide each route, conditional mixins included. A route whose mixin didn't apply is off. A non-conditional route that didn't apply fails the boot (Mixin's `defaultRequire` would have too).
  - One `MethodHandle` per route.
  - Per-family receiver bitsets.
- **`routes.bin`** goes in the content cache folder beside `entities.bin`. `fingerprint_of` hashes `routing.json`. Rust reads it once (`horizon_java_host::routes()`, a `OnceLock`).
- **Gate resolution:** each `route!` site's `static Gate` resolves its key against `routes()` on first call and caches the route id (or "off") in its atomic.

### 2.8 Enforcement: the lint and the report (SPEC-4)
- **`route!` keys and W5's `gui_route!` markers are the registry.** `tools/java_routing.py gates` scans the source and writes `J/boot/res/route_gates.tsv` (key, file:line, body fn), committed, with a CI freshness check like `animate_closure.tsv`. Java reads it at boot, so the boot report knows which routes this build gates.
- **CI lint (`tools/java_routing.py lint`, no jars needed) fails when:**
  - a routed key of a reference pack (`tools/java_routing/packs/*.tsv`: vanilla keys, kinds, mod ids; no mod code; decision U3) has no gate and no row in `tools/java_routing/deferred.tsv` (key, reason: `worldgen: W10`, `client: phase C`, `view: entities 1c`, `seam: lift`, `special: <existing mechanism>`);
  - a gate's key has no body bridge entry (edge) or isn't routed by any reference pack (stale);
  - a gate isn't the first statement of a `Game` method;
  - a gated method with TAIL changes and more than one vanilla return has a body fn without `route::early` marks.
- **Ledger lint (`--ledger`, in `pr_ready.py` and the parity job, which have `.local/`) fails when:**
  - a ledger port fn of a routed key is neither gated nor called only from gated fns;
  - an `overriders` super-call site has no gate;
  - a routed key's weak-status verdict (§1) isn't recorded in `tools/port_map/`.
- **Boot report (M2):** `Horizon: route <key> (<mods>): <gated|deferred: reason|UNGATED>` once per route. `java_mods.py replaces` and `mixins` print the same table. An ungated route in a player's pack is a loud boot warning naming the mod and the behaviour (decision U2).

## 3. Contract for a gated port (what juniors write)
```rust
/// `Inventory.setSelectedSlot`: the acting player's hotbar selection.
pub(in crate::game) fn set_selected_slot(&mut self, slot: i32) {
    route!(self, "net/minecraft/world/entity/player/Inventory#setSelectedSlot(I)V", Recv::Player, (slot));
    self.set_selected_slot_body(slot)
}
/// `Inventory.setSelectedSlot`'s body, native.
pub(in crate::game) fn set_selected_slot_body(&mut self, slot: i32) { … }
```
And the body bridge, in the family's `java_bridges!` table:
```rust
route "net/minecraft/world/entity/player/Inventory".setSelectedSlot "(I)V" if backed =>
fn route_inventory_set_selected_slot(game, slot: i32) -> () { game.set_selected_slot_body(slot) }
```
- `route!` expands to nothing but the `static Gate` (for the lint) when `java-mods` is off.
- With `java-mods`, it is `if let Some(r) = GATE.admitted(recv) { return self.route_call(r, recv, args) }`. `route_call` is generic over the return type through `Ret`.
- Early returns of a body fn whose vanilla method has several returns and a TAIL change: `return route::early(v)`.

## 4. First targets, ranked
**By what the showcase pack's players lose today:**
1. **Lootr's loot generation:** per-player loot is Lootr's whole point.
2. **Carry On:**
   - picking up works through events;
   - the carried entity's placement, dismount and the slot lock while carrying are the mixins.
3. **Waystones:**
   - teleporting works: its blocks are mod block entities;
   - lost today: warp-plate state on entities, team sharing, and waystone villages (worldgen).

**By build order (what each needs first):**

| # | Target | Routes | Kind | Needs | Spec |
|---|---|---|---|---|---|
| T1 | Carry On, the player's inventory | `Inventory#setSelectedSlot`, `pickSlot`, `addAndPickItem` (HEAD), `getFreeSlot` (wrap_operation → body); all conditional on Carry On's config plugin | edge ×3, body ×1 | player views (exist); seams: the selection, vanilla's `tryPickItem` pieces, one `getFreeSlot` port for the `Game` paths. The lib `Inventory.add` ports (`menu.rs`, `cauldron.rs`) stay partial seams until one `Inventory.add` port exists (next seam spec) | `SPEC-5.md` |
| T1b | Carry On, the client's slot lock | `Minecraft#handleKeybinds` (wrap_with_condition) | window gate | phase C | later |
| T2 | Waystones, gameplay; Carry On's player load | `ServerScoreboard#addPlayerToTeam`/`removePlayerFromTeam` (RETURN), `Entity#load`/`saveWithoutId` (HEAD) on players, `Entity#tick` (TAIL, after its verdict is fixed), `Player#readAdditionalSaveData` (RETURN, carryon) | edge | scoreboard view, save-stream args; entity receivers other than players after entities 1c | after T1 |
| T3 | Lootr, loot generation | `RandomizableContainer#unpackLootTable`, `tryLoadLootTable`, `setLootTable`, `LootTable#fill`, `BrushableBlockEntity#tryLoadLootTable`, `HopperBlockEntity#getBlockContainer`/`getEntityContainer` (wrap_method), `AbstractContainerMenu#getRedstoneSignalFromContainer` | body ×4, edge ×4 | **first a senior JFR trace on real NeoForge** (which merged Lootr handlers fire when a structure chest generates, is opened by player A, then by B; clean room: traces only); vanilla container BE views; native's lazy loot table (keep table and seed until open, roll in `unpackLootTable` with the player; a P1 parity change, decision U5); `setLootTable` from structure pieces is worldgen (with W10) | after T2 |
| T4 | Carry On, riding | `Entity#positionRider`, `getDismountLocationForPassenger` (HEAD), `startRiding` (body) | edge, body | entities 1c views, riding bridges | after entities 1c |

**Then, by count:** clumps (8, `ExperienceOrb`, entities), ae2 and jade (entity routes from entities §3), balm, architectury, glitchcore. Worldgen with W10, client in phase C.

## 5. Phases
| # | Phase | Delivers | Checks |
|---|---|---|---|
| 1 | Analysis (SPEC-1) | `routing.json`/`routing.tsv` at install: kinds, lambdas folded, weak-status verdict flags, overriders, closure, hot flags; reference pack TSVs | Python unit tests on javac-built mixins; the showcase numbers in §1 |
| 2 | Boot fact and stubs (SPEC-2) | `Routing.confirm`, `routes.bin`, Rust reader, `BridgeProcessor` route stubs with exact TAIL | Java unit check on the stub (TAIL only at the shared tail); `cargo test -p horizon-java-host routes` |
| 3 | The gate (SPEC-3) | `route!`, `route_call`, op `route`, `Recv::{Player, Level, Static}`, early marks, exceptions, profile ops; the test mod's mixins on `Inventory#setSelectedSlot` | test-mod checks: HEAD cancel, RETURN after body, TAIL; Rust test: a throw panics naming the mod; no route without the mod |
| 4 | Lint and report (SPEC-4) | `gates`, `lint`, `--ledger`, CI job, boot report, `deferred.tsv` seeded with every current route and its reason | Python unit tests; the CI job red on a removed gate |
| 5 | T1 (SPEC-5) | Carry On's server inventory routes; one `getFreeSlot` port | test-mod body route on `getFreeSlot`; user test with Carry On |
| 6+ | T2, T3, T4; new receiver families (scoreboard, save streams, vanilla BEs, entities); phase C (window gates); W10 uses the worldgen context | one spec each, under 500 lines | per spec |

## 6. Decisions for the user
- **U1. Scope boundary in CI.** CI fails on any reference-pack route that is neither gated nor listed in `deferred.tsv` with a reason. The alternative, failing on every ungated route, keeps CI red until all 172 are done. Recommended: the deferral list, which shrinks with each target.
- **U2. A player's pack with ungated routes.** Recommended: boot with a loud warning naming the mod and the behaviour, plus the M2 report. Refusing would stop nearly every pack today.
- **U3. Commit reference-pack route lists** (`tools/java_routing/packs/showcase.tsv`). They hold vanilla method keys, mixin kinds and mod ids, and no mod code or assets. Without them, CI can't check routes without the jars.
- **U4. W12 replaces the entities design's phase 3 routing hook.** Entity routes become W12 routes with `Recv::Entity` and per-type bitsets. The entities workstream keeps views, events and loops. Coordinator to confirm.
- **U5. Lootr target needs native loot to follow vanilla's lazy loot table.** Containers keep table and seed until opened, and roll in `unpackLootTable` with the opening player. That's a P1 change to saves and harness outputs.
- **U6 (decided): a throwing handler propagates as in vanilla**, with the mod named (§2.2).
- **U7. Weak ledger verdicts.** About 85 mixin changes in the showcase pack sit on methods the ledger marks `mentioned`/`inferred`/`absent`/`none`/`unknown`. The ledger lint makes each one a recorded verdict before it can route `java-already`; you decide the borderline ones.

## Decisions (2026-11-05)
- **U1 (user):** CI allows a deferral list. Each unrouted method has a fixed reason; the list must shrink to zero.
- **U2 (user):** a pack with ungated routes boots with a loud warning naming each mod and behaviour.
- **U3:** reference-pack route lists may be committed (method keys, patch kinds and mod ids only).
- **U4:** W12 replaces the entities phase-3 routing hook.
- **U5:** native loot rolls at open, as vanilla (a vanilla fix; Lootr needs it).
- **U6 (user):** a routed handler's exception propagates as in vanilla, with the mod named.
- **U7:** the 85 weak ledger matches get verdicts; the borderline ones go to the user.
