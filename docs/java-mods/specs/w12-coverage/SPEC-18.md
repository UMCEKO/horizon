# SPEC-18: lazy loot, as vanilla (W12 decision U5; a vanilla fix, no Java)

Design: `DESIGN.md` §2.6 row 18; W12 `DESIGN.md` §4 T3 and U5. Needs nothing. Size: ~450 changed lines (Rust ~300, harness ~150). Works with or without `java-mods`.

Paths: `G/` = `crates/horizon-client/src/game/`, `L/` = `crates/horizon-client/src/`, `W/` = `crates/horizon-world/src/`. Read first: vanilla `RandomizableContainer.java` (`unpackLootTable`, `tryLoadLootTable`, `trySaveLootTable`, `setLootTable`), `RandomizableContainerBlockEntity.java` (every item accessor calls `unpackLootTable(null)` first), `BaseContainerBlockEntity`, `ChestBlock.getMenuProvider` (double chests), `Containers.dropContents`, `AbstractMinecartContainer` (`unpackChestVehicleLootTable` via `ContainerEntity`), `BrushableBlockEntity.unpackLootTable`, `DecoratedPotBlockEntity`, `StructurePiece.createChest`, `RandomizableContainer.setBlockEntityLootTable`; native: `G/commands.rs` `unpack_opened_loot` (~2817), `G/lan_actions.rs` ~1400 (its caller at open), `L/save.rs` (`rolled_loot`, `consume_container_loot`), `W/loot.rs` (~712, `LootTable#fill`'s port), the worldgen code that fills structure chests (grep `rolled_loot` and `createChest` under `W/worldgen/structure/`), `G/minecarts.rs`, `G/decorated_pot.rs`, `G/brush.rs`, `horizon-client-core/src/block_entity_nbt.rs`.

Comments in code: one line at most.

## Why
Native rolls a structure container's loot when the structure generates and keeps the roll (`rolled_loot`) beside the table until the container is opened. Vanilla keeps only `LootTable` and `LootTableSeed` and rolls in `unpackLootTable(player)` at the first access, with the opening player in the loot context (luck, `THIS_ENTITY`, `ORIGIN`), or with no player for a hopper, a comparator, a dropper or a break. Results differ whenever the context matters (Luck, the player's attributes, mods' loot conditions and modifiers), `/data get` on an unopened chest shows items instead of the table, and Lootr (SPEC-19) has nothing to hook: its whole job is to roll per player in `unpackLootTable`.

## 1. What native keeps
- A container block entity with a pending table holds `loot_table: Option<(String, i64)>` (table id, seed) and no rolled items. Add the field where native's container block entities hold items (or its saved tag, if that is how containers are stored: follow the existing `LootTableSeed` reads in `block_entity_nbt.rs`). Same for chest boats and minecarts (`G/minecarts.rs`) and decorated pots.
- Structure generation stores the table and seed (`StructurePiece.createChest` → `RandomizableContainer.setBlockEntityLootTable(level, random, pos, table)`: the seed is `random.nextLong()` at that moment, so the generation's random draws stay in vanilla's order). Remove the generation-time roll and `rolled_loot`.
- Saves write `LootTable`/`LootTableSeed` while pending (vanilla `trySaveLootTable`), items otherwise. Loading reads them back (`tryLoadLootTable`). An old save with `rolled_loot` entries: drop the rolled items and keep the table (the roll was never visible to a player, by construction).

## 2. One unpack, at every access vanilla unpacks at
`fn unpack_loot_table(&mut self, at: ContainerRef, player: Option<PlayerRef>)` in `G/loot_unpack.rs` (new):
- does nothing without a pending table;
- builds the `LootParams` as vanilla (`ORIGIN` = container center, and with a player: `withLuck(player luck)`, `THIS_ENTITY` = the player), takes the table, clears the pending field **before** filling (vanilla's order: `setLootTable(null)` then `fill`), and runs `W/loot.rs`'s `fill(container, params, seed)` (the `LootTable.fill` port: shuffle and split into slots with the seeded random);
- triggers `player_generates_container_loot` for a player (moved from `unpack_opened_loot`);
- marks the chunk modified.
Callers (each is a vanilla `unpackLootTable` site):
- a player opening the container (today's `unpack_opened_loot` caller; both halves of a double chest, as `ChestBlock.getMenuProvider`);
- every native read or write of a container's items: hopper push and pull, droppers and dispensers, comparators (`getRedstoneSignalFromContainer`), `/item`, `/data get` of `Items` (vanilla's `/data get` saves the block entity with `saveWithoutMetadata`, which writes the table while pending: check, and follow vanilla), `/loot … insert`, breaking (drops: `Containers.dropContents` → `unpackLootTable(null)`), structure blocks saving the area;
- chest minecarts and boats: on open, on hopper minecart access, on break;
- brushing suspicious sand and gravel: `BrushableBlockEntity.unpackLootTable(player)` at the brush's completion (`G/brush.rs`), and decorated pots on break (`G/decorated_pot.rs` `unpack_loot` becomes a caller of this fn).
Find the item accessors with a grep for container item reads in `G/` and `W/level/` (`items`, `slots` on chest-like block entities); each must go through one accessor that unpacks first, mirroring `RandomizableContainerBlockEntity.getItem/removeItem/setItem/getItems`.

## 3. Tests
- **Parity harness `loot_unpack`** (new; copy a nearby harness: `tools/test_<name>_parity.py`, a Java oracle `mods/horizon/src/test/java/dev/horizonlab/parity/VanillaLootUnpackOracle.java`, a Rust probe `crates/horizon-client/examples/loot_unpack_probe.rs` or the game with a fixture env var as `game/command_harness.rs` does). Cases, each comparing the container's items after the action, oracle against probe:
  1. a chest placed with `{LootTable:"minecraft:chests/simple_dungeon",LootTableSeed:12345L}`, opened by a player with luck 0;
  2. the same with luck 3 (an attribute modifier);
  3. the same pulled first by a hopper (no player);
  4. the same broken (the dropped item entities' stacks);
  5. a double chest, both halves with tables, opened from the left half;
  6. `/data get block` on an unopened chest (the table shows, no items);
  7. a chest minecart with a table, opened.
- Existing harnesses that look at structure chests (`parity.py --changed-files <files> --list` names them: worldgen structures, `save_roundtrip`, container commands): their outputs move toward vanilla (unopened chests hold tables). Report each moved harness and why the oracle agrees (entities SPEC rules for behaviour-changing steps).
- Rust unit tests in `G/loot_unpack.rs`: `unpack_clears_table_before_fill` (a fill that reads the container sees no table); `second_unpack_is_a_noop`.
- `cargo test` of the touched modules; clippy with and without `java-mods`; `python3 tools/pr_ready.py --quick`.

## Done bar
`loot_unpack` passes all seven cases; every harness that moved is listed with the oracle's agreement; no `rolled_loot` left in the code.

## For the lead and the user
- Lead: `python3 /home/umceko/hz-poc/verify/parity.sh loot_unpack` and the moved harnesses; `lan.sh chest`.
- User (vanilla, no mods): find a dungeon or a village chest, open it: loot as before. Break an unopened shipwreck chest: it drops loot. `/data get block` on an unopened desert temple chest shows `LootTable` (as vanilla).
