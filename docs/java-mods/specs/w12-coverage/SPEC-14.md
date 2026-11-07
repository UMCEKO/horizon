# SPEC-14: pure statics and small queries, mostly in lib code (AE2, Creative Core, Lithostitched, Lootr, Balm, Nether Portal Fix)

Design: `DESIGN.md` §2.4, §2.6 row 14, question Q2. Needs SPEC-4 (item, block and argument-class guards), SPEC-5 (lib gates). Size: ~400 changed lines.

Paths as SPEC-2; `L/` = `crates/horizon-client/src/`. Read first, for each key, its vanilla method and the native port named below; SPEC-4's guard output for each key (`routing.json` `guards`).

Comments in code: one line at most.

## Keys and seams
| Key | Kind | Mod | Port (seam) | Gate | Receiver, args, admission |
|---|---|---|---|---|---|
| `EnchantmentHelper#getItemEnchantmentLevel(Lnet/minecraft/core/Holder;Lnet/minecraft/world/item/ItemInstance;)I` | edge RETURN, static, hot | ae2 | `crates/horizon-items/src/item_data.rs` ~274 | `lib_route!` | `Static`; `HolderArg` (enchantment id), stack bytes; `item =` the stack's item (expected: AE2's tools only) |
| `Shapes#joinIsNotEmpty(…VoxelShape;…VoxelShape;…BooleanOp;)Z`, `Shapes#joinUnoptimized(…)…VoxelShape;` | body, static | creativecore | `crates/horizon-core/src/geometry/shape/join.rs` ~41, ~103 | `lib_route!` if not inert | expected inert by `arg_class` (SPEC-4); else §2 |
| `VillagerType#byBiome(Lnet/minecraft/core/Holder;)Lnet/minecraft/resources/ResourceKey;` | edge RETURN (modify_return_value), static | lithostitched | `L/mobs/finalize.rs` ~2187 | `lib_route!` | `Static`; `HolderArg` (biome id); returns `text` key |
| `PoiTypes#forState(Lnet/minecraft/world/level/block/state/BlockState;)Ljava/util/Optional;` | edge RETURN, static | lootr | `L/mobs/villager_brain.rs` ~827 and the POI section code (`crates/horizon-client-render/src/poi_sections.rs`) | `lib_route!` | `Static`; state; `block =` the state (expected: Lootr's chests) |
| `PoiTypes#hasPoi(…BlockState;)Z` | inject, static | lootr | same files | `lib_route!` | same |
| `PoiType#is(…BlockState;)Z` | inject | lootr | `poi_sections.rs` | `lib_route!` | `Value` (the POI type's registry id, Java `Flat.toHolder(…).value()`); state; `block =` |
| `CatSitOnBlockGoal#isValidTarget(Lnet/minecraft/world/level/LevelReader;Lnet/minecraft/core/BlockPos;)Z` | edge HEAD | lootr | `L/mobs/tame.rs` ~1329 | `lib_route!` | `Entity(cat)` projected to its goal (§1); `LevelArg`, pos; `block =` the state at pos (expected `block_at`: Lootr's chests) |
| `CropBlock#mayPlaceOn(…BlockState;…BlockGetter;…BlockPos;)Z`, `StemBlock#mayPlaceOn(…)` | inject | balm | `crates/horizon-world/src/level/{bonemeal.rs,tick.rs}` (survival checks) | `lib_route!` | `Block(crop state)`; ground state, `LevelArg`, pos; `block =` per SPEC-4 |
| `ChunkMap#anyPlayerCloseEnoughForSpawning(Lnet/minecraft/world/level/ChunkPos;)Z` | inject | ae2 | every native caller: natural spawning's per-chunk check and `G/creatures.rs` `portal_spawn` | `route!` in each `Game` caller's helper | `Level` projected to `level.getChunkSource().chunkMap` (owner `ChunkMap`; add to SPEC-3's projection table); `long` chunk pos as `ChunkPos.toLong` |
| `NetherPortalBlock#getExitPortal(…ServerLevel;…Entity;…BlockPos;…BlockPos;Z…WorldBorder;)…TeleportTransition;` | body (`modify_expression_value` on `PortalForcer.findClosestPortalPosition`) | netherportalfix | `G/nether_portal.rs` | `route!` | `Block(portal state)`; `LevelArg` (destination), `EntityArg`, two `BlockPos`, `bool`, `WorldBorderArg` (§1); returns `TeleportTransition` bytes or none |

## 1. New pieces
- **Goal receivers.** `CatSitOnBlockGoal` is an instance method of a goal. Java has the cat's view, not its goals (vanilla goals are native, W9 7-1). Receiver projection for owner `CatSitOnBlockGoal`: a goal built once per cat view with vanilla's constructor arguments from `Cat.registerGoals` (`new CatSitOnBlockGoal(cat, 0.8)`, check the speed), cached on the view (a `WeakHashMap<Cat, CatSitOnBlockGoal>` in `Routing`). The goal's own fields aren't read by `isValidTarget`'s vanilla body (check); if they are, report it.
- **`WorldBorderArg`**: `text dim`; Java passes `GameBridge.level(dim).getWorldBorder()` (the destination level's border, which is what vanilla passes). Add to SPEC-3's table.
- **`TeleportTransition` return**: Java writes `Flat.raw(transition)` (exists, `Flat.java:69`); Rust reads it as `java_teleport` does (`G/java_entities.rs` ~1366) into native's exit.
- **`getExitPortal`'s body** calls `PortalForcer.findClosestPortalPosition` (wrapped by Nether Portal Fix) and `createPortal`: bridge both on a backed `PortalForcer` (the level view's) to native's portal search and creation in `G/nether_portal.rs`. Nether Portal Fix keeps its portal links in its own saved data (Java-owned): nothing native to add.

## 2. Hot routes: measure, never skip
- `getItemEnchantmentLevel` is called many times a tick (mining speed, damage, armor, looting, fortune…). With SPEC-4's item list only AE2's tools cross. Report the guard SPEC-4 found; if it's `all`, every enchantment lookup crosses: measure (below) before landing and bring the numbers to the lead.
- `Shapes` joins are called from collision thousands of times a tick. Expected inert by `arg_class`. If SPEC-4 can't prove it, stop and report (DESIGN Q2); don't gate it.
- `anyPlayerCloseEnoughForSpawning`: once per ticking chunk per tick in natural spawning. Measure with the 360-mob scenario.
- Cost bar: each hot key's `route:` total under 1 ms a tick in W9's 360-mob scenario and a mining session (horizon-ctl `attack` on stone for 200 ticks with a Fortune pickaxe). Over the bar: report, don't narrow by guess.

## 3. Tests
- Test mod mixins (`J/testmod/…/mixin/StaticRouteMixin.java`), with test-mod classes so the guards are provable:
  - `EnchantmentHelper#getItemEnchantmentLevel` RETURN: `if (!(stack.getItem() instanceof TestToolItem)) return;` then `cir.setReturnValue(cir.getReturnValue() + 3)` for `minecraft:efficiency`;
  - `VillagerType#byBiome` `@ModifyReturnValue`: `minecraft:plains` → `minecraft:snow`;
  - `PoiTypes#forState` RETURN: for `TestChestBlock` states, `Optional.empty()`;
  - `CatSitOnBlockGoal#isValidTarget` HEAD: `if (!(level.getBlockState(pos).getBlock() instanceof TestChestBlock)) return;` then `false`;
  - `NetherPortalBlock#getExitPortal` `@ModifyExpressionValue` on `findClosestPortalPosition`: records the found position.
- `ChecksRouting`/`ChecksEntities`:
  - `efficiency_from_route`: mining stone with a `horizon_java_test:tool` (a `TestToolItem` with Efficiency 0) takes the ticks vanilla's formula gives for Efficiency 3 (compute the expected tick count by hand from vanilla's `getDestroySpeed` with efficiency 3: write it as a constant with its arithmetic); a plain iron pickaxe isn't routed (no `route:` call for it in the profile);
  - `villager_plains_snow`: a villager spawned with `/summon villager` in plains has type `minecraft:snow`;
  - `cat_skips_test_chest`: a tamed sitting-capable cat beside a `TestChestBlock` and no other target never sits on it in 400 ticks;
  - `portal_exit_recorded`: going through a nether portal (the session lights one and teleports the player into it) records a found position, and the player arrives in the nether at native's exit.
- `python3 tools/parity.py --changed-files <files> --list` and what it names (villager types, POI, cat AI, portals, mining speed, collision shapes): without mods nothing moves.
- `cargo test` of the touched modules; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run.

## Done bar
Checks pass; every key gated or proven inert (`inert (guard)` at boot); the cost report for the hot keys; lint passes.

## For the lead and the user
- Lead: the cost report; a boot line `inert (guard)` for the two `Shapes` keys, or DESIGN Q2 raised with numbers.
- User (AE2): an AE2 tool with its intrinsic bonus (e.g. a Fluix pickaxe, check AE2's docs for which have one) mines at the speed it has on NeoForge (time a block of obsidian against NeoForge).
- User (Nether Portal Fix): build two portals in the overworld far apart, go through each: each leads back to its own nether portal, not to the nearest one (the mod's purpose), as on NeoForge.
