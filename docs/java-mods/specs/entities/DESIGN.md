# Entities for Java mods: design (revision 3, no debt)

Status: design only, nothing implemented. Specs to implement first, in order: `SPEC-0-analysis.md`, `SPEC-1a.md`, `SPEC-1b.md`, `SPEC-1c.md`, `SPEC-1d.md` (beside this file).

## 0. Decisions (binding, user, 2026-11-04)
1. **Java is only a modloader.**
   - Vanilla behaviour is native, on the server and the client.
   - Vanilla Java objects exist only as **views** that mod code reads, and writes through.
   - Vanilla logic never runs in Java in place of native code.
2. **Detected behaviours only.**
   - A vanilla behaviour runs in Java only when a loaded mod is detected to change *that* behaviour: its mixins, overrides, event listeners or data.
   - Detection is static, from the jars at install, and confirmed once at boot.
   - Everything else stays native.
3. **The Java client stays only while it duplicates nothing native does.** Its `ClientLevel` holds packet-fed entity views that are never ticked. Native fires the client events (variant (a), §6).
4. **No debt.**
   - Every phase is final in what it delivers. A phase may cover less, but what it covers is exact and stays as it is.
   - Join and leave fire at the real add and remove sites, in vanilla's order.
   - Setters write through. Nothing is ever written back from a diff.
   - Per-entity tick events fire in exact vanilla order: never batched, never reordered.
   - The Rust→Java crossing cost is fixed separately by the host-crossing work. §1.3 gives it a target.
5. **Pre-analyse everything possible from the jars.**
   - Runtime does only what a loaded mod can observe: listeners filtered by their provable guards, views carrying only the state mod code touches, one route flag per entity type and method.
   - The analysis is one versioned boot fact (§1.4, `SPEC-0-analysis.md`).
   - What can't be decided statically is confirmed at boot, never guessed.
6. **Standing rules:** one owner per effect; no polling or mirroring; no per-mod code (decisions come from registries, ownership, analysis tables); all rendering native; boot data lands in native at boot.

## 1. Evidence
### 1.1 Real NeoForge (`parity-tools/run2`, Mekanism + AE2 + Jade + BG2, one player, fresh world; counts halved, the tool doubles them)
- Per server tick:
  - `MobDespawnEvent` ≈ 220–250;
  - `EntityTickEvent.Pre/Post` ≈ 210–236 each;
  - `LivingBreatheEvent` ≈ 190–217;
  - `EntityMobGriefingEvent` ≈ 13;
  - `LivingVisibilityEvent` ≈ 6.6.
- `EntityJoinLevelEvent`: 291 server and 202 client in the run. `StartTracking` 198, `StopTracking` 114.
- Per frame: `RenderLivingEvent` ≈ 8, `RenderNameTagEvent.CanRender` ≈ 9.
- Horizon fires none of these.

### 1.2 Horizon, live run 1 (`entities/run1-profile`)
- 190 natural mobs near one player (seed 1).
- Native `tick_mobs`: 1.35 ms a tick at 190 mobs, 2.5 ms at 364, so **≈ 7 µs per mob per tick**.
- Smallest Rust→Java request (`block_ticks`, 20,590 calls): **mean 8 µs**. `level_tick` 177 µs (151 µs of it lock wait), `tracking` 170 µs.

### 1.3 Target for the host-crossing work
With exact order, this pack makes one crossing per **living non-player server entity per tick**: Mekanism's `PlayerExposure.onLivingTick` (`EntityTickEvent.Post`), whose guard is proven in §1.4.
- One player: ≈ 190 crossings a tick, **1.5 ms** today.
- 15 players spread out: ≈ 2,900 crossings a tick, **23 ms** today.
- **Target: ≤ 1 µs per crossing**, including refreshing the subject's view (§2). That is ≤ 3 ms a tick at 15 players, ≤ 6% of the 50 ms tick, against native's ≈ 20 ms for the same mobs.
- Join, leave and the damage, death and drop events are rare per tick; the same per-crossing cost applies to them.

