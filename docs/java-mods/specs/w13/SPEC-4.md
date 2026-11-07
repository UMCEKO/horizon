# SPEC-4 (W13): `PistonEvent.Pre` and `PistonEvent.Post`

Design: `DESIGN.md` §1.4, §4.4 ("block-event step"). Read first: SPEC-1, `crates/horizon-world/src/level/pistons.rs`
(`trigger_event`, `piston_trigger_event`, `run_block_events`), `G/game.rs` (`tick_blocks`' call to `run_block_events`
near line 5546, `with_level`, `process_level_events`, `sync_moving_pistons`), the patched `PistonBaseBlock.triggerEvent`
bytecode in `javap/callers.javap` (search `class net.minecraft.world.level.block.piston.PistonBaseBlock`).

Depends on SPEC-1. About 300 changed lines. Code comments: one line at most, only for a non-obvious why.

## Vanilla (patched jar)
`PistonBaseBlock.triggerEvent(state, level, pos, b0, b1)`: `direction`, `extendedState`; server only: the signal
re-check (`extend && (b0 == 1 || b0 == 2)` → `setBlock(extended, 2)`, return false; `!extend && b0 == 0` → return false);
then `EventHooks.onPistonMovePre(level, pos, direction, b0 == 0)`: true (canceled) returns false. Then vanilla's move
(`b0 == 0`: `moveBlocks`...; `b0 == 1|2`: retract), and on every path that reaches the end,
`EventHooks.onPistonMovePost(level, pos, direction, b0 == 0)` right before `return true`. Paths that `return false`
after `Pre` (a failed `moveBlocks`) post no `Post`. `PistonEvent.Pre` with `PistonMoveType` `EXTEND` for `b0 == 0`,
`RETRACT` otherwise.

Horizon's `piston_trigger_event` lines 703–712 are the re-check; everything after is the move.

## horizon-world (`pistons.rs`)
Split the method at the re-check, no behaviour change:
```rust
/// What `triggerEvent` does before NeoForge's `PistonEvent.Pre`.
pub enum PistonStep { Done(bool), Move { direction: Direction, extend: bool } }
fn piston_trigger_begin(&mut self, pos, state, a) -> PistonStep        // lines 693–712
fn piston_trigger_finish(&mut self, pos, state, a, param_b) -> bool    // lines 713–end
```
`piston_trigger_event` becomes `match begin { Done(b) => b, Move{..} => finish(..) }`.

