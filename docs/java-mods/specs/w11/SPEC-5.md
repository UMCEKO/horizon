# SPEC-5 (W11): the loot seam in both engines (Rust only)

Read first: `DESIGN.md` G4, D4, D5; javap of the patched `LootTable` (DESIGN G4: every public `getRandomItems` and `fill` go
through the private `getRandomItems(LootContext)`, which calls `CommonHooks.modifyLoot` after the stack splitter; `fill`
then draws `getAvailableSlots` and `shuffleAndSplitItems` from the same random). Rust: `W/loot.rs` (`roll`, `roll_entity`,
`roll_split`, `fill_with`, `native_ctx`, `LootStack`), `W/native_load_hooks.rs`, `I/items.rs` (`evaluate` and its
top-level callers: `block_drops_raw`, `loot_table`, `inline_loot_table`, `inline_loot_table_with_tool`, `interact_loot`,
`loot_table_from`, `entity_drops`; `native_loot_context`, `loot_of`, `apply_loot`), `L/entry.rs` (hook install),
`H/bridge.rs` (`with_active`, `enter`), `horizon-core/src/random.rs`.

Depends on: nothing. Lands alone: with no hook that wants a table, every roll is bit for bit today's (no conversion, no
draw). SPEC-6 installs the Java arm.

Comments in code: one line at most, only for a non-obvious why.

## Goal

One hook at each place NeoForge runs `modifyLoot`: after a top-level table's roll and stack splitter, before a chest's slot
shuffle, in both native engines. It sees the table id, the roll's context, the stacks, and the random's state, which it may
advance. Plus `bridge::isolated`, the safe way to call Java from inside a roll (D4).

## `horizon-core/src/random.rs`

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RandomState { Legacy(u64), Xoroshiro(u64, u64) }
```
`RandomSource` gains two default methods: `fn snapshot(&self) -> Option<RandomState> { None }` and
`fn restore(&mut self, state: RandomState) -> bool { false }`. `LegacyRandom` (its 48-bit `state`) and `XoroshiroRandom`
(`state_words`, `from_state`) implement both; `restore` also clears the cached gaussian (vanilla's `setSeed` resets
`haveNextNextGaussian`). If loot rolls run on another `RandomSource` type (`rg 'impl.*RandomSource for'`), implement it
there too or make the engine skip the hook for it with `log::warn!` once: never a fresh random.

## `W/native_load_hooks.rs`

`NativeLoadHooks` gains:
- `pub loot_wanted: fn(&str) -> bool`: cheap; `false` means no hook runs for that table id.
- `pub loot_rolled: fn(&str, &NativeLootContext<'_>, Option<&LootStack>, &mut Option<RandomState>, &mut Vec<LootStack>)`:
  the table id, the roll's context, the tool as a whole stack (the items engine's `held` through `loot_of`; `None` in the
  main engine, whose context has no tool), the random's state, the stacks.
Crate functions `loot_wanted(id)` (false when nothing is installed) and `loot_rolled(..)`. Update every `NativeLoadHooks`
literal (`L/entry.rs`, `W/item_defaults.rs`, `W/tests/native_load_hooks.rs`): `entry.rs` installs
`loot_wanted: |id| crate::game::java_hook_loot_wanted(id)` and `loot_rolled: |..| crate::game::java_hook_loot_rolled(..)`
(free functions in `G/java_hooks.rs`, `false`/no-op without the feature; SPEC-6 fills them).

## `W/loot.rs` (main engine)

- `fn rolled<R: RandomSource>(table: &str, ctx: NativeLootContext, random: &mut R, items: &mut Vec<LootStack>)`: if
  `!loot_wanted(table)` return; take `random.snapshot()` (None: warn once, return); call `loot_rolled`; if the state came
  back changed, `random.restore(state)`.
- Call it in `roll`, `roll_entity`, `roll_split` after `*random = c.random;`, and in `fill_with` after `*random = c.random;`
  and before `shuffle(&mut available, random)`. The context: a new `Context::hook_ctx(&self)`, like `native_ctx` but with
  `seed: 0` and no draw (the hook gets the random itself).
- `LootStack::to_codec_json(&self) -> Value` and `LootStack::from_codec_json(&Value) -> Option<LootStack>`: vanilla's
  `ItemStack.CODEC` JSON form (`{"id", "count", "components": {..}}`). Typed fields map to their components
  (`minecraft:damage`, `minecraft:enchantments`, `minecraft:potion_contents`, `minecraft:custom_name`/`item_name` from
  `names`, `minecraft:suspicious_stew_effects`, `minecraft:ominous_bottle_amplifier`); `components` are written as they
  are; `from_codec_json` fills the typed fields from those components and keeps every other component in `components`.
  A stack with `map` or `exploration` set is written with its other fields; the caller keeps the original when Java
  returns it unchanged (below).
