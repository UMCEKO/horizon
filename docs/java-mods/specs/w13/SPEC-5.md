# SPEC-5 (W13): `BlockEvent.EntityPlaceEvent` / `EntityMultiPlaceEvent` for native placements

Design: `DESIGN.md` §1.4 (`onPlaceItemIntoWorld`), D3. Read first: SPEC-1, `G/game.rs` `use_hand` (the tail from
`// BlockItem.place through the vanilla placement rules.` to `consume_held`), `crates/horizon-world/src/level/place.rs`
(`Behaviours::place_block`, `Sim::place`, `PlaceOutcome`), `crates/horizon-world/src/level/update.rs`
(`Sim::set_block_limit`, `chunk_set_block_state`), `level/hooks.rs` `on_place`, the patched bytecode of
`CommonHooks.onPlaceItemIntoWorld` (`javap/nfhooks.javap`), `Level.setBlock` and `LevelChunk.setBlockState`
(`javap/callers.javap`).

Depends on SPEC-1. About 450 changed lines. Code comments: one line at most, only for a non-obvious why.

## Vanilla + NeoForge, exactly
`ItemStack.useOn` → `CommonHooks.onPlaceItemIntoWorld(context)`:
1. `level.captureBlockSnapshots = true`; `Item.useOn` → `BlockItem.place`: each `Level.setBlock` while capturing records
   `BlockSnapshot.create(dim, level, pos, flags)` (the **old** state, before the write), writes the chunk (old block
   entity removal and `affectNeighborsAfterRemoval` still run; `onPlace` does not), and skips `markAndNotifyBlock`.
   `BlockItem.place` then runs `setPlacedBy` (its own writes are captured too: a door's top, a bed's head), applies the
   item's block entity data, fires the `placed_block` trigger, plays the place sound, posts `BLOCK_PLACE`, and consumes
   one item.
2. Capture off. If the result consumes the action and snapshots exist: more than one → `EventHooks.onMultiBlockPlace(
   player, snapshots, face)`, else `EventHooks.onBlockPlace(player, snapshot, face)` (`face` = the clicked face).
3. Canceled: snapshots restored in **reverse** order (`snapshot.restore(flags | UPDATE_CLIENTS)`: `setBlock(pos, old,
   flags)`), the stack's count and components put back; the result is `FAIL`.
4. Not canceled: for each snapshot in order, `newState.onPlace(level, pos, oldState, false)` then
   `level.markAndNotifyBlock(pos, chunk, oldState, newState, flags, 512)`; then `awardStat(ITEM_USED)`.

Without a listener (D3) Horizon keeps vanilla's own order (write with updates, then `setPlacedBy`): nothing changes.

## horizon-world
- `update.rs`: factor the update half of `Sim::set_block_limit` (everything after `chunk_set_block_state` returned,
  when `new == state`) into `fn mark_and_notify(&mut self, pos, old, state, update_flags, limit)`. No behaviour change.
- `Sim` gets `capture: Option<Vec<Snapshot>>` (`None` normally).
  ```rust
  /// A block write NeoForge's placement capture recorded: where, what was there, the write's flags.
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub struct Snapshot { pub pos: BlockPos, pub old: BlockStateId, pub flags: i32 }
  ```
  While `capture` is `Some`, `set_block_limit` pushes `Snapshot { pos, old: self.state(pos), flags }` before the write,
  `chunk_set_block_state` skips its `on_place` call (only that), and `set_block_limit` skips `mark_and_notify`. A write
  that changes nothing (`chunk_set_block_state` returns `None`) pops its snapshot, as NeoForge's `remove` does.
- `Behaviours` (in `place.rs`):
  ```rust
  /// `BlockItem.place` under NeoForge's snapshot capture: written, `setPlacedBy` run, no `onPlace`, no updates.
  pub fn place_block_captured<L>(&self, level: &mut L, ticks: &mut Ticks, ctx: &PlaceContext) -> (PlaceOutcome, Vec<Snapshot>)
  /// After the event let it: `onPlace` then `markAndNotifyBlock(…, 512)` per snapshot, in order.
  pub fn finish_captured<L>(&self, level: &mut L, ticks: &mut Ticks, snapshots: &[Snapshot])
  /// After a cancel: each snapshot's old state written back with `flags | UPDATE_CLIENTS`, last first.
  pub fn restore_captured<L>(&self, level: &mut L, ticks: &mut Ticks, snapshots: &[Snapshot])
  ```
  `finish_captured` reads the new state from the level at each position (as NeoForge reads `level.getBlockState`).
  Replaceable blocks carry no block entity in vanilla, so a snapshot holds none; `restore_captured` removes a block
  entity the placement created through the normal write path.

## Rust (`G/game.rs` `use_hand`, `G/java_hooks.rs`, `G/java_events.rs`)
- `java_hooks.rs` pair: `java_hook_place(&mut self, face: Direction, snapshots: &[Snapshot]) -> bool` (true = canceled);
  not-arm false.
