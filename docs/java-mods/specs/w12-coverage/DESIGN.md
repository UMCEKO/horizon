# W12 coverage: every showcase route owned, gated or proven, and a route call that costs microseconds (design, revision 1)

Status: design only. Base `java-mods` at 8bd103c9b (a merge with origin/main was being finished in the working tree while this was written; nothing here depends on it). Binding: RULES.md, COVERAGE-SPEC §1 and W12, `../w12/DESIGN.md` (mechanism, decisions U1–U7), `../entities/DESIGN.md` §2–§3 and `../entities/SPEC-2-8.md` (W9 phases), `../w5/DESIGN.md` D3/D11 and `../w5/SPEC-7.md` §1, `../w10/DESIGN.md` §4.

Files here: `DESIGN.md`, `SPEC-1.md` … `SPEC-22.md`, `owners.py` (the ownership rules) and `owners.tsv` (its output: one owner per key). SPEC numbers 1–22 are this folder's; W12's own specs are written `W12 SPEC-n`, the entities workstream's `W9 n-m`.

## 0. The bar and what it needs from W12

The finish bar: Mekanism, AE2, Refined Storage, Jade, Building Gadgets 2 and the top-50 library set (`agents/w12/install`, 43 jars, 47 mods; `play/install-rs`, 11 jars) play end to end on single player and on a dedicated server; mod events match NeoForge (M1); **no native-owned mixin target is left unrouted (M2)**.

From W12 that means:
1. Every route of both installs has exactly one owner, and that owner either gates it, proves Java already runs it at vanilla's moment, or proves it has no effect on anything native owns. A route that is only "deferred" doesn't meet M2.
2. A gated route is cheap enough to stay on in a 15-player server. Today it is not (§1.4).
3. Routes that belong to another workstream's mechanism (screens, worldgen, entity rendering, fog, sounds) are handed to it with the key list, and counted by M2 only when that workstream lands them.

## 1. The gap, with evidence

