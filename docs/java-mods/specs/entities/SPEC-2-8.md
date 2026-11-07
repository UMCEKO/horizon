# SPEC-2-8: entities phases 2 to 8, in small steps (index)

Design: `DESIGN.md` (§0 decisions, §3 routing, §4 events, §6–§8, Corrections). Base: the 1c branch merged (1c, 1c2a, 1c2c 1–12, 1c2d, 1c2b) **and SPEC-1d** (join and leave events, player views per level). Every step is one review of about 300 changed lines or less, final for what it covers, and testable alone.

Step files: `SPEC-<phase>-<n>.md`. This index has the name `SPEC-2-8.md`, so phase 2's eighth step is `SPEC-2-8a.md` (reviews 2-8a and 2-8b). Implement a phase's steps in order unless the table says otherwise.

## Decisions every step follows

### E1. One tick list per level, as vanilla's `EntityTickList`
`ServerLevel.tick` (`ServerLevel.java:431–451`) ticks **every** entity of the level, players included, in one loop, in the order each one started ticking (`EntityCallbacks.onTickingStart` → `EntityTickList.add`, a linked map). Native ticks kind by kind in separate passes. Phase 2 replaces those passes with one loop over a native `TickList` (2-5), in vanilla's order. Two ledger skips that allowed the per-kind passes are reversed, with this phase as their P2 evidence:
- `EntityTickList.forEach` `skip:java` (`tools/port_map/classes-05.tsv:11`);
- `PersistentEntitySectionManager.updateChunkStatus` `skip:other` (`classes-05.tsv:27`).

### E2. Every entity is a kind
`level_add` / `level_remove` (SPEC-1b) cover 15 kinds. Falling blocks, tridents, rockets, dragon fireballs, area effect clouds, evoker fangs, ominous item spawners, thrown potions, eyes of ender, fishing hooks and leash knots live outside them: ids given late (`lan_assign_ids`), no join, no view, no place in a tick list. Steps 2-2 to 2-4 make them kinds, as `SPEC-1c2a.md` §1 did for TNT and bolts. Projectiles and crystals also get their ids at construction there.

### E3. One turn per entity, effects inside the turn
A turn is vanilla's `tickNonPassenger(e)` or `tickPassenger(v, e)`. Everything an entity's `tick()` does in vanilla happens in its turn: damage it deals, deaths it causes, items it picks up, sounds, game events. Native defers part of it to after the pass (`handle_mob_events(events)`, `mob_died` of `dead`, `tick_allays`, `piglin_pickups`…). Steps 2-6 to 2-12 move each into the turn of the entity whose vanilla tick does it, with a turn function per kind over a per-pass context (`TickPass`, 2-6).

### E4. Events fire through NeoForge's own hook, on the refreshed view
At each site, Java calls **the hook the patched vanilla calls** (javap of `minecraft-server-patched-26.3.0.101.jar`), with the same arguments, on the subject's view:

| Vanilla site (patched) | Hook |
|---|---|
| `ServerLevel.tickNonPassenger`, `Entity.rideTick` | `EventHooks.fireEntityTickPre(Entity)` → `EntityTickEvent$Pre` (`isCanceled`); `EventHooks.fireEntityTickPost(Entity)` |
| `Mob.checkDespawn` | `EventHooks.checkMobDespawn(Mob)` → `boolean`: true returns at once |
| `LivingEntity.baseTick` (replaces vanilla lines 441–463) | `CommonHooks.onLivingBreathe(LivingEntity, ServerLevel, int consume, int refill)` |
| `ServerEntity.addPairing` (after the bundle and `startSeenByPlayer`) | `EventHooks.onStartEntityTracking(Entity, Player)` |
| `ServerEntity.removePairing` (after `stopSeenByPlayer` and the remove packet) | `EventHooks.onStopEntityTracking(Entity, Player)` |

- Nobody reads NeoForge's code. A hook's **reads and writes** come from the hook-closure analysis (2-1, a tool over the jar, as SPEC-0 scans mods): its reads join the subject's refresh mask, and every writer it reaches is a bridge, or `java_mods.py check` fails.
- A hook that **replaces vanilla code** (`onLivingBreathe` replaces the air block) runs in Java **only for admitted subjects**; native skips its port of the replaced lines for them. Otherwise native runs vanilla's lines and Java isn't called.
- **Guard filter:** before any crossing, one bit: `facts.hook_admits(hook, type, false)` (2-1). No admitted listener: no crossing.

### E5. Tracking pairs at vanilla's moments
`ChunkMap.addEntity` pairs at once, `ChunkMap.removeEntity` unpairs at once (before `EntityLeaveLevelEvent`), `ChunkMap.tick` runs before the entity loop (inside `ServerChunkCache.tick`), `ChunkMap.move` at the player's move. Steps 2-15 and 2-18.