- In `use_hand`, branch at the `with_level(... place_block ...)` call:
  `let capture = self.java_listens(JavaEvent::EntityPlace) || self.java_listens(JavaEvent::EntityMultiPlace);`
  - `!capture`: today's code, untouched.
  - `capture`: `place_block_captured` instead; on `Placed`, the same steps as today in vanilla's order: block state from
    the item (`apply_item_block_state`), block entity and `setPlacedBy` work, `swing_arm`, the solid bucket's sound
    swap, `process_level_events` (sound, game event), `trigger_block("minecraft:placed_block", ...)`, then
    **`consume_held` before the event** (vanilla consumes inside `BlockItem.place`; keep a copy of the held stack).
    Then `if self.java_hook_place(face, &snapshots)`: `with_level(restore_captured)`, put the held stack back from the
    copy, `process_level_events`, `self.use_failed = true`, return `Ok(true)` (the click was handled, as `FAIL` is).
    Else `with_level(finish_captured)`, `process_level_events`, then the rest as today minus what already ran:
    `award_stat("used", …)`, `native_place_block`, `check_wither_spawn`, `check_golem_spawn`, the solid bucket's
    bucket.
  - Write the two paths as two private fns (`place_vanilla_block`, `place_vanilla_block_captured`) called from
    `use_hand`, sharing the per-step helpers, so `use_hand` doesn't grow.
- Java side of the hook (`java_events.rs`): `player_event` id 5, tail `byte face` (3D data value), `int n` x (`long pos`,
  `int oldState` as Java's id through the bound `StateMap`, `int flags`). Result empty.
- `JavaEvent::EntityPlace`, `EntityMultiPlace` into `SITED`.
- Guests and the window's player on its server thread reach `use_hand` through `as_guest`, so they are covered.

## Java
- `B/mixin/BlockSnapshotAccess.java` (new, listed in `horizon_java.mixins.json` `mixins`): an `@Invoker("<init>")` on
  `BlockSnapshot`'s private constructor `(ResourceKey<Level>, LevelAccessor, BlockPos, BlockState, CompoundTag, int)`.
- `ServerEvents.player` case 5: build the snapshots with the invoker (`level.dimension()`, the level view, pos, the old
  state by id, `null` tag, flags), then, as `onPlaceItemIntoWorld` does: `n > 1` →
  `canceled = EventHooks.onMultiBlockPlace(call.player, snapshots, face)`, else `EventHooks.onBlockPlace(call.player,
  snapshots.get(0), face)`. Debug line: `Horizon: server event BlockEvent.EntityPlaceEvent n=<n> canceled=<b>`.

## Tests
- horizon-world (`level/tests.rs`, its test level): `a_captured_placement_defers_on_place_and_updates`: placing a
  redstone torch next to a redstone lamp with `place_block_captured` writes the torch, leaves the lamp unlit and records
  one snapshot (air, flags 11); `finish_captured` then lights the lamp (vanilla's `onPlace` → neighbour update), and the
  final world equals `place_block`'s on a fresh level.
- `a_door_records_two_snapshots`: an oak door placement gives two snapshots, bottom first (vanilla's order: the bottom
  written by `place`, the top by `setPlacedBy`).
- `restore_puts_back_the_old_states_last_first`: after `restore_captured` the two positions hold air and the world
  equals the level before the placement.
- Test mod (`ChecksServerEvents`): a listener cancels any `EntityPlaceEvent` whose placed block is `minecraft:diamond_block`
  and records each event's class and snapshot count. Driven by `java_mods.py bridges`' `drive` (add: give the player a
  diamond block and oak door, place both on the test floor with horizon-ctl `place`): `place_cancel_restores`: the
  diamond block's position is air and the stack count unchanged; `door_is_multi_place`: the door's event was
  `EntityMultiPlaceEvent` with 2 snapshots.

## Done bar
- `cargo test -p horizon-world` (the three new tests and the existing placement ones), `cargo test -p horizon-client
  --features java-mods --lib java_events`, clippy both ways, fmt.
- Harnesses (`verify/parity.sh`, at most two): the placement ones `tools/parity.py --changed-files` names for `place.rs`
  and `update.rs` (expected: `block_place`-like names; they run without listeners, so they must be unchanged).
- `bridges` CHECKs pass. M1 `w13` steps `place` and `door`: `BlockEvent$EntityPlaceEvent` 1 and
  `BlockEvent$EntityMultiPlaceEvent` 1, as on Forge.
- Append to `tools/scenarios/w13-events.json`: checks for `EntityPlaceEvent n=1` since `place`, `n=2` since `door`.

## Live test (lead, showcase jars, `HORIZON_JAVA_DEBUG=1`)
1. Survival, place a torch, a door, a bed, a chest. Each looks and behaves as before (the door opens, the bed has both
   halves, the chest opens); one `EntityPlaceEvent` debug line each (`n=2` for door and bed).
2. Place a redstone torch beside a redstone lamp: the lamp lights the same tick.
3. Place a pumpkin on two stacked snow blocks: the snow golem appears (the golem check after the deferred updates).