### 1.1 Showcase routes (`finish/report-showcase.txt`, `finish/rep-showcase/routing.json`, 11-06)
177 routes: 4 gated (Carry On's inventory, W12 SPEC-5), 2 UNGATED, 171 deferred: client 62, worldgen 40, view 34 (entity 21, connection 5, block_entity 4, scoreboard 3, save_stream 1), seam 37.

The 2 UNGATED are being closed by a staged change in the working tree (11-06): `BaseContainerBlockEntity#sendChestLockedNotifications` has a `route!` in `G/lan_actions.rs:1357` with `ComponentArg`/`PlayerArg` in `G/java_routing.rs`, and `KeyMapping#setAll` has a `special: screens` row in `deferred.tsv`. This design assumes that change lands; SPEC-1 re-files `KeyMapping#setAll` under its real owner.

### 1.2 The play pack's 20 routes (`play/game31.log` L177–L198, `finish/bugs/run-rs/ctl.log` L165–L186)
`Horizon: routes: 20 on, 0 off, 0 changes dropped`, every one `UNGATED` in that boot. They are the finish bar's core:
- server: `MinecraftServer#createLevels` (ae2 TAIL), `MobEffectInstance#update` (jade wrap_method), `LivingEntity#addEffect` (jade HEAD), `ItemEntity#hurtServer` and `#tick` (ae2: explosion and in-water transforms, **AE2's fluix crystals depend on it**), `AbstractContainerMenu#doClick` (refinedstorage HEAD), `EnchantmentHelper#getItemEnchantmentLevel` (ae2 RETURN, static, hot), `StructureTemplate#fillFromWorld` (ae2, its spatial IO saving);
- client: `KeyMapping#click`/`setAll`, `Font$PreparedTextBuilder` ×2, `GuiGraphicsExtractor#item` (ae2, RS), tooltips ×3, `BossHealthOverlay`, `FogRenderer#setupFog`, `LanguageManager#onResourceManagerReload` (jade).

### 1.3 The 74 undecided methods (`finish/rep-showcase/replaces.json`, `routing: undecided`)
Ledger statuses `none`/`trivial`/`mentioned`/`unknown`, so the analysis couldn't tell whether native runs them. `port_ledger.py where` (11-06) shows native does run most of the gameplay ones: `SimpleContainer.setChanged` (`horizon-world/src/simple_container.rs`, `mobs/villager.rs`), `Entity.isInvulnerable`/`onBelowWorld`/`onPassengerTurned` (`motion.rs`, `mobs/living.rs`), `ExperienceOrb.playerTouch` (`game.rs` `tick_experience`, `lan_actions.rs` `guest_orbs`), `ItemEntity.ignoreExplosion`/`setItem` (`entities.rs`, `explosion.rs`), `Player.getFlyingSpeed` (`motion.rs` `MotionWorld::flying_speed`), `ItemStack.finishUsingItem` (`game.rs` `tick_used_item`), `ItemStack.overrideOtherStackedOnMe` (`item_data.rs`), `CropBlock`/`StemBlock.mayPlaceOn` (`level/bonemeal.rs`, `level/tick.rs`), `Scoreboard.onTeamAdded`/`onTeamRemoved`/`removePlayerFromTeam` (`scoreboard.rs`, `remote.rs`), `ChunkMap.anyPlayerCloseEnoughForSpawning` (`creatures.rs`), `ServerConfigurationPacketListenerImpl.startConfiguration` (`lan/conn.rs`). `Entity.syncData` and `AnvilMenu.createResultInternal` are not in the vanilla jar (NeoForge's names). 31 are worldgen, codec or gametest classes.

### 1.4 What a route call costs today (`agents/w12s3/scratch/1/java-ticks.log`, the `bridges` session with SPEC-3's `setSelectedSlot` route, `HORIZON_PROFILE`, µs: count|total|max)

| Window | `route` op (Rust, whole call) | `route:…setSelectedSlot` (Java, merged method incl. nested body) | `refresh` (nested player fill) | `player_tick` (a `use` fill per player per tick) |
|---|---|---|---|---|
| L2 | 3 / 1412 / 698 → 470 mean | 3 / 800 → 267 | 2 / 142 → 71 | 21 / 9576 → 456 |
| L3 | 1 / 268 | 1 / 113 | 1 / 49 | 20 / 6607 → 330 |
| L11 | 1 / 213 | 1 / 101 | 1 / 27 | 21 / 4125 → 196 |
| L13 | 1 / 248 | 1 / 148 | 1 / 71 | 20 / 2898 → 145 |

The route's own work is a slot assignment. Everything else is the player view: `java_use_request` (`G/java_mods.rs:354`) encodes the whole player every call (41 stacks through `item_to_nbt` + `write_tag`, effects with a `format!` each), and `GameBridge.fill` (`B/GameBridge.java:390`) decodes it all (`inventory.clearContent()`, then each stack through `ItemStack.CODEC`), runs `PlayerSync.fill` (rebuilds the effect map), `absSnapTo`, mode and abilities. The nested body bridge then calls `java_refresh_player`, which encodes everything again. So a route on a player costs two full player fills; `player_tick` pays one per player per tick, about 150–450 µs, which is 2–7 ms a tick at 15 players before any mod code runs.

### 1.5 What is already designed elsewhere (not repeated here)
- **W9 3-1** adds `Recv::Entity` and per-type admission; **W9 3-2** gates ae2 `ItemEntity#tick`, `ItemEntity#hurtServer` and Jade `LivingEntity#addEffect`. Both need W9 phase 2 (one turn function per entity kind).
- **W9 4-2** makes crossings from inside a mob's turn possible (shared mode, deferred writes): the only way to call Java from code that holds `MobWorld`/`MotionWorld` borrows.
- **W5 SPEC-7** routes vanilla screen methods per method (GUI bridges, `gui_route!`); W5 D11 keeps item drawing native with edge routes recorded over it.
- **W10 §4** routes worldgen by seams and needs from W12 only the patched-method fact (`Host::patched`, `Routing.patched`).

## 2. Approach

### 2.1 One owner per key (SPEC-1)
`owners.py` gives each of the 177 routes and 74 undecided methods one owner (`owners.tsv`):

| Owner | Routes | Undecided | What it means |
|---|---|---|---|
| gated / in flight | 5 | | done |
| W9 3-2 | 2 | | entities workstream (`ItemEntity#hurtServer`, `LivingEntity#addEffect`) |
| **this folder, SPEC-6 … SPEC-22** | 68 | 24 | gated here, by vanilla class or system (`ItemEntity#tick` is shared: W9 3-2 gates it for ae2, SPEC-9 adds travelersbackpack's HEAD) |
| `java-client` | 20 | 3 | the Java client runs the real method at vanilla's moment; SPEC-1 proves each with a method trace, failures move to SPEC-22 |
| `java-boot` | | 3 | runs in Java's boot (class init, `WorldLoader`), whose results reach native as boot facts |
| `java-already` | | 1 | NeoForge's own method; native has no port and never calls it |
| `inert` | | 5 | gametest and datagen code; Horizon has neither |
| W5 | 28 | 11 | screens, tooltips, HUD, text |
| W10 | 40 | 21 | worldgen, codec `<clinit>`s |
| W1 / W2 / W4 / W6 / W7 / W8 / W13 | 0 / 3 / 1 / 1 / 4 / 3 / 2 | 2 / 0 / 1 / 0 / 0 / 0 / 2 | outline and vanilla BER, models, fog and the local player's motion (`Player#getFlyingSpeed` is read by the window's client-side movement, W4's `LocalPlayer` path), suggestions, environment attributes, sounds, commands and the configuration phase (`ServerConfigurationPacketListenerImpl#startConfiguration`/`handleConfigurationFinished`: W13's configuration tasks) |
| out of scope | | 1 | `DimensionStorageFileFix#makeFixer` (upgrading pre-26.3 worlds) |

`deferred.tsv` keeps a row only for a key whose owner hasn't landed it, and its reason names the owner (`owner: W5`, `owner: W10`, `owner: W9 3-2`, `spec: w12-coverage SPEC-n`). The lint fails a row whose owner says done.

### 2.2 The cost, fixed at the root: the player view moves by groups (SPEC-2)
The lead's direction was "refresh only the groups a route reads". A static read set of mod handlers is not provable (they call arbitrary mod code), so this design sends **only the groups that differ from what the view already holds**, which is the same set for every route that changes nothing and is exact for every route that does:
- Rust keeps, per acting player, what it last gave Java (`SENT`, thread-local, as `java_entities.rs`'s `VIEWED`): pose, mode, selected slot, experience, vitals, effects, motion, and the 41 stacks.
- A request carries a group mask and only the changed groups; stacks go per slot. An unchanged player is a header of about 80 bytes.
- Java keeps per player the stacks it last synced (`PlayerSync.SYNCED`) instead of clearing the inventory at every fill. Stacks keep their identity across calls, as in vanilla (mods caching a stack object work; today every fill makes new ones).
- The nested `refresh` after a native body becomes the same delta: it carries only what the body changed (the selected slot for `setSelectedSlot`).
- Before a route stub calls a native body, Java's uncommitted stack changes are flushed to native (only when there are any), so the native body reads what the Java handlers wrote, in vanilla's order.
- Every Rust site where Java drops or rebuilds a player view resets `SENT`; Java refuses a delta request on a fresh view with a loud error. No fallback.

This is vanilla's own pattern (`AbstractContainerMenu.broadcastChanges` compares against `remoteSlots`). It fixes `player_tick` and every `use`-family op at the same time.

### 2.3 Admission narrowed by provable handler guards (SPEC-4)
W12 admits receivers by class chain only. Several handlers start with a guard on what they act on: Travelers Backpack's `Entity` handlers call `travelersbackpack$isBackpack()` (javap, `mixin/entity/item/EntityMixin`), AE2's `EnchantmentHelperMixin.hookGetItemEnchantmentLevel(Holder, ItemInstance, cir)` acts on AE2's tools. Without narrowing, `Entity#isInWater` and `Entity#tick` cross for every entity many times a tick, and `getItemEnchantmentLevel` for every enchantment lookup.

SPEC-4 reuses the entities analysis's guard walker (`tools/mod_entity_facts.py` `walk_guards`, entities SPEC-0 §3) on route handlers, with two more value shapes (the receiver `this`, an `ItemStack`/`ItemInstance` argument, and `getItem().getItem()` on either) and one level of private boolean helpers. Boot turns class guards into admitted entity types and admitted item ids (`routes.bin` format 2). A route whose admitted set holds nothing native owns is **inert by proof** and reported so; that is how M2 accepts it.

### 2.4 Gates in library code (SPEC-5)
Some routed methods have their only port inside a lib crate (`horizon-items` enchantment levels, `horizon-core` shape joins, `motion.rs` water and flying speed, `effects.rs` effect merging, mob brain statics). W12 §2.5 forbids a `route!` there. SPEC-5 adds a lib gate (`horizon_core::lib_gate`): a `static` with the same atomic states as `Gate`, plus one process-wide hook the game installs; a lib fn asks it first. The hook crosses through **W9 4-2's shared crossing**, the only crossing allowed while lib code holds world borrows. Without java-mods the hook is never installed: one relaxed load.

### 2.5 Values and receivers routes need (SPEC-3)
Route arguments today are primitives, `BlockPos`, `String`, `ItemStack`, `Component` and the acting `Player`. The batches need: `Vec3`, `DamageSource` (W9 1c2a's `Source`), `MobEffectInstance`, entity references (W9 3-1), resource keys and holders by name, `TeleportTransition` (existing `Flat.raw`), `ValueInput`/`ValueOutput` (save streams: a tag in; Java's written keys merged with the native body's), and returns `Vec3`, `ItemStack`, `Optional`. Receivers: `Recv::Server` (the `MinecraftServer` view), `Recv::Scoreboard`, `Recv::Connection` (the acting player's `ServerGamePacketListenerImpl`), `Recv::Menu` (the acting player's open menu, SPEC-16), `Recv::BlockEntity(pos)` (SPEC-16). Java reads every argument through the bridge `Flat.to<Type>` table instead of the hand-written chain in `Routing.run`, so a new type is one method per side, as for bridges.

### 2.6 Batches, by vanilla class or system

| Spec | Batch | Keys (mods) |
|---|---|---|
| 6 | Scoreboard and teams | `ServerScoreboard#addPlayerToTeam`/`removePlayerFromTeam` (waystones), `Scoreboard#addPlayerToTeam`/`removePlayerFromTeam`/`onTeamAdded`/`onTeamRemoved` (lootr) |
| 7 | Entity save streams | `Entity#saveWithoutId` (waystones), `Entity#load` (balm, waystones), `ExperienceOrb#add/readAdditionalSaveData` (clumps), `Player#readAdditionalSaveData` (carryon) |
| 8 | Experience orbs | `ExperienceOrb#canMerge` ×2, `merge`, `tryMergeToExisting`, `repairPlayerItems`, `playerTouch` (clumps) |
| 9 | Guarded entity hooks | `Entity#tick` (waystones, mekanismadditions), `isInWater`, `isInvulnerable`, `onBelowWorld`; `ItemEntity#tick` HEAD, `fireImmune`, `ignoreExplosion`, `setItem` (travelersbackpack) |
| 10 | Living damage and effects | `LivingEntity#causeFallDamage`, `hurtServer` (balm), `MobEffectInstance#update`, `onEffectAdded` (jade) |
| 11 | Riding | `Entity#positionRider`, `getDismountLocationForPassenger`, `startRiding`, `onPassengerTurned` (carryon) |
| 12 | Other entity ticks | `LightningBolt#tick` (architectury), `FallingBlockEntity#tick` (architectury, lootr), `Allay#aiStep`, `shouldStopDancing` (sophisticatedcore) |
| 13 | Use paths | `ItemStack#use` (glitchcore, architectury), `finishUsingItem` (glitchcore), `BucketItem#use`, `BlockTransformer#transformBlock` ×2, `BlockStateBase#useWithoutItem` (architectury); `getDestroyProgress` (balm) proven served |
| 14 | Pure statics in lib code | `EnchantmentHelper#getItemEnchantmentLevel` (ae2), `Shapes#joinIsNotEmpty`/`joinUnoptimized` (creativecore), `VillagerType#byBiome` (lithostitched), `PoiTypes#forState`/`hasPoi`, `PoiType#is`, `CatSitOnBlockGoal#isValidTarget` (lootr), `CropBlock`/`StemBlock#mayPlaceOn` (balm), `ChunkMap#anyPlayerCloseEnoughForSpawning` (ae2), `NetherPortalBlock#getExitPortal` (netherportalfix) |
| 15 | Player packets | `ServerGamePacketListenerImpl#handlePickItemFromBlock`/`FromEntity` (architectury), `tryPickItem` (travelersbackpack), `handleUseItemOn` (lootr) |
| 16 | Vanilla menus on the server's Java side | the receiver family 17 needs (vanilla block entities already live in Java: `BlockEntities`) |
| 17 | Menus and hoppers | `AbstractContainerMenu#doClick` (refinedstorage), `getRedstoneSignalFromContainer` (lootr), `AnvilMenu#createResultInternal` (ae2), `ItemStack#overrideOtherStackedOnMe` (travelersbackpack), `HopperBlockEntity#getBlockContainer`/`getEntityContainer`/`getContainerOrHandlerAt` (lootr), `SimpleContainer#setChanged` |
| 18 | Lazy loot (a vanilla fix, W12 U5) | native keeps table and seed until a container opens |
| 19 | Lootr's loot | `RandomizableContainer#setLootTable`/`tryLoadLootTable`/`unpackLootTable`, `LootTable#fill`, `BrushableBlockEntity#tryLoadLootTable`, `ChestBlock#isBlockedChestByBlock`, `BrushItem#onUseTick` |
| 20 | Server start and reload | `MinecraftServer#createLevels` (ae2), `reloadResources`, `ReloadableServerResources#loadResources` (balm), `Eula#readFile`, `DedicatedServerSettings#<init>` (puzzleslib), `DedicatedServer`/`IntegratedServer#initServer` (lithostitched) |
| 21 | Player join and travel | `PlayerList#placeNewPlayer` (creativecore), `ServerLevel#addPlayer`, `ServerPlayer#teleport` (glitchcore) |
| 22 | Window gates (W12 phase C) | `Minecraft#handleKeybinds` (architectury, carryon), `continueAttack`, `run`, `runTick`, `resizeGui`, `KeyboardHandler#handleDebugKeys`, `ClientLevel#syncBlockState`, plus any `java-client` key SPEC-1's trace disproves |

## 3. Decisions (with reasons)

- **C1. Delta groups, not static read sets.** Mod handlers call arbitrary code; a static read set is unsound, and a wrong one is a silent stale read. Sending what changed is exact for any handler and gives the same win, since a route that changes nothing sends nothing.
- **C2. Out-of-call writes to a player view keep today's outcome.** Java code that changes a player's stacks outside a filled call (a block entity tick) is lost today, at the next full fill. With deltas, SPEC-2 restores the synced stack at the next fill and counts it (`player view drift` in the profile), so the outcome is unchanged and the case is visible. Making those writes reach native is a write-through bridge on `Inventory`, a separate item (§5).
- **C3. Inert by proof only.** A route is inert when its guards admit nothing native owns (SPEC-4), or when its class runs only in code Horizon doesn't have (gametest, datagen). "Probably harmless" is never a verdict.
- **C4. `java-client` needs a trace.** A client key is `java-client` only after one JFR `jdk.MethodTrace` session shows the method running on the Java client's "Render thread" in the situation vanilla calls it (SPEC-1). Otherwise it is a window gate (SPEC-22).
- **C5. Body routes stay body routes.** A wrap or redirect inside a body still runs vanilla's body in Java (W12 §2.1). A "call-site" route kind that calls one merged handler with a synthetic `Operation` was considered: it would re-implement MixinExtras' chaining, which W12 §0 forbids. The closure (`needs_bridge`) of each body route is closed by bridges in its batch.
- **C6. Long-running methods.** `Minecraft#run` routes only HEAD changes at the window loop's start. A RETURN or TAIL change on a method whose body is the main loop fails the boot with `route <key>: changes after a main loop are not supported` (a scope boundary, loud, not a fallback).
- **C7. Lootr waits for evidence.** SPEC-19 starts with a senior JFR trace on real NeoForge (which merged Lootr handlers fire when a structure chest generates, is opened by player A, then B), as W12 DESIGN §4 T3 requires. Its specs' expected values come from that trace.
- **C8. Entity-family routes come after W9 phase 2 and 3-1.** Each gate is the first statement of the turn or port function W9 makes; this folder adds no entity loop changes of its own.

## 4. Specs and order

| # | Title | Lines | Needs |
|---|---|---|---|
| 1 | Owners, verdicts, the `java-client` trace, lint vocabulary | ~300 | — |
| 2 | Player view by groups (the cost root fix) | ~450 | — |
| 3 | Route values and receivers | ~400 | 2 |
| 4 | Admission from handler guards; inert by proof | ~400 | W9 3-1 for the entity half |
| 5 | Gates in library code | ~300 | W9 4-2 |
| 6 | Scoreboard and teams | ~300 | 3 |
| 7 | Entity save streams | ~350 | 3, W9 3-1 |
| 8 | Experience orbs; field writes from Java bodies | ~400 | 3, 7, W9 2-9, 3-1 |
| 9 | Guarded entity hooks | ~400 | 4, 5, W9 2-9/2-10, 3-1, 3-2 |
| 10 | Living damage and effects | ~400 | 3, 5, 8 (field bridges), W9 3-2, 4-3, 4-4, 4-5 |
| 11 | Riding | ~300 | 3, W9 2-13, 2-14, 3-1 |
| 12 | Other entity ticks | ~350 | 4, 8 (field bridges), W9 2-6/2-8, 2-10, 3-1 |
| 13 | Use paths | ~400 | 2, 3, 4 |
| 14 | Pure statics in lib code | ~400 | 3, 4, 5 |
| 15 | Player packets | ~350 | 2, 3 |
| 16 | Vanilla menus on the server's Java side | ~450 | 2, 3 |
| 17 | Menus and hoppers | ~400 | 4, 5, 16 |
| 18 | Lazy loot | ~450 | — |
| 19 | Lootr's loot | ~400 | 3, 16, 18, senior trace (its step 0) |
| 20 | Server start and reload | ~450 | 3, 7 (Java-bodied stubs), W10 (mod dimensions, for AE2's spatial level only) |
| 21 | Player join and travel | ~400 | 2, 3, W9 SPEC-1d |
| 22 | Window gates | ~450 | 1, 3 |

Landing order: 1 and 2 first (independent), then 3, then 4. After those, the batches in any order their outside dependencies allow; 18 can land any time. Priority for the finish bar: 2 (cost), then the play pack's server routes (W9 3-2, 17's `doClick`, 14's `getItemEnchantmentLevel`, 20's `createLevels`, 10's `MobEffectInstance#update`), then the showcase batches; 22 (window) after SPEC-1's trace.

## 5. Out of scope
- `DimensionStorageFileFix#makeFixer` (waystones): upgrading pre-26.3 worlds; the bar plays fresh worlds.
- Writes to a player's stacks from Java code outside a filled call (C2): a write-through `Inventory` bridge family, W9 or W14.
- Client keys owned by W1, W2, W4, W5, W6, W7, W8, and worldgen keys owned by W10: listed in `owners.tsv`, landed by those workstreams.
- `StructureTemplate#fillFromWorld` (ae2, its spatial storage capture) and `ChunkMap#<init>`/`scheduleChunkLoad`: W10's seams; AE2 spatial IO also needs W10's mod dimensions.
- Packs beyond the two installs.

## 6. Questions for the user
- **Q1.** `SimpleContainer#setChanged` (puzzleslib, refinedstorage, shulkerboxtooltip, sophisticatedbackpacks, toms_storage): if SPEC-4's guards don't make it inert, routing it means Java views of every native `SimpleContainer` (villager, allay, horse inventories) for an effect nobody has observed. Recommended: route it anyway (SPEC-17), as M2 demands; the alternative is a recorded exception.
- **Q2.** `Shapes#joinIsNotEmpty`/`joinUnoptimized` (creativecore, body, static, called from collision): if its guards don't narrow it, every shape join crosses (thousands a tick). Recommended: accept the cost only if SPEC-14's measurement stays under 1 ms a tick at the 360-mob scenario; otherwise come back with the trace.
- **Q3.** `DimensionStorageFileFix#makeFixer`: confirm out of scope.
