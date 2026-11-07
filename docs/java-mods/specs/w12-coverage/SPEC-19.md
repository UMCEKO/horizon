# SPEC-19: Lootr's loot (containers, loot tables, brushing, blocked chests)

Design: `DESIGN.md` §2.6 row 19, decision C7; W12 `DESIGN.md` §4 T3. Needs SPEC-18 (lazy loot: native has an `unpackLootTable` seam), SPEC-16 (block-entity and container arguments), SPEC-3, and **step 0** below, done by a senior before a junior starts. Size: ~400 changed lines.

Paths as SPEC-2. Read first: vanilla `RandomizableContainer.java`, `RandomizableContainerBlockEntity.java`, `LootTable.fill`, `BrushableBlockEntity` (`tryLoadLootTable`, `unpackLootTable`, `brush`), `BrushItem.onUseTick`, `ChestBlock.isBlockedChestByBlock` and its callers; SPEC-18's `G/loot_unpack.rs`; `B/BlockEntities.java` (`get`, `load`: vanilla block entities become Java objects by `loadWithComponents`, which runs `loadAdditional` → `tryLoadLootTable`); `G/storage.rs` (chest opening), `G/brush.rs`.

Comments in code: one line at most.

## Step 0 (senior, clean room: a runtime trace on real NeoForge)
Run a NeoForge 26.3.0.101 dedicated server with Lootr and its libraries from `agents/w12/install/mods` (gate first; `GATE_OWNER=w12-coverage`), JFR `jdk.MethodTrace` on: `RandomizableContainer::setLootTable`, `::tryLoadLootTable`, `::unpackLootTable`, `LootTable::fill`, `BrushableBlockEntity::tryLoadLootTable`, `ChestBlock::isBlockedChestByBlock`, `BrushItem::onUseTick`, and every method of Lootr's mixin classes for these targets (`noble.lootr.mixin.MixinRandomizableContainer`, `MixinLootTable`, `MixinBrushableBlockEntity`, `MixinBrushItem`, `MixinChestBlock`: names from javap of the jar). Scenario with two bot or real clients: a new world; `/locate` and teleport to a village, a desert pyramid (suspicious sand) and a shipwreck; wait for generation; player A opens a chest; player B opens the same chest; A brushes suspicious sand; a hopper is placed under a chest; `/setblock` a chest with `{LootTable:"minecraft:chests/simple_dungeon"}` and open it.

Record, per traced method: thread (worker vs server), the stack's top vanilla frame, and whether a Lootr handler ran. Fill this table in a file next to this spec (`step0.md`) and check it against the plan below; any row that differs changes the plan before a junior starts:

| Method | Expected (this plan) |
|---|---|
| `setLootTable(key, seed)` | worker threads, from structure placement: **W10's** S-place (structures with chests run in Java when this key is patched, W10 §4); never from the server thread in the scenario |
| `tryLoadLootTable` | server thread, from block entity loading (chunk load and `/setblock` with NBT) |
| `unpackLootTable` | server thread, from opening (A and B: Lootr gives each their own inventory) and the hopper |
| `LootTable.fill` | inside `unpackLootTable`, and from Lootr's own code for its per-player inventories |
| `BrushableBlockEntity.tryLoadLootTable` | block entity load |
| `isBlockedChestByBlock` | opening a chest with a block above |
| `BrushItem.onUseTick` | each tick of brushing |