A stepped block-event queue, beside `run_block_events`, sharing its body (factor the per-event part out of the loop):
```rust
/// One queued block event of `runBlockEvents`, or `None` when the queue is empty (rescheduled ones are kept aside).
pub fn next_block_event<L>(&self, level: &mut L, ticks: &mut Ticks) -> Option<BlockEventStep>
pub enum BlockEventStep { Ran, Piston { event: BlockEventData, state: BlockStateId, direction: Direction, extend: bool } }
/// Runs the rest of a piston event after `PistonEvent.Pre` let it; true when it moved (broadcast and `Post`).
pub fn finish_piston_event<L>(&self, level: &mut L, ticks: &mut Ticks, event: BlockEventData, state: BlockStateId) -> bool
/// Puts back the events `next_block_event` set aside (unticking chunks), as the loop's end does.
pub fn end_block_events(&self, ticks: &mut Ticks)
```
- `next_block_event` pops the first event; unticking → onto `ticks.block_events_aside` (a new `Vec` in `Ticks`), loop to
  the next; block mismatch → loop; a piston base whose `piston_trigger_begin` returns `Move` → `Piston{..}`; any other
  event (and a piston's `Done`) runs exactly as today, emitting `BlockEntityEvent` when handled → `Ran`.
- `finish_piston_event` runs `piston_trigger_finish` and on true emits the `BlockEntityEvent` as `run_block_events` does.
- `run_block_events` keeps its signature and result; rewrite it on these three so the two paths can't diverge.
- Client side (`client_block_event`) is unchanged: no events on the client.

## Rust (`G/game.rs`, hooks in `G/java_hooks.rs`)
- `java_hooks.rs` pair: `java_hook_piston(&mut self, pos: [i32; 3], direction: Direction, extend: bool, post: bool) -> bool`
  (true = Pre canceled; Post's return is ignored). The java arm: `level_event` id 3 (Pre) or 4 (Post), tail `long pos`,
  `byte direction` (3D data value), `bool extend`; result empty. Returns false at once unless the id is listened.
- In the tick, replace the single `with_level(|b, level, ticks| b.run_block_events(level, ticks))` by
  `self.run_block_events_stepped()` when `java_listens(PistonPre) || java_listens(PistonPost)`, else keep the old call.
  ```rust
  /// `ServerLevel.runBlockEvents` one event at a time, so a piston's NeoForge events cross with no level borrowed.
  fn run_block_events_stepped(&mut self)
  ```
  Loop: `match self.with_level(|b, l, t| b.next_block_event(l, t))`; `None` → break; `Ran` → continue; `Piston{event,
  state, direction, extend}` → `if self.java_hook_piston(pos, direction, extend, false) { continue }` (Pre canceled:
  triggerEvent returns false, nothing broadcast); `let moved = self.with_level(|b, l, t| b.finish_piston_event(l, t,
  event, state));` `if moved { self.java_hook_piston(pos, direction, extend, true); }`. After the loop
  `self.block_ticks` → `end_block_events`. Call `process_level_events` where the old path did (after the whole loop),
  so sounds, particles and drops keep their order.
- `JavaEvent::PistonPre`, `PistonPost` into `SITED`.

## Java (`B/ServerEvents.java`, cases 3 and 4)
`var pos = BlockPos.of(in.readLong()); var dir = Direction.from3DDataValue(in.readByte()); boolean extend = in.readBoolean();`
case 3: `canceled = EventHooks.onPistonMovePre(level, pos, dir, extend)`; case 4: `EventHooks.onPistonMovePost(level,
pos, dir, extend)`. Both through `ServerEvents.post`. Debug: `Horizon: server event PistonEvent.Pre <pos> <dir>
extend=<b> canceled=<b>`.

## Tests
- horizon-world (`pistons.rs` tests or `level/tests.rs`, with the existing test level): `the_stepped_queue_matches_the_loop`:
  the same scene (a piston and a redstone block, a sticky piston pulling stone, a piston whose move fails against
  obsidian, a note block event, an event in an unticking chunk) run once with `run_block_events` and once with
  `next_block_event` / `finish_piston_event` / `end_block_events` gives the same world, the same `LevelEvent`s in the
  same order and the same queue left over. Expected values come from the old loop, which the parity harnesses already
  pin against vanilla (`pistons`).
- `a_failed_move_posts_no_post`: the obsidian case's stepped run reports `finish_piston_event` false.
- Test mod (`ChecksServerEvents`): Pre cancels when the piston is at the marked point; `piston_pre_cancel_keeps_it_retracted`:
  piston at the marked point with stone in front, a redstone block set beside it from Java (`level.setBlock`, bridged),
  then wait for the tick (the check runs in `later`): the piston is `extended=false` and the stone hasn't moved.
  `piston_post_fires_after_move`: a second piston not at the mark; the Post listener records
  `level.getBlockState(pos.relative(dir))` and it is `minecraft:moving_piston` or the piston head (the move happened
  before Post).

## Done bar
- `cargo test -p horizon-world pistons` (and the new tests), `cargo test -p horizon-client --features java-mods --lib
  java_events`, clippy both ways, fmt.
- Harnesses through `verify/parity.sh`: `pistons` (or the nearest names `tools/parity.py --changed-files` lists for
  `pistons.rs`), at most two.
- `bridges` CHECKs pass. M1 `w13` step `piston`: `PistonEvent$Pre` and `$Post` counts equal Forge's (2 each).
- Append to `tools/scenarios/w13-events.json`: a check for `PistonEvent.Pre` since `piston`.

## Live test (lead, showcase jars, `HORIZON_JAVA_DEBUG=1`)
1. Build a piston facing a stone block, power it with a lever, unpower it. The stone moves out and stays (no pull),
   as without mods; the log has two `PistonEvent.Pre` lines, `extend=true` then `extend=false`.
2. A sticky piston pulling a block, and a slime-block flying machine: behaviour unchanged, one Pre/Post pair per move.
3. With no listener (install-rs: architectury absent), the log has no `PistonEvent` line and `tb_block_events` in
   `HORIZON_PROFILE`'s `server-ticks.log` is unchanged.