### 1.4 Static analysis of the pack (`replaces/replaces.json` from `tools/mod_replaces.py`, plus `entity_calls.py` and `listeners_where.py` here)
**Entity listeners on the game bus:**
- `EntityTickEvent.Post`: Mekanism `PlayerExposure.onLivingTick`. Its bytecode starts with `level.isClientSide() → return`, `!(entity instanceof LivingEntity) → return`, `entity instanceof Player → return`, then `tickRateManager().isEntityFrozen(e) → return`. The first three are provable static guards.
- `EntityJoinLevelEvent`: Jade, Mekanism ×2, NeoForge itself. `EntityLeaveLevelEvent`: Jade.
- `LivingDeathEvent`, `LivingIncomingDamageEvent`, `LivingFallEvent`, `LivingJumpEvent`, `EntityInvulnerabilityCheckEvent`, `StartTracking`: Mekanism. `FinalizeSpawnEvent`: MekanismTools.
- Not listened: `MobDespawnEvent`, `LivingBreatheEvent`, `LivingVisibilityEvent`, `EntityMobGriefingEvent`, `RenderLivingEvent`, `RenderNameTagEvent`.

**Routes:**
- Mixins: `ItemEntity.tick` @RETURN (ae2), `ItemEntity.hurtServer` @HEAD (ae2), `LivingEntity.addEffect` @HEAD (Jade).
- Own-type overrides, 168 keys in all. Entity ones:
  - `PrimedTnt.tick`/`explode` (AE2 tiny TNT, Mekanism obsidian TNT);
  - `LivingEntity.isBaby` (Mekanism's baby mobs);
  - `Mob.tick`/`baseTick`/`removeWhenFarAway`/`addAdditionalSaveData`, `LivingEntity.actuallyHurt`/`isPushable` (robit);
  - `Goal.start/stop/tick` (robit goals);
  - `Projectile.tick` (flame);
  - `Entity.*` (balloon).
- Renderer and model overrides: `EntityRenderer.extractRenderState`/`submit`, `LivingEntityRenderer.submit`, `HumanoidArmorLayer.submit` (`MekanismArmorLayer`), `Model.setupAnim`.

**State mod code touches on entities:** 480 distinct vanilla entity methods called, 68 of them writers.
- Writers on entities native backs:
  - `discard`, `remove`, `hurtServer`, `hurt`;
  - `setDeltaMovement`, `addDeltaMovement`, `setPos`, `teleport`, `teleportTo`, `knockback`;
  - `igniteForSeconds`, `setRemainingFireTicks`, `setItemSlot`;
  - `ItemEntity.setItem`, `setPickUpDelay`, `setNoPickUpDelay`, `setTarget`;
  - `startRiding`, `stopRiding`, `setSilent`, `Sheep.setColor`;
  - `setData` (Java-owned).
- guideme's `setId`, `setRemoved`, `setYRot`… act on its own scene entities, which native doesn't back.

**Native lifecycle sites** (`lifecycle-sites.txt`): 60 add and 113 remove sites over 12 entity vectors, plus the dragon. Join and leave can only be exact once they go through one choke point (phase 1b), and the 14 loops that take an entity vector out of `Game` must be re-entrant first (phase 1a).

**Vanilla order** (javap of the patched jar):
- `EntityJoinLevelEvent` is posted first in `PersistentEntitySectionManager.addEntity(entity, loaded)`, before the entity is in the level; a cancel returns false.
- `EntityLeaveLevelEvent` is posted last in `ServerLevel.EntityCallbacks.onTrackingEnd`, after `entity.onRemovedFromLevel()`. It runs inside `setRemoved`, after the removal reason is set.

## 2. Ownership and views

| What | Owner | Java holds |
|---|---|---|
| Vanilla entity state, behaviour and saving of vanilla keys | Native | A view |
| Java-only data: attachments, persistent data (`NeoForgeData`), capability backing, mods' own synched-data items, mod fields of mod types | Java | The live object; saved by asking Java at native's save (phase 1c) |
| A vanilla method a loaded mod patches or overrides | Native at the site; Java for the patched part only (§3) | The view the call runs on |
| Event listeners | Native fires at the site matching vanilla's caller, only for subjects a listener's guard admits (§4) | Subject views |
| Mod entity types | Native, as the nearest vanilla kind they subclass; overrides are routes (§8) | View plus Java-owned fields |
| Tracking | Native tracker; Java adds only the bytes of Java-owned data items and complex spawn data | None |
| Client entities | Native | Packet-fed views, never ticked (§6) |
| Drawing | Native | Recordings of mod renderer, model and layer code (§7) |

### Views
- **One live object per native entity**, from its join (the real add site) to its leave (the real remove site).
- **A view carries only the state groups the analysis says mod code reads**, per entity type where provable: position, rotation, motion, health, effects, equipment, fire, air, flags, name and tags, item, riding.
- **Groups are refreshed** from native, raw (accessor writes, no setter, no vanilla logic), at each crossing that hands the view to mod code: the event's subject, a route's receiver, or an entity a query returns.
- **No tick-wide sync.** Freshness is per crossing, which makes it exact. A query (`getEntities(box)`) is answered by native with ids; each returned view is refreshed for its groups.
- **Writes:**
  - A setter of native-owned state on a native-backed view is a bridged native that writes through. Every writer the analysis finds has a bridge, or `java_mods.py check` fails. Nothing is diffed or written back.
  - Java-owned state stays in Java. Native keeps the saved compound's Java-owned keys opaque in `persist.raw`. At each native entity save, Java answers with the live Java-owned keys (`entities_save`, phase 1c), which is vanilla's `saveWithoutId` moment for attachments.
- **The "native-backed" guard:** a boolean field a mixin adds to `Entity` (`horizon$backed`), set when native makes the view. `BridgeProcessor` gets a predicate guard form, so entity bridges apply to backed views only, whatever their class (a mod subclass of `Zombie` included).

### Re-entrancy for exact order
- Exact per-entity order means synchronous crossings from inside native loops: the mob loop, item ticks, projectiles, vehicles.
- Today the mob loop takes `self.mobs` and holds `MobWorld` borrows (`creatures.rs` 672–860), so `&mut Game` isn't free between mobs.
- Phase 1a makes the entity loops re-entrant:
  - `MobWorld`'s shared parts become owned or `Arc` snapshots that don't borrow `Game`;
  - the taken vector goes back into `Game` around each crossing (two `mem::swap`s);
  - an entity's own tick effects keep going through the loop's `events` vector, which is also where Java's writes during that entity's turn land, in order.
- Box queries from Java during a turn are answered by native from that same state.

## 3. The per-behaviour routing hook
**The table (boot fact, §5).** `methods`: vanilla method keys native owns that some loaded mod changes. Per entity type, one bitset over those methods. Each `(type, method)` route is one of:
- **edge:** HEAD, RETURN or TAIL injectors without local capture. A list of merged handler names, a cancellable flag, and the position.
- **body:** any other injector, or an overwrite.
- **override:** the type's class redeclares the method.

**At the native site:**
```text
if facts.route(type, M):     one bit test
    edge:   HEAD handlers (Java, on the refreshed view) → canceled? use the handler's return, skip the body
            native body
            exit handlers (Java) → may replace the return value
    body or override: the Java method runs on the view; the vanilla callees it reaches come back
            to native through bridged natives (receiver = the view's entity, predicate guard)
else: native body
```
- Handler-only calls use `MethodHandle`s resolved at boot from the merged handler names, with a fresh `CallbackInfo` or `CallbackInfoReturnable`.
- A routed body's callee with no bridge is an analysis failure (`needs-bridge`), reported at install and boot. A pack needing it isn't supported until the bridge exists; it never silently runs vanilla Java.

**The pack's routes:**

| Method | Detection | Route | Native site |
|---|---|---|---|
| `ItemEntity.tick` | ae2 mixin @RETURN | edge, exit | item tick (`game.rs` ~3188–3212, `entities.rs:161`) |
| `ItemEntity.hurtServer` | ae2 mixin @HEAD, cancellable | edge, HEAD | item damage (explosions, fire, lava) |
| `LivingEntity.addEffect` | Jade mixin @HEAD | edge, HEAD | the bin's effect adds (`effects.rs:186` callers) |
| `PrimedTnt.tick`, `explode` | own-type override (tiny TNT, obsidian TNT) | override; `super.tick()` → native | `game/tnt.rs` |
| `LivingEntity.isBaby`, `Creeper.explodeCreeper`, `getDefaultDimensions` | own-type override (baby mobs) | override; dimensions are a boot fact when pure (checked twice at boot) | species code that asks |
| `Mob.tick`, `baseTick`, `actuallyHurt`, `mobInteract`, save data; `Goal.*` | own-type override (robit) | override; goals in §8 | phase 7 |

## 4. Event firing (server)
- Each event fires synchronously at the native site matching its vanilla caller, in vanilla's order.
- It fires only when listened, and only for subjects the listeners' guards admit (§5).
- A subject is refreshed (§2), the event is posted, and the result returns in the reply.

| Event | Vanilla caller | Horizon site | Result back |
|---|---|---|---|
| `EntityJoinLevelEvent` | top of `PersistentEntitySectionManager.addEntity(e, loaded)`; `ServerLevel.addPlayer` | the add choke point (phase 1a), before the entity enters the vector | Canceled → not added |
| `EntityLeaveLevelEvent` | end of `ServerLevel.EntityCallbacks.onTrackingEnd`, after `onRemovedFromLevel` | the remove choke point (SPEC-1b), after removal from the vector (or marking, inside a lent loop), with the reason | None |
| `EntityTickEvent.Pre/Post` | `ServerLevel.tickNonPassenger` | each entity loop, per entity, at SPEC-1a's crossing points (phase 2) | Pre canceled → its tick skipped |
| `MobDespawnEvent` | `Mob.checkDespawn` | `mobs/ai.rs:1677`, the loop's despawn step | ALLOW / DENY / DEFAULT read by the check |
| `LivingBreatheEvent` | `LivingEntity.baseTick` | `mobs/ai.rs:847` | `canBreathe`, refill and consume amounts used by `base_tick` (in-turn crossing) |
| `LivingVisibilityEvent` | `getVisibilityPercent` ← `TargetingConditions.test` | `mobs.rs:2125`, `goals.rs:712` | Multiplier (in-turn crossing) |
| `EntityMobGriefingEvent` | `EventHooks.canEntityGrief` (27 callers) | each `rules.bool("mob_griefing")` site | Replaces the rule's value there |
| `LivingIncomingDamage`, `ShieldBlock`, `Damage.Pre/Post` | `hurtServer`, `actuallyHurt` | the bin's hurt paths | Amount and cancel; `DamageContainer` reductions |
| `LivingDeathEvent` | `LivingEntity.die` | `creatures.rs:916` | Canceled → no death |
| `LivingDropsEvent`, `LivingExperienceDropEvent` | `dropAllDeathLoot`, `dropExperience` | `mobs/drops.rs`, `drops.rs:401` | The drop list, the XP amount |
| `LivingFallEvent`, `LivingJumpEvent` | `causeFallDamage`, `jumpFromGround` | `mobs/living.rs:904`, `motion.rs:4605` | Distance and multiplier or cancel; none |
| `LivingChangeTargetEvent` | `Mob.setTarget`, `StartAttacking` | `monster_brain.rs:571`, goal target setters | New target or cancel |
| `FinalizeSpawn`, `PositionCheck`, `SpawnPlacementCheck`, `SpawnClusterSize` | `NaturalSpawner`, `BaseSpawner`, `SpawnPlacements` | `natural_spawning.rs:522`, `spawn_selection.rs:670`, `spawn_rules.rs:318` | Spawn decision |
| `StartTracking` / `StopTracking` | `ServerEntity.addPairing` / `removePairing` | where the hub pairs (`lan/hub.rs`, the tracker), after the pairing packets | None |
| `ItemExpireEvent`, `ItemEntityPickupEvent` | `ItemEntity.tick`, `playerTouch` | item tick, `game.rs:3114` | Extra life; allow or deny |
| `EntityInvulnerabilityCheckEvent` | `isInvulnerableToBase` | `damage_immunity.rs:38` callers | Bool |
| `LivingKnockBack`, `EntityEvent.Size`, `StruckByLightning`, `ProjectileImpact`, `EntityTeleport.EnderEntity`, `EntityMount`, `LivingConversion` | as `events-explained.txt` | as `events-explained.txt` | As vanilla |

## 5. The analysis boot fact (`SPEC-0-analysis.md`)
**Install time, static:** `tools/mod_replaces.py`, extended, writes `entity-facts.json` (format 1) beside `replaces.json`:
- **events:** per event class, the listeners, each with its provable guards. Guards are a side (server, client, both), required and excluded classes on `getEntity()`, and entity types compared by identity. An unprovable guard is `all`.
- **reads and writes:** per vanilla entity method mod code calls, its state group from a fixed table and its static receiver class. An unmapped method fails the analysis until the table names it.
- **routes:** per vanilla entity class, its edge and body mixins with handler names. Per mod class, its overrides.
- **capabilities:** `registerEntity(cap, EntityType.X, …)` calls whose type is a vanilla static field. The rest is left for boot.
- **opaque:** jars that can't be analysed ahead (coremods, class processors). The pack is refused for entity routing, loudly.

**Boot, confirmation only:**
- the bus listener lists (`EventBus.getListenerList(c).getListeners()`) match the static listener set; a listener registered dynamically is added with guard `all`;
- conditional mixins (config plugins) are applied iff their merged handlers exist in the target class;
- each owned entity type's class, from one instance built on a view level;
- capability providers per type, from NeoForge's registry after `RegisterCapabilitiesEvent`;
- attachment types that serialize.

**The boot fact** `entities.bin`, versioned, in the content cache folder: methods, per-type route bitsets, per-type state groups, per-event subject filters, writers with their bridges. The content cache's fingerprint covers it, so it is computed once per mod set. Rust reads it once (`OnceLock`).

## 6. Client: variant (a)
**Feeding:**
- The Java server sends no entity packets. `remote.rs` applies each native entity packet, then appends a feed entry to the next client tick's batch, in arrival order:
  - `ADD`; `DATA` (raw item bytes, decoded by the real serializers, written raw); `EQUIP`; `PASSENGERS`; `EVENT`; `REMOVE`.
- Views live in a Horizon registry that `ClientLevel.getEntity`, `getEntities` and `entitiesForRendering` answer from. Vanilla's section manager stays empty.
- In-process, the feed is ordered against Java-server packets by the hub's per-player send sequence.

**Vanilla client code in Java:**
- **Never:** entity packet handlers, entity ticking, interpolation, the render dispatcher, vanilla renderers, models and layers.
- **On detection only:**
  1. mod renderers' overrides (base bodies bridged to native);
  2. mod models' `setupAnim` (geometry from `RegisterLayerDefinitions`, baked at boot);
  3. layers added through `AddLayers` (renderer layer lists diffed at boot);
  4. `IClientItemExtensions` armour model overrides;
  5. client mixins through §3's hook;
  6. mod types' client overrides;
  7. client events fired by native: join and leave at `remote.rs` add and remove, `EntityTickEvent` at the remote entity tick, render events at the recording pass, only when listened and guard-admitted;
  8. clicks on entities, with native firing the interaction events with the view.

## 7. Client drawing of mod code
- **Model-driven:** per tick, part poses from the overrides at partials 0 and 1, lerped natively, with boot meshes.
- **Free-form submits:** `BlockRenders`' collector per entity.
- **Mod layers on vanilla entities:** native supplies the parent model's poses to the model view.
- **Render events:** fire at the recording pass, which is per tick. Firing them per frame needs per-frame Java, which the rules ban. Decision 4 forbids approximating, so render events are delivered only once the user accepts the tick rate (question 2).

## 8. Mod entity types
- **Boot facts:** type ids, base vanilla class, dimensions, tracking, attributes, overrides, mod data items, complex spawn data.
- **Native:** the base kind, an existing species where the superclass is one. Overrides are routes.
- **Tracking:** mod data items go out as raw values Java packs.
- **Goals:** vanilla goals become their native goal, built from the goal object's fields; mod goals run in Java and are driven by native's selector.

## 9. Phases (each final; one spec each, under about 500 lines)

| # | Phase | Delivers, exactly | Checks |
|---|---|---|---|
| 0 | **Analysis** (SPEC-0-analysis) | `entity-facts.json` at install; its boot confirmation; `entities.bin` in the content cache | Python unit tests on synthetic class files (guards, groups, routes); pack run matches §1.4 |
| 1a | **Re-entrant entity loops** (SPEC-1a) | `Game::lend` and crossing points in the 14 loops that take an entity vector; fixed bounds, removal marks, refresh after a crossing; `MobShared` split; Rust only | Rust crossing tests; parity harnesses unchanged; `tick_mobs` within ±3% |
| 1b | **Native entity lifecycle** (SPEC-1b) | Every add and remove of a non-player entity goes through `level_add` / `level_remove` with origin and reason, at the vanilla moment; Rust only | Site test (no direct vector mutation); Rust order tests; harnesses unchanged |
| 1c | **Live, backed, write-through views** (SPEC-1c, ~600 lines, one piece on purpose) | A view per entity from join to leave; group refresh at crossings; every analysed writer bridged; Java spawns native at once; Java-owned keys saved by asking Java; copy-back deleted | Test-mod checks per writer, identity, persistence; `check` requires bridges |
| 1d | **Join and leave events** (SPEC-1d) | `EntityJoinLevelEvent` (cancellable) and `EntityLeaveLevelEvent` at the choke points in vanilla's order, players included, catch-up at level start | Test-mod checks; hz_scenario `entities.json` |
| 2 | **Per-tick events and tracking events** | `EntityTickEvent` Pre/Post, `MobDespawnEvent`, `LivingBreatheEvent` per entity in exact order at SPEC-1a's crossing points, guard-filtered; `StartTracking`/`StopTracking` at pairing | Test-mod order check (Pre, body, Post per entity, interleaved); count = admitted entities; crossing cost reported |
| 3 | **Routing hook** | Edge and body routes from the boot fact; predicate guard; ae2 ×2 and Jade ×1 routes live | Test-mod mixins on `ItemEntity.tick`/`hurtServer`; M1 shows those handlers alive |
| 4 | **Damage, death, drops, spawn and movement events** | §4's remaining rows, at their sites, with results | Test-mod checks per event |
| 5 | **Client views and events** | Feed entries; registry; client events; entity clicks with views; feed ordering | Client checks; Jade on a mob (user) |
| 6 | **Mod entity types without AI** | Facts, base kinds, own-type routes, data items, persistence | Test TNT subclass; AE2 tiny TNT, baby creeper (user) |
| 7 | **Mod mobs with goals** | Goal bridging; `PathfinderMob` kind | Test mob; robit follows (user) |
| 8 | **Mod rendering, layers, armour** | Boot meshes, pose recordings, layers, armour | Parser tests; Meka-Suit model (user) |

## 10. Risks and measurement
1. **Crossing cost at exact order** (§1.3). Phase 2 reports `EntityTickEvent.Post` crossings a tick and their mean, from `HORIZON_PROFILE`. Accepted when the host-crossing work's figure is ≤ 1 µs. Until then phase 2 ships behind nothing, because exactness doesn't wait. Its profile number is the input for that work.
2. **View creation at join**, a spike at world load. Phase 1d reports `EntityJoinLevelEvent` time (`Profile.event`) and the tick `max_ms` with 200 joins.
3. **View memory.** Phase 1c: `jcmd GC.class_histogram` with 1,000 zombies.
4. **Analysis soundness.** Phase 0 tests each guard pattern, plus a negative per unprovable shape. At boot, the listener set is confirmed and mismatches are logged.
5. **Re-entrant loops changing native results.** Phase 1a: the parity harnesses `mob_ai`, `server_entities`, `server_player_damage`.
6. **Feed vs payload order on the in-process client.** Phase 5: a payload naming a just-spawned entity is found.
7. **Mixin handler name resolution.** Boot fails loudly if a routed handler is missing.

## 11. Open questions
1. Is a pack with an opaque jar (a coremod or class processor) refused for entity routing, as specified, or do you want a manual override?
2. Render events can only fire per tick (per-frame Java is banned). Accept tick-rate firing as exact for Horizon, or keep them unsupported?
3. `LivingVisibilityEvent` and `EntityMobGriefingEvent` need in-turn crossings inside AI code (phase 1a's re-entrancy). Is that refactor acceptable, or should those two stay unsupported until a pack needs them?

## Corrections (from me, carrying the user's earlier answers)
- **Render events:** the HUD rule, as the user chose. Record per tick, and also per frame while the recorded result keeps changing; Java is called per frame only then. Per-frame Java isn't banned.
- **Mob griefing and visibility events:** call Java at the site inside the mob AI loop when a loaded mod listens, as the user chose. SPEC-1a's re-entrancy is what allows it.
- **Opaque mods (user, 2026-11-04):** a pack with a coremod or class transformer that can't be analysed ahead is refused. Java mods don't start, and the message names the mod and its transformer. There's no override.
- **User, 2026-11-04:**
  - Passengers tick inside their vehicle's turn as part of phase 2's merge into one vanilla-ordered entity loop. Until then riders are placed exactly but tick in their kind's loop.
  - Unify movement: a spec (SPEC-move-unify) moves orbs', boats' and projectiles' own ticks onto native's generic movement port, checked against the vanilla harnesses, so there's one port of `Entity.move`.