## Keys and plan
| Key | Kind | Plan |
|---|---|---|
| `RandomizableContainer#setLootTable(Lnet/minecraft/resources/ResourceKey;J)V` | body | **owner W10** (only worldgen calls it with a seed; W10's S-place runs vanilla placement in Java). No server-time gate. `deferred.tsv`: `owner: W10` with this spec's step 0 as evidence |
| `RandomizableContainer#tryLoadLootTable(Lnet/minecraft/world/level/storage/ValueInput;)Z` | body | **verdict `java-view`**: Java's block entity objects are made by vanilla's load (`BlockEntities.load` → `loadWithComponents`), which runs it, Lootr's wrap included. Check `lootr_load_seen` below. If step 0 shows Lootr needs it at chunk load time rather than first Java access, make `BlockEntities` load a chunk's loot-table containers when the chunk reaches Java (`java_chunks_changed`), and record that change here |
| `BrushableBlockEntity#tryLoadLootTable(…)Z` | edge RETURN | same verdict and check |
| `RandomizableContainer#unpackLootTable(Lnet/minecraft/world/entity/player/Player;)V` | body (inject at `getLootTable`) | gate in SPEC-18's `unpack_loot_table`: `route!(…, Recv::BlockEntity(pos) or Recv::Entity(minecart id), (PlayerArg or none))`. Java runs vanilla's body on the block entity view: it reads the table and seed (the view's own, loaded from native's saved form), asks the real loot tables (Java's, data), runs Lootr's inject, and calls `LootTable.fill` → items go into the view's container through `setItem`, written through by `BlockEntities.changed` |
| `LootTable#fill(Lnet/minecraft/world/Container;Lnet/minecraft/world/level/storage/loot/LootParams;J)V` | body (inject at `shuffleAndSplitItems`) | reached in Java from the routed `unpackLootTable`; native's own call in `unpack_loot_table`'s body also gets a gate (`route!`, `Recv::Value(table id)`, `ContainerArg(pos)`, `LootParamsArg`, `i64`), for packs that patch `fill` but not `unpackLootTable` |
| `ChestBlock#isBlockedChestByBlock(Lnet/minecraft/world/level/BlockGetter;Lnet/minecraft/core/BlockPos;)Z` | body, static | gate in `G/storage.rs` where native checks a chest's top: `chest_blocked_by_block(pos) -> bool`, `Static`; `LevelArg`, pos |
| `BrushItem#onUseTick(…Level;…LivingEntity;…ItemStack;I)V` | body (expression) | gate in `G/brush.rs`'s per-tick brushing step: `brush_use_tick(remaining)`, `Recv::Value(the brush item)` with `LevelArg`, `PlayerArg`, the stack, `i32`; `item =` the brush |

## New values
- `LootParamsArg`: `text dim`, origin `double[3]`, `float luck`, `int this_entity` (0 none), `int killer` (0 none); Java builds `new LootParams.Builder(level).withParameter(ORIGIN, …)…create(LootContextParamSets.CHEST)` (follow vanilla `unpackLootTable`'s builder exactly).
- `Recv::Value(table id)` for `LootTable`: `Flat` reads the key and resolves the real table from `server.reloadableRegistries()`.

## Bodies and their closures
`check --analysis` lists the bodies' native-owned callees (11-06 report: `unpackLootTable` 10 — `Inventory#add`, `giveExperiencePoints`, `drop`, `Level#getRandom`, `playSound`…; `fill` 1 — `Level#getRandom`; `onUseTick` 19; `isBlockedChestByBlock` none listed). Bridge each to native's port; `Level#getRandom` to native's level random (W9 4-7's bridge).

## Tests
- Test mod mixins (`J/testmod/…/mixin/LootRouteMixin.java`):
  - `unpackLootTable` inject at `INVOKE` `getLootTable`: records the player (null or a name);
  - `LootTable#fill` inject at `shuffleAndSplitItems`: records the number of items;
  - `isBlockedChestByBlock` `@WrapOperation` on its block check: returns false when the block above is `minecraft:stone` (which vanilla treats as blocking);
  - `BrushItem#onUseTick` `@ModifyExpressionValue`: count.
- `ChecksRouting`:
  - `unpack_routed_with_player`: opening a `{LootTable:"minecraft:chests/simple_dungeon",LootTableSeed:12345L}` chest records the player's name, and the items are those SPEC-18's `loot_unpack` case 1 expects (the same seed: Java's fill and native's fill agree);
  - `unpack_routed_without_player`: the hopper case records null;
  - `stone_doesnt_block_chest`: a chest with stone above opens;
  - `brush_route_ticks`: brushing a suspicious sand block counts one call per brushing tick;
  - `lootr_load_seen`: a block entity with a loot table loaded from disk reaches Java through `BlockEntities.load` before any player opens it (only if step 0 required the chunk-time load).
- `python3 /home/umceko/hz-poc/verify/parity.sh loot_unpack` (no route without mods: unchanged); `cargo test` of the touched modules; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run.

## Done bar
Step 0's table is filled and agrees with the plan (or the plan was changed by the senior first); checks pass; gates for `unpackLootTable`, `fill`, `isBlockedChestByBlock`, `onUseTick`; verdict rows for the two `tryLoadLootTable` keys; the `setLootTable` row is `owner: W10`; lint passes.

## For the user (two players, dedicated server, showcase install)
Find a village or dungeon chest (Lootr's chest look: it has Lootr's texture). Player A opens it and takes everything; player B opens it: B sees a full, different-or-same roll of its own (Lootr's per-player loot), as on NeoForge. A hopper under it doesn't drain it. Brush suspicious sand in a desert temple: each player gets their own find. Break-protection behaves as Lootr's config says.