- `NativeLootPlace` gains `pub this_entity: Option<(i32, u128)>` (entity id and uuid of `THIS_ENTITY`: the breaking
  player, the dying mob) and `pub killer: Option<u128>` (`LAST_DAMAGE_PLAYER`); document them with the next API number in
  `docs/native-mods.md` as the struct's other fields are. Fill them where the game builds a place: `G/game.rs`
  `block_loot_at` (the acting player: `native_acting_id` and its entity id) and the mob-death sites that call
  `with_native_loot_place` (`rg with_native_loot_place crates/horizon-client/src`). Java finds its views by these ids.

## `I/items.rs` (items engine)

- `fn rolled<R: RandomSource>(&self, table: &str, random: &mut R, out: &mut Vec<ItemStack>)`, called right after each
  top-level `evaluate` in the callers listed above (not inside `evaluate`: a nested `loot_table` entry is
  `getRandomItemsRaw`, which NeoForge doesn't modify). Inline tables pass `""`; SPEC-6's Java side names them
  `LootTableIdCondition.UNKNOWN_LOOT_TABLE`.
- It converts with `loot_of`, calls the hook through the same `W` function as the main engine (make `rolled` in `W/loot.rs`
  `pub` and generic over the context), and rebuilds: an output equal to the input at the same index keeps the original
  `ItemStack` (no data lost); any other becomes `stack_from_loot(&LootStack)` = `ItemStack::new` + `apply_loot` from that
  item's empty loot form.
- `loot_of` must be total for what the engine makes: add whatever `StackData` field it skips (check every field of
  `StackData` against `apply_loot`) so `apply_loot(loot_of(s))` gives `s` back.

## `H/bridge.rs`

`pub fn isolated<R>(f: impl FnOnce() -> R) -> R`: `ACTIVE` set to null for `f`, restored after (also on unwind: a guard
struct). A bridged native entered inside it finds no game and throws to Java, as `enter` already does. Plus
`pub fn has_active() -> bool` for tests and debug assertions.

## Tests

- `horizon-core`: `LegacyRandom::new(42)` after 3 `next_int(10)`: `snapshot`, `restore` into `LegacyRandom::new(0)`, and the
  next 5 `next_int(100)` equal the original's next 5; the expected numbers are literals produced once with `jshell`
  (`var r = new java.util.Random(42); ...`), pasted with the command in a one-line comment. Same for `XoroshiroRandom`
  with literals from vanilla's `XoroshiroRandomSource(1L, 2L)` (a `U/` check can print them: `U/RandomStateCheck.java`).
- `W/loot.rs`: `to_codec_json`/`from_codec_json` round trip for a stack with damage 3, Sharpness 2, a potion, a custom name
  and a `minecraft:custom_data` component; the expected JSON literal comes from vanilla's `ItemStack.CODEC` (a `U/` check,
  `U/LootJsonCheck.java`, prints it once for the same stack; paste it as the test's expected value).
- `W/tests/native_load_hooks.rs` (the process installs once; put the new cases in this file): a hook that wants
  `minecraft:chests/simple_dungeon` only, appends `minecraft:diamond` and advances the state by one `next_int`
  (`LegacyRandom::from_state`, draw, snapshot back). `fill_with` on that table with seed 7 holds a diamond, and its slots
  equal a hand-run of the same steps (roll; one `next_int` on the random; `shuffle`; `shuffle_and_split`) written in the
  test without the hook. A table the hook doesn't want: output equal to a run with no hook installed, and the hook was
  never called (a counter).
- `I/items.rs`: a hook that leaves the list as is keeps a stack carrying a component outside `LootStack`'s typed fields
  byte-identical; one that replaces stack 0 with `minecraft:stick` x2 yields that `ItemStack`.
- `H/bridge.rs`: `with_active(&mut x, || isolated(has_active))` is false, and `has_active()` is true again after.
- `cargo test -p horizon-core random`, `-p horizon-world loot native_load_hooks`, `-p horizon-items`,
  `-p horizon-java-host bridge`; clippy; loot harnesses through `/home/umceko/hz-poc/verify/parity.sh` (at most 5 the
  `--changed-files ... --list` names for `loot.rs` and `items.rs`; they must be unchanged with no hook).

## Done bar

- [ ] All tests above; the named loot parity harnesses pass unchanged.
- [ ] A default build and a `java-mods` build compile; with java-mods but no SPEC-6 the hooks answer `false`.

## Live steps for the lead

None: no behaviour changes until SPEC-6.