### E6. Deprecated in-frame mode
It keeps compiling and gets nothing new. Its host is not in the tick list.

### E7. Measurement
Each event step reports the op's calls per tick and mean (`HORIZON_PROFILE`, `profile_report.py`) with the 360-mob scenario (`/home/umceko/hz-poc/agents/entities/mobs.json`). The no-behaviour-change steps keep `tick_mobs` (later `tick_entities`) within ±3%.

## Phase 2: per-tick events in exact order, the merged loop, tracking events

| # | Step | Lines | Proves it |
|---|---|---|---|
| 2-1 | Hook-closure analysis: what NeoForge's hooks read and write, into `entity-facts.json` and `entities.bin` | ~250 | Python tests on javac classes; pack run; `check --analysis` |
| 2-2 | Kinds: falling blocks, tridents, rockets; ids at construction | ~280 | `game_tests` join/leave; site test; harnesses; scenario `kinds.json` |
| 2-3 | Kinds: dragon fireballs, clouds, fangs, ominous spawners, thrown potions, eyes of ender | ~300 | same |
| 2-4 | Kinds: fishing hooks, leash knots | ~250 | same; `lan_e2e` fishing, lead |
| 2-5 | `TickList`: `EntityTickList` and ticking visibility (no behaviour change) | ~250 | order tests |
| 2-6 | `TickPass` and `turn_mob` (no behaviour change) | ~300 | `mob_ai`; timing ±3% |
| 2-7 | A mob's effects inside its turn | ~220 | order tests; harnesses; scenario `mob_turns.json` |
| 2-8a, 2-8b | The after-pass mob work into the mob's turn (`SPEC-2-8a.md`) | 2 × ~150 | per-row tests; harnesses |
| 2-9 | Turns: items, orbs, projectiles, thrown potions, tridents, rockets, fangs, fireballs, clouds, eyes (no behaviour change) | ~300 | harnesses |
| 2-10 | Turns: TNT, falling blocks, bolts, boats, carts, stands, frames, paintings, cushions, crystals, spawners, others, dragon, hooks, knots (no behaviour change) | ~300 | harnesses |
| 2-11 | `baseTick`/`move` work done in passes moves into turns: portals, `checkInsideBlocks`, sculk steps | ~250 | portal and sculk harnesses |
| 2-12 | Players in the list: `ServerPlayer.tick` as the player's turn | ~250 | `dedicated_server`; `lan_e2e` |
| 2-13 | The merged loop: `tick_entities` at vanilla's place; despawn, frozen, range and vehicle checks; `tickPassenger`/`rideTick` (riders tick in their vehicle's turn) | ~300 | order tests; entity harnesses; riding `lan_e2e`; scenario `entity_order.json` |
| 2-14 | The riding special cases the per-kind passes needed are deleted | ~200 | riding tests; riding `lan_e2e`; scenario `riding.json` |
| 2-15 | Tracking at vanilla's moments | ~300 | tracker tests; `lan_e2e`; `server_entities` |
| 2-16 | `EntityTickEvent` Pre and Post | ~220 | test-mod order and count checks; profile |
| 2-17 | `MobDespawnEvent` and `LivingBreatheEvent` (and whatever else `onLivingBreathe` posts) | ~260 | test-mod checks |
| 2-18 | `StartTracking` and `StopTracking` | ~150 | test-mod checks; scenario |

Order: 2-1 any time before 2-16. 2-2, 2-3, 2-4 in any order (one merge at a time: they all touch `Kind`), before 2-5. Then 2-5 → 2-14 in order. 2-15 after 2-13. 2-16 after 2-14; 2-17 after 2-16; 2-18 after 2-15 and 2-16.

## Phases 3 to 8

