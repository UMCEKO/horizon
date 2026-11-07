# W13: the remaining server events (design, revision 1)

Status: design only, nothing implemented. Specs `SPEC-1.md` .. `SPEC-15.md`, in landing order (section 7).
Binding: `RULES.md`, `COVERAGE-SPEC.md` (principle 0, W13), `entities/DESIGN.md` (W9 owns entity events), `w12/DESIGN.md`
(W12 owns mixin routing). Scratch evidence in this folder: `javap/` (patched-jar call sites), `refs/refs.txt` (event
classes each pack jar references), `sites.py`, `seq.py` (the scripts that produced them).

## 1. The gap, with evidence

### 1.1 What Horizon fires today
`grep` of the Java host (`mods/horizon/src/java-mods-host/{boot,host,plugin}`, `java-mods` at cc1b066b6) for event
constructors and NeoForge hook calls: server side it posts only lifecycle (`ServerLifecycleHooks.*`), server, level and
player ticks, `PlayerLoggedIn`/`LoggedOut`, `RightClickBlock`, `RightClickItem`, `AttackEntity`, `BreakBlockEvent`
(`CommonHooks.fireBlockBreak`), chunk events (`ChunkEvent`, `ChunkDataEvent`, `ChunkWatchEvent`), `LevelEvent.Load/Save/Unload`,
`OnDatapackSyncEvent`, `RegisterCommandsEvent` (probe only), `ModifyCustomSpawnersEvent`. Everything else NeoForge posts
from vanilla server code is posted only when Java itself runs that vanilla code, which for native-owned gameplay is never.

Worse, six vanilla methods are **bridged to native** (`G/java_bridges/*.rs`) whose Java body is where NeoForge posts an
event, so even a mod's own call loses it: `ServerLevel.explode` (`ExplosionEvent`), `ServerLevel.updateNeighborsAt`
(`NeighborNotifyEvent`), `ServerLevel.gameEvent` (`VanillaGameEvent`), `ServerPlayer.awardStat` (`StatAwardEvent`),
`ServerPlayer.setGameMode` (`PlayerChangeGameModeEvent`), `Block.dropResources` (`BlockDropsEvent`). Rule for this
design: **the native body posts what the Java body would**, at the same native site native's own callers use, so one
site serves both paths (Java → bridge → native site → nested crossing back into Java, which `bench_nested` covers).

### 1.2 What real NeoForge fires (M1, measured)
`/home/umceko/hz-poc/agents/tools2/explain/run2/report.json` (11-04, `basic` scenario, install-rs, real client vs Horizon):
91 event classes fire on Forge and never on Horizon. The server ones outside W9's entity list, with Forge's counts:

| Event | Forge count | Notes |
|---|---|---|
| `ChunkTicketLevelUpdatedEvent` | 60,649 | Mekanism listens |
| `VanillaGameEvent` | 3,075 | no listener in the bar packs |
| `StatAwardEvent` | 2,137 | no listener |
| `CropGrowEvent.Pre` / `.Post` | 1,608 / 224 | balm maps it (runtime listener possible) |
| `BlockEvent.NeighborNotifyEvent` | 264 | no listener (Mekanism Generators and Tom's Storage *post* it) |
| `AdvancementEvent.AdvancementProgressEvent` / `EarnEvent` | 30 / 2 | architectury, lootr |
| `PlayerChangeGameModeEvent`, `NameFormat`, `LoadFromFile`, `SaveToFile`, `PlayerSwitchHotbarSlotEvent`, `PlayerContainerEvent.Close` | 2–4 each | see section 3 |
| `CommandEvent` | 8 | W6 (section 3) |

Since then the chunk events, `LevelEvent.Save/Unload` and `ServerStopping/Stopped` were added (host grep above). Events
the `basic` scenario never triggers (explosions, pistons, placement, farmland, respawn, dimension change, sleep, XP) are
missing by construction: no host site posts them.

### 1.3 Who listens (the finish bar's packs)
- **Static listeners** (`mod_replaces` reports `finish/rep-rs/replaces.json`, 12 mods; `finish/rep-showcase/replaces.json`,
  47 mods; `tools2/replaces/out-top50/replaces.json`, 78 mods), W13 classes only:

| Event | Listening mods |
|---|---|
| `ExplosionEvent.Start`, `.Detonate` | architectury |
| `BlockEvent.EntityPlaceEvent` | architectury |
| `BlockEvent.FarmlandTrampleEvent` | architectury |
| `PistonEvent.Pre`, `.Post` | architectury |
| `ChunkTicketLevelUpdatedEvent` | mekanism (install-rs and showcase) |
| `PlayerEvent.PlayerChangedDimensionEvent` | mekanism, architectury, creativecore, shulkerboxtooltip, sophisticatedbackpacks, travelersbackpack |
| `PlayerEvent.PlayerRespawnEvent` | mekanism, architectury, creativecore, sophisticatedbackpacks, travelersbackpack |
| `PlayerEvent.Clone` | architectury, carryon, creativecore (and NeoForge's own attachment copy) |
| `AdvancementEvent.AdvancementEarnEvent` | architectury, lootr |
| `PlayerSetSpawnEvent`, `PlayerWakeUpEvent` | travelersbackpack |
| `PlayerXpEvent.PickupXp` | travelersbackpack |
| `ItemTossEvent`, `UseItemOnBlockEvent`, `PlayerContainerEvent.Open/Close`, `CommandEvent`, `ServerChatEvent` | architectury |

- **Generic listeners.** balm, klf and puzzleslib register through `addListener(Class, Consumer)`: the report shows them on
  `net/neoforged/bus/api/Event`, so only the bus knows their classes at runtime. Their jars reference (constant pool,
  `refs/refs.txt`): balm `CropGrowEvent.Pre/Post`, `EntityTravelToDimensionEvent`, `PlayerContainerEvent`; puzzleslib
  `BonemealEvent`, `ExplosionEvent`, `PlayLevelSoundEvent`, `GameRuleChangedEvent`, `LevelEvent.PotentialSpawns`,
  `BlockDropsEvent`, `PlayerWakeUpEvent`, `PlayerXpEvent`, `ArrowLooseEvent`; Clumps `PlayerXpEvent.PickupXp`.
- **Posters, not listeners** (javap: the reference is in a method body, no handler signature): Mekanism Generators'
  logic adapters and Tom's Storage post `NeighborNotifyEvent`, Mekanism's `WorldUtils` posts `BlockDropsEvent`, Carry On's
  `CommonEvents` posts `EntityPlaceEvent`, AE2's quartz knife posts `PlayerDestroyItemEvent`. Java-posted events already
  reach Java listeners; they need nothing from W13.

### 1.4 Where vanilla fires them (patched server jar, `javap/sites.txt`)
`javap -c` of every patched class that references NeoForge (388 classes) gives each hook's callers. The ones W13 ports,
with the order inside the vanilla method (`javap/seq.py`):
- `ServerLevel.explode`: `new ServerExplosion` → `onExplosionStart` (cancel returns) → `explode()`. Inside `explode`:
  `gameEvent` → `calculateExplodedPositions` → `hurtEntities(blocks)`: `getEntities(box)` → `onExplosionDetonate(level,
  explosion, entities, blocks)` → per entity damage, then `getExplosionKnockback` → `pushFromExplosion`. Horizon's
  `explosion.rs` `server_explode_as` already follows that order (native-mod hooks `native_explosion_prime`,
  `native_explosion_detonate`, `native_explosion_knockback` sit at the same points).
- `PistonBaseBlock.triggerEvent`: signal re-check (may `setBlock` and return) → `onPistonMovePre` (cancel returns false) →
  move → `onPistonMovePost` → return true. `pistons.rs` `piston_trigger_event` lines 703–712 are the re-check.
- `ItemStack.useOn` → `CommonHooks.onPlaceItemIntoWorld`: `captureBlockSnapshots = true`, `Item.useOn`, capture off; on a
  consuming result `onMultiBlockPlace` (more than one snapshot) or `onBlockPlace` per snapshot; cancel → restore the
  snapshots in reverse and the stack; else per snapshot `BlockState.onPlace` + `Level.markAndNotifyBlock`. While capturing,
  the patched `Level.setBlock` records `BlockSnapshot.create` **before** the chunk write and skips `markAndNotifyBlock`;
  the patched `LevelChunk.setBlockState` skips `onPlace` (bytecode 530–554).
- `FarmlandBlock.fallOn` → `CommonHooks.onFarmlandTrample(level, pos, DIRT, fallDistance, entity)` → `turnToBaseBlock`.
- `BoneMealItem.applyBonemeal` → `fireBonemealEvent` first (cancel returns `isSuccessful()`), then vanilla.
- `PlayerList.respawn`: `removePlayerImmediately` → `firePlayerRespawnPositionEvent` → `new ServerPlayer` →
  `restoreFrom` (its last step `onPlayerClone`) → packets → `addRespawnedPlayer` → `firePlayerRespawnEvent`.
- `ServerPlayer.teleport` / `Entity.teleport`: `onTravelToDimension` (cancel) first; `firePlayerChangedDimensionEvent` last.
- `CropBlock.randomTick` (and 9 other growers): light, age, `nextInt` draw → `canCropGrow(level, pos, state, drawn)` →
  `setBlock` → `fireCropGrowPost`.
- `ServerLevel.updateNeighborsAt`, `updateNeighborsAtExceptFromFacing`, `Level.updateNeighborsAt`,
  `DiodeBlock.updateNeighborsInFront` → `onNeighborNotify` (cancel skips the updates).
- `ChunkMap.updateChunkScheduling` → `fireChunkTicketLevelUpdated` at its end, whenever the old or new level is loaded
  (`ChunkLevel.isLoaded`, level ≤ `MAX_LEVEL` = 44).

### 1.5 Live boot: not run
A boot can't show the bus's listener lists (no op dumps them, and adding one is SPEC-1). The static report, the jar
references and the M1 run above are the evidence; SPEC-1's census is the runtime measure from then on. Note for the lead:
`/home/umceko/hz-poc/agents/w12/install/mods/` is **empty** (the `client/mods` and `server/mods` symlinks dangle), so the
showcase install doesn't boot as it is; its jars are in `/home/umceko/hz-poc/play/pack3` and `top50/pack/mods`.

## 2. Scope (what the finish bar needs)

| Event(s) | Native site (Horizon) | Context | Listeners | Spec |
|---|---|---|---|---|
| census of all W13 classes, the gate, the two ops | `java_world_phase` | at server start | – | 1 |
| M1 scenario and the listened filter | `tools/` | – | – | 2 |
| `ExplosionEvent.Start`, `.Detonate`, `ExplosionKnockbackEvent` | `explosion.rs` `server_explode_as`, `explosion_hurt_entities` | Game | architectury, puzzleslib? | 3 |
| `PistonEvent.Pre`, `.Post` | `game.rs` block-event step, `pistons.rs` split | Game, per block event | architectury | 4 |
| `EntityPlaceEvent`, `EntityMultiPlaceEvent` (native placement) | `game.rs` `use_hand` (`place_block`) | Game | architectury | 5 |
| the same for placements Java runs (mod items) | `java_bridges/world.rs` | bridge | architectury | 6 |
| `FarmlandTrampleEvent`, `BonemealEvent` | `landing_change_cancelled`, mob landing, `growth.rs` `use_bone_meal`, `dispenser.rs` `dispense_bone_meal` | Game | architectury, puzzleslib? | 7 |
| `PlayerEvent.Clone`, `PlayerRespawnEvent`, `EntityTravelToDimensionEvent` (players), `PlayerChangedDimensionEvent` | `lan_actions.rs` `guest_respawn`, `server_travel.rs` | Game | 6 mods + NeoForge | 8 |
| `PlayerSetSpawnEvent`, `PlayerWakeUpEvent`, `AdvancementEvent.*`, `GameRuleChangedEvent` | `try_sleep`, `respawn_anchor_used`, `cmd_spawnpoint`, `wake_up`, `wake_guest_from`, `advancements.rs`, `set_game_rule` | Game | travelersbackpack, architectury, lootr, puzzleslib? | 9 |
| `PlayerXpEvent.PickupXp/XpChange/LevelChange` | `guest_orbs`, `xp_change`, `give_orb_points`, `give_levels` | Game | travelersbackpack, Clumps, puzzleslib? | 10 |
| `ItemTossEvent`, `ArrowLooseEvent` | `throw_from_eyes`, `release_bow`, `shoot_crossbow` | Game | architectury, puzzleslib? | 11 |
| `UseItemOnBlockEvent` (three phases) | `use_hand` | Game | architectury | 12 |
| `ChunkTicketLevelUpdatedEvent` | `ticket_levels.rs` `ChunkTickets::update` caller | Game, batched per update | mekanism | 13 |
| `CropGrowEvent.Pre/Post` + cascade crossings | horizon-world growers, `GameLevel` | in a level cascade | balm? | 14 |
| `NeighborNotifyEvent` | horizon-world `update_neighbors_at` and kin | in a level cascade | none seen | 15 |

`?` = only a generic library references it; SPEC-1's census decides at runtime. SPEC-14/15 land last (decision D1).

## 3. Out of scope, with the owner

| Event(s) | Why not here | Owner |
|---|---|---|
| Every `EntityEvent`/`LivingEvent` subclass, `StartTracking`/`StopTracking`, `ItemEntityPickupEvent`, spawn events, `LevelEvent.PotentialSpawns`, `EntityMobGriefingEvent` | entity subjects, entity loops | W9 (`entities/DESIGN.md` §4) |
| `ItemCraftedEvent`, `ItemSmeltedEvent`, `AnvilUpdate`, brewing, enchanting, `TradeWithVillagerEvent`, `LootTableLoadEvent`, `ItemAttributeModifierEvent` | recipe, loot and item-data hooks | W11 |
| `CommandEvent`, `ServerChatEvent` | both need the chat line or command to reach Java first (signed-chat decoration, a Brigadier parse of every command); that is the mod-command dispatch work | W6 / "mod commands" (COVERAGE-SPEC W13 item 1) |
| `PlayerContainerEvent.Open/Close` | the event carries the open `AbstractContainerMenu`; native's vanilla menus have no Java menu, and a faithful one is W12's vanilla container view family | W12 (vanilla block entity views) then a one-site spec on SPEC-1's plumbing |
| `PlayLevelSoundEvent.AtPosition/AtEntity` | puzzleslib only; native plays server sounds from several paths with no single seam | after a "server sound broadcast" seam exists (W8/W17) |
| `PlayerRespawnPositionEvent`, `LoadFromFile`, `SaveToFile`, `NameFormat`, `TabListNameFormat`, `PlayerChangeGameModeEvent`, `PermissionsChangedEvent`, `PlayerSwitchHotbarSlotEvent`, `StatAwardEvent`, `VanillaGameEvent`, `CanPlayerSleepEvent`, `CanContinueSleepingEvent`, `SleepFinishedTimeEvent`, `FluidPlaceBlockEvent`, `CreateFluidSourceEvent`, `BlockDropsEvent`, `BlockGrowFeatureEvent`, `AlterGroundEvent`, `BlockToolModificationEvent`, `PortalSpawnEvent`, `NoteBlockEvent.*`, `LevelEvent.CreateSpawnPosition`, `DifficultyChangeEvent`, `PlayerDestroyItemEvent`, `CustomClickActionEvent`, `ClientInformationUpdatedEvent` | no listener in any bar pack (static report and jar references) | none until one appears: SPEC-1's census logs `listened but unsited` naming the mod, and each is then one site on SPEC-1's plumbing (cascade ones on SPEC-14's) |

## 4. The approach

### 4.1 Fire only what is listened, decided at server start (SPEC-1)
- Principle 0: an event no one listens to is not built. The decision is per event class, read from the bus's listener
  lists **at each server start** (after `ServerStartedEvent`): balm and puzzleslib subscribe late, JEI-style, and a
  boot-time read would miss them. Reading the lists once at the moment the server starts is not polling; it is
  `Shells.wanted`'s rule at a coarser moment.
- Every listener counts except Horizon's own (`dev.horizonlab.javamods.*` outside the test mod): NeoForge's internal
  listeners have effect (its `Clone` attachment copy) and must be served.
- Rust keeps the bits as a `u64` (`JavaEvent` ids) in the server thread's state; a site's whole cost without a listener
  is one bit test. Without `--features java-mods` the gate is a `const false`.
- **Census report:** one log line per listened class, and a loud warning for a listened class this build has no site for
  (Rust sends its `SITED` mask). That is the M1 completeness check for W13 and lists every out-of-scope event the day a
  pack starts listening to it.

### 4.2 Post through NeoForge's own hooks, on views
Java calls the real `EventHooks`/`CommonHooks` method where one exists (`onExplosionStart`, `onPistonMovePre`,
`onBlockPlace`, `fireBonemealEvent`, `onPlayerClone`, `firePlayerRespawnEvent`, `onTravelToDimension`, ...), so NeoForge's
own result logic runs. Arguments are views: the level view (`GameBridge.level`), the acting player (`GameBridge.fill`
through the use header, guests through `as_guest`), entity views by id (`EntityViews`, W9 1c), and real vanilla value
objects built from the wire (`BlockPos`, `BlockState` by id, `Vec3`, `ServerExplosion`, `BlockSnapshot`,
`AdvancementHolder` from the server's real advancement manager). Each post goes through `GameBridge.safely`.

### 4.3 Results come back and native applies them
Every site reads the reply and changes its own native behaviour: a cancel skips what vanilla skips, a changed value
(`ArrowLoose` charge, `XpChange` amount, Detonate's lists, knockback vector, placement restore) is used by native.
Nothing is copied back from a view.

### 4.4 Three contexts
- **Game** (SPECs 3, 5–13): the site is a `Game` method with no level borrow alive; the crossing is synchronous.
- **Block-event step** (SPEC-4): vanilla's piston event sits inside `runBlockEvents`. When `PistonEvent` is listened, Game
  drives the block-event queue one event at a time, and the piston's `triggerEvent` is split at the exact point vanilla
  posts `Pre`. Without a listener, the existing single `run_block_events` call runs.
- **Level cascade** (SPECs 14, 15): sites inside horizon-world code that runs under `with_level`, where `GameLevel` holds
  `&mut World` and `&mut Ticks`. A crossing there must not hand Java `&mut Game` (the borrow is alive). SPEC-14 adds a
  **cascade context**: the crossing installs the running `GameLevel` and `Ticks` as the bridge context, every level bridge
  gets a cascade body that runs on them (re-entrantly, exactly as vanilla's nested `setBlock` does), and a bridge without
  one throws `IllegalStateException` naming itself and the event. Decision D1.

### 4.5 Two ops cover every site
- `level_event` (through `java_level_call`'s header: dimension, game time): `byte id`, the event's tail; reply `bool
  canceled`, `bytes result`, effects.
- `player_event`: `byte id`, then the use header (so the acting player's view is filled, guests through `as_guest`), tail;
  reply `bool canceled`, `bytes result`, then the `use` reply's xp, stacks and effects.
Each event's tail and result layout is in its spec.

## 5. Decisions (with reasons)
- **D1 (for the user). Cascade crossings serve level bridges only.** Inside a block update a Java listener may read and
  write blocks, schedule ticks, read signals, emit level and game events; a call to any other bridge (entities,
  players, inventory) throws, naming the bridge and the event, logged once per bridge. The alternative, a crossing with
  all of `Game`, needs the cascade to run on `Game` itself (the doc's "hand Java the running Sim"), a refactor of every
  `LevelAccess` site across horizon-world, outside W13. Recommended: D1 as specified, SPECs 14/15 landing only when
  SPEC-1's census on the bar packs shows `CropGrowEvent` or `NeighborNotifyEvent` listened.
- **D2. Listened is decided per server start, not per call.** A listener added during play (after `ServerStarted`) is
  not served until the next start. Vanilla posts regardless, so a per-call read would be exact but costs a bus lookup per
  site; per start matches how every pack in the bar registers (mod construction, `ServerStarting`).
- **D3. NeoForge's capture semantics for placements, only while listened.** With a placement listener, native places in
  NeoForge's order (block written, `setPlacedBy`, sounds, then the event, then `onPlace` and neighbour updates); without
  one, vanilla's order stays (vanilla is the spec for native, and nothing observes the difference).
- **D4. Ticket level events are batched per ticket update, in vanilla's propagation order approximated as ascending new
  level then chunk position.** Vanilla's order within one `runAllUpdates` is its fixed-point queue's; the event API
  exposes no cross-chunk relation, and one crossing per batch keeps 60k events a session affordable.
- **D5. Player views for events need W9 SPEC-1d.** Explosion entity lists and every player event about a player who is
  not the acting one need every player's view (`PLAYERS`). Specs 3 and 8 depend on it; until then they don't land.
- **D6. One site serves native and bridged callers** (section 1.1): no Java-side re-post for bridged methods.

## 6. Questions for the user
1. D1: accept level-only cascade crossings for `CropGrowEvent`/`NeighborNotifyEvent`, or keep both out until the
   cascade runs on `Game`?
2. D3: is NeoForge's capture order (neighbour updates after the event) acceptable for native placements while a mod
   listens? It is what real NeoForge does for every placement.
3. `PlayerContainerEvent` waits on W12's vanilla container views (section 3). Raise it in W12's order (architectury
   forwards it to every architectury mod's `MenuEvent`)?

## 7. Order and dependencies

| Spec | Title | Depends on |
|---|---|---|
| 1 | Census, gate, `level_event` and `player_event` ops | – |
| 2 | M1 scenario and the listened filter | 1 |
| 3 | Explosions | 1, W9 SPEC-1d |
| 4 | Pistons | 1 |
| 5 | Native placement events with capture | 1 |
| 6 | Placements Java runs honour the capture | 1, 5 |
| 7 | Farmland trample and bone meal | 1 |
| 8 | Respawn, clone, dimension travel | 1, W9 SPEC-1d |
| 9 | Spawn point, wake up, advancements, game rules | 1 |
| 10 | XP pickup, change, level change | 1, W9 SPEC-1c (views, merged) |
| 11 | Item toss, arrow loose | 1, W9 SPEC-1c |
| 12 | `UseItemOnBlockEvent` | 1 |
| 13 | Chunk ticket levels | 1 |
| 14 | Cascade crossings and `CropGrowEvent` | 1, D1 |
| 15 | `NeighborNotifyEvent` | 14 |

Specs 3–13 are independent of each other. Each adds its ids to Rust's `SITED` mask.

## 8. Cost and risks
- Without a listener: one bit test per site (`java_listens`). With one: one crossing per vanilla post (about 0.1 µs
  empty plus view fill; the use header's player fill is the heavy part, measured by W12 SPEC-3).
- Hot sites when listened: chunk tickets (batched, D4), pistons (one `with_level` per block event while listened),
  crops (one crossing per grower random tick that reaches the hook: 1,608 `Pre` in the M1 run), neighbour
  notifies (every `updateNeighborsAt`: the census shows none today; SPEC-15 reports the rate before it lands).
- Risk: a bridged method called from Java inside a cascade crossing would re-enter `with_level`. SPEC-14 gives the level
  bridges cascade bodies that act on the running cascade instead (no `with_level`, no `&mut Game`), every other bridge
  throws there, and the test mod checks a nested `setBlock` from a listener.
- Comments in all code: one line at most, only for a non-obvious why (each spec repeats it).
