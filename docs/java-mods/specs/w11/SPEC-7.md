# SPEC-7 (W11): mod loot functions and conditions in the tables Horizon rolls

Read first: `DESIGN.md` G5, D4, D6; `SPEC-1.md` (owned loot kinds), `SPEC-5.md`, `SPEC-6.md` (the context encoding and
`B/Loot.java` it adds); `W/loot.rs` `native_loot_function`, `native_loot_condition`, `NativeLootContext`, `modify` (it tests
an entry's `condition` before `run`), `run`; `I/items.rs` `mod_function`, the condition site (l. ~1716); `L/entry.rs` (the
installed `loot_function`/`loot_condition` dispatch to native mods); `G/java_bridges/effects.rs` `block_get_drops`,
`block_drop_resources`, `block_drop_resources_of`, `drop_resources`; `B/Flat.java` `raw(LootParams.Builder)` →
`B/Effects.java` `lootParams`; vanilla `LootItemFunctions.ROOT_CODEC`, `LootItemCondition.DIRECT_CODEC`,
`LootItemConditionalFunction`, `Block.dropResources`, `Block.getDrops`.

Depends on: SPEC-1, SPEC-5 (and SPEC-6 for the shared context writer; if SPEC-6 hasn't landed, this spec adds the writer).

Comments in code: one line at most, only for a non-obvious why.

## Goal

A loot function or condition whose type a mod registered (SPEC-1's owned kinds), met in a table Horizon rolls, runs as the
mod's real class in Java, on the stack and a `LootContext` built from the roll. When the roll was asked by Java's own
`getDrops` (a mod's block broken through Java), the context is Java's real `LootParams` (the live block entity, the
breaking entity, the tool), not a rebuild.

## Wire

- Op `loot_function`: `text kind`, `text json` (the entry's object without `conditions`/`condition`: native already tested
  them), `bool live`, the context as SPEC-6 encodes it (no stacks, no random), `long seed`, `text stack` (codec JSON).
  Reply: `text stack` (empty = unchanged; Java threw).
- Op `loot_condition`: the same without the stack; reply `byte holds` (0, 1, or 2 = threw).

## Java (`B/Loot.java`)

- `function(in, out)`, `condition(in, out)`; registered as `Crossing.game("loot_function", ...)`,
  `Crossing.game("loot_condition", ...)`.
- Decode with `LootItemFunctions.ROOT_CODEC` / `LootItemCondition.DIRECT_CODEC` and the level's registry ops; cache the
  decoded object by its JSON text (`HashMap<String, Object>`, cleared at server stop).
- Context: when `live` and `LIVE.get() != null`, `new LootContext.Builder(LIVE.get().create(LootContextParamSets.BLOCK))`;
  else build as SPEC-6 step 3. Random: `RandomSource.create(seed)` (D6).
- `function.apply(stack, ctx)`; `condition.test(ctx)`. Throws are logged once per kind:
  `Horizon: loot <function|condition> <kind> threw: <message>` (D4's line when the cause is a bridged native with no game).
- `static final ThreadLocal<LootParams.Builder> LIVE`: `Effects.lootParams(params)` (which `Flat.raw(LootParams.Builder)`
  calls as Java's `getDrops` jumps to native) sets it before flattening.

## Rust

- `G/java_loot.rs`: `pub(crate) fn java_hook_loot_function(kind, data, ctx, stack: &mut LootStack)` and
  `java_hook_loot_condition(kind, data, ctx, holds: &mut bool)`: only for `owns_function(kind)` / `owns_condition(kind)`;
  request inside `bridge::isolated`; `live` = the thread-local `JAVA_LOOT_LIVE` (below); apply the reply
  (`LootStack::from_codec_json`, kept as is when unchanged).
- `L/entry.rs`: the installed `loot_function`/`loot_condition` closures call the native mods' dispatch, then the Java hook
  (`crate::game::java_hook_loot_function`, a no-op without the feature). A kind is either a native mod's or a Java mod's
  owned registry entry, never both, so the order only matters for the log.
- `G/java_bridges/effects.rs`: `block_get_drops` sets `JAVA_LOOT_LIVE` (a `thread_local! Cell<bool>` in `java_loot.rs`,
  with a guard that resets it) around its roll, so loot kinds met in that roll use Java's real params.

## Tests

- **Test mod**: SPEC-1's `dropper` table (`horizon_java_test:always` condition, `horizon_java_test:stamp` function). Check
  `ChecksBlockEntities` (or the family that breaks the dropper; `Breakables`) gains `stamped_drop`: after the wand's break of
  a dropper, the dropped item entity's stack has `custom_data {stamped: 1}`. The expected component is the test function's
  own definition, written in the check.
- `ChecksServer` `loot_kinds_isolated`: call `Loot.function` with a hand-built request for `horizon_java_test:stamp` on a
  stone stack: the reply's stack has the component; with `live` true and `LIVE` holding a builder whose `BLOCK_ENTITY` is a
  test-mod block entity, a test function variant `horizon_java_test:stamp_entity` (writes the block entity type id into
  `custom_data`) writes `horizon_java_test:breakable`.
- **Rust**: `java_hook_loot_function` sends nothing for a kind outside the owned set (the request builder is a pure
  function: test it returns `None`); the live flag guard resets after a panic in the roll (`catch_unwind` in the test).
- `java_mods.py build`, `check`, `unit`, `bridges`; `cargo test ... --lib java_loot`; clippy both ways.

## Done bar

- [ ] `bridges` prints `CHECK stamped_drop ok` and `CHECK loot_kinds_isolated ok`.
- [ ] RS on install-rs: a Controller charged from a Creative Energy source (or `/data` on its block entity, through
      `java_hook_block_entity_written`), broken with a pickaxe, drops a controller item whose tooltip (Java item text)
      shows the same stored energy; placed again, it holds that energy.
- [ ] An RS 1k storage block holding items, broken and placed again, still lists those items in a Grid (no data loss).
- [ ] No `loot function ... threw` line in either install's play log.

## Live steps for the lead

1. install-rs, creative: place `refinedstorage:creative_controller`, then `refinedstorage:grid` and
   `refinedstorage:1k_storage_block` touching it; insert 64 cobblestone through the Grid.
2. `/gamemode survival`, break the storage block with a pickaxe, pick it up, place it back touching the network: the Grid
   lists 64 cobblestone again. Before this spec it comes back empty (`refinedstorage:storage_block` never ran).
3. Place a plain `refinedstorage:controller` next to Mekanism's `creative_energy_cube` (set to output), wait until Jade
   shows it charged, break it: the item's tooltip shows stored energy; placed alone again it still has it
   (`refinedstorage:energy`, a table-level function in `blocks/controller.json`).