| # | Step | Lines | Proves it |
|---|---|---|---|
| 3-1 | Entity receivers for W12's gate (`Recv::Entity`, admitted entity types); no separate hook (W12 decision U4) | ~200 | `java_routing` tests; `RoutingReachesCheck`; W12 lint |
| 3-2 | ae2 `ItemEntity.tick`/`hurtServer` and Jade `addEffect` gated; test-mod mixins | ~220 | test-mod route checks; M1 handler counts |
| 4-1 | A living entity dies at the killing blow (native) | ~250 | `game_tests`; harnesses |
| 4-2 | Crossings from inside a mob's tick: shared mode, deferred writes, `in-turn-writer` check | ~280 | host tests; `game_tests` |
| 4-3 | Damage sessions; `LivingIncomingDamageEvent`; `EntityInvulnerabilityCheckEvent` | ~280 | test-mod checks |
| 4-4 | Shield block, reductions, `Damage.Pre`/`Post`, armour hurt | ~280 | test-mod checks |
| 4-5 | `LivingDeathEvent`, totem, `LivingDropsEvent`, `LivingExperienceDropEvent` | ~280 | test-mod checks |
| 4-6 | Fall, jump, knockback, size, ender teleport events | ~260 | test-mod checks |
| 4-7 | Spawn events (finalize, position, placement, cluster size, potential spawns) | ~280 | test-mod checks |
| 4-8 | Item expire and pickup, lightning, projectile impact, mount, conversion, target, heal, effects (4-8a/4-8b if over) | ~300 | test-mod checks |
| 4-9 | Mob griefing and visibility | ~220 | test-mod checks |
| 5-1 | The Java client's entities fed by native packets, in order; `ClientLevel` answers from them | ~300 | encoder tests; `ClientEntitiesCheck`; client checks |
| 5-2 | Client join, leave and tick events | ~200 | client checks |
| 5-3 | Entity clicks through the Java client | ~180 | user steps (Jade, villager) |
| 6-1 | Mod entity types as boot facts (base kind, dims, tracking, attributes, data items, overrides) | ~280 | host tests; test-mod boot check |
| 6-2 | A mod type spawns, ticks, tracks and saves as its base kind | ~300 | `game_tests`; test-mod checks |
| 6-3 | A mod type's overrides as W12 override routes; `super` → native | ~280 | test TNT; AE2 tiny TNT, baby creeper (user) |
| 6-4 | Mod data items and complex spawn data on the wire | ~250 | client checks |
| 7-1 | A mod mob's goals listed at join; vanilla goals made native | ~280 | `java_goals` tests; test-mod check |
| 7-2 | Native's selector drives Java goals (selector phase split, write-through) | ~300 | test mob follows |
| 7-3 | Navigation, control, target, sensing and random bridges; `PathfinderMob` kind | ~280 | test mob fights; robit (user) |
| 8-1 | Mod entity models and renderers as boot meshes | ~280 | parser tests; boot check |
| 8-2 | Model-driven entities: poses per tick, native lerp, per-frame only when non-linear (HUD rule) | ~300 | recording tests; user |
| 8-3 | Free-form renderers through `SubmitCapture` | ~250 | unit check; user |
| 8-4 | Mod layers on vanilla entities; armour models (8-4a/8-4b if over) | ~300 | client checks; Meka-Suit (user) |
| 8-5 | Entity render events (HUD rule) | ~220 | client checks; user |

Order: phase 3 after W12 SPEC-1–4 and phase 2. Phase 4: 4-1, 4-2 first, then 4-3 → 4-4 → 4-5; 4-6 to 4-9 after 4-2 in any order. Phase 5 after 1d (independent of 3–4). Phase 6 after phases 2, 3 and 5-1. Phase 7 after 6 and 4-2. Phase 8 after 5-1 and 6.

## Rules for the implementer of any step
- Port from vanilla's source named in the step (`.local/minecraft/26.3/sources/`), not from these notes, when they differ. Report the difference.
- Never read NeoForge or mod source or decompiled classes. `javap -p`/`-s` signatures and the analysis tools' outputs only.
- "No behaviour change" steps leave every parity harness and `lan_e2e` check they touch identical. Run what `python3 tools/parity.py --changed-files <files> --list` names.
- A step that changes behaviour does so toward vanilla only, and names in its report each harness whose numbers moved and why the oracle agrees.
- Comments: one line, only for a non-obvious why.

## Decisions on the phase-2 questions (coordinator, 2026-11-05, from the user's rules)
1. 2-1's bytecode analysis of NeoForge hooks is fine (bytecode, not source).
2. 2-15 moves sendChanges before the entity loop as vanilla does; the one-tick latency is vanilla's.
3. Results moving toward vanilla in 2-7, 2-8 and 2-11 are accepted where the oracle agrees.
4. A despawned entity still ticks its turn detached, as vanilla.

## Decisions on the phase 4–8 questions (coordinator, 2026-11-05, the user's no-debt rule)
1. **4-2:** no deferral. Do the refactor so MobWorld holds no borrow of Game, and writes from Java inside a mob's turn apply at the call (about 1,000 mechanical lines; split it into steps).
2. **5-2:** client-side EntityTickEvent.Pre cancels must take effect. When a mod listens, native's client asks Java before ticking that entity (now cheap at about 90 ns per call).
3. **6-1:** a mod type whose vanilla base has no native kind means native ports that base kind (W17 P2 evidence); no "can't spawn" gap.
4. **4-7:** level random draws in Java must use native's level random through a bridged native; no Java copy.
