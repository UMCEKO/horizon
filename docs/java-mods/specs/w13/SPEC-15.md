# SPEC-15 (W13): `BlockEvent.NeighborNotifyEvent`

Design: `DESIGN.md` §1.1 (the bridged `ServerLevel.updateNeighborsAt`), §4.4, D1. Read first: SPEC-1, SPEC-14 (cascade
crossings: this spec only adds a hook on them), `crates/horizon-world/src/level/update.rs` (`Sim::update_neighbors_at`,
`update_neighbors_at_oriented`, `update_neighbors_at_except_oriented`), `crates/horizon-world/src/level/redstone.rs`
`diode_update_neighbors_in_front`, the patched bytecode of `ServerLevel.updateNeighborsAt`,
`ServerLevel.updateNeighborsAtExceptFromFacing`, `Level.updateNeighborsAt`, `DiodeBlock.updateNeighborsInFront`
(`javap/callers.javap`).

Depends on SPEC-1 and SPEC-14 (and D1). Land it only when SPEC-1's census shows `NeighborNotifyEvent` listened in a bar
pack (none in the static evidence: Mekanism Generators and Tom's Storage post it, they don't listen). About 200 changed
lines. Code comments: one line at most, only for a non-obvious why.

## Vanilla + NeoForge (patched jar)
- `ServerLevel.updateNeighborsAt(pos, block)` (the two-argument form): `EventHooks.onNeighborNotify(this, pos,
  getBlockState(pos), EnumSet.allOf(Direction.class), false)`; canceled → return (no update queued); else the oriented
  form. The oriented three-argument form posts nothing.
- `ServerLevel.updateNeighborsAtExceptFromFacing(pos, block, skip, orientation)`: the same with `allOf` minus `skip`.
- `DiodeBlock.updateNeighborsInFront(level, pos, state)`: the event first, at the diode's own position with a single side
  (read which one from the bytecode: the `Direction` passed to `EnumSet.of`), canceled → return; then
  `neighborChanged` and `updateNeighborsAtExceptFromFacing`, which posts its own event.
- `forceRedstoneUpdate` is `false` at every site (`iconst_0` before each call).

## horizon-world
`LevelAccess` gets
```rust
/// `EventHooks.onNeighborNotify`: true when a mod canceled the neighbour updates from `pos`; `sides` is a bit per `Direction` ordinal.
fn neighbor_notify(&mut self, _ticks: &mut Ticks, _pos: BlockPos, _state: BlockStateId, _sides: u8) -> bool { false }
```
- `Sim::update_neighbors_at` (the native port of the two-argument form; its callers are the ones vanilla's two-argument
  form has, check with the port ledger: `python3 tools/port_ledger.py where ServerLevel.updateNeighborsAt`): first
  `if self.level.neighbor_notify(self.ticks, pos, self.state(pos), 0b11_1111) { return }`.
- `Sim::update_neighbors_at_except_oriented`: the same with the `skip` bit cleared.
- `diode_update_neighbors_in_front`: the event first with the one side, canceled → return.
- `update_neighbors_at_oriented` posts nothing (vanilla's oriented form doesn't). If a native caller of
  `update_neighbors_at_oriented` mirrors vanilla's two-argument call (the ledger says which vanilla caller each mirrors),
  route it through `update_neighbors_at` first, no behaviour change, so the event fires where vanilla's does.

## `GameLevel` (`G/java_cascade.rs`)
`neighbor_notify`: when `native_authoritative` and `java_listens_now(NeighborNotify)`: `cascade_event(self, ticks,
NeighborNotify, |out| { long pos, int javaState, byte sides })` → `canceled`. Else false.

## Java (`ServerEvents` case 27)
`var sides = EnumSet.noneOf(Direction.class)` filled from the bits by `Direction.values()[ordinal]`;
`canceled = EventHooks.onNeighborNotify(level, pos, state, sides, false).isCanceled()`. Debug (per call, debug only):
`Horizon: server event BlockEvent.NeighborNotifyEvent <pos> sides=<n> canceled=<b>`.
`JavaEvent::NeighborNotify` into `SITED`.

## Tests
- horizon-world (test level): `a_canceled_notify_queues_nothing`: a level whose hook cancels at one position; a
  redstone block placed there next to a lamp leaves the lamp unlit (vanilla: no neighbour update was queued); with the
  default hook the lamp lights (the existing redstone tests pin that).
- `except_facing_clears_its_side`: the hook records `sides`; `update_neighbors_at_except_oriented` with skip `Up` reports
  all bits but `Up`'s.
- Test mod: a `NeighborNotifyEvent` listener canceling at the marked position; `drive` places a redstone block at the
  marked position beside a lamp (through Java's bridged `setBlock`, which reaches the native cascade). Checks:
  `notify_cancel_keeps_lamp_off`, `notify_sides_all_six` (an unmarked placement's event had six sides).

## Done bar
- `cargo test -p horizon-world` (redstone and update tests), `cargo test -p horizon-client --features java-mods --lib
  java_cascade`, clippy both ways, fmt; the redstone harness `tools/parity.py --changed-files` lists for `update.rs` and
  `redstone.rs` (at most two) unchanged.
- `bridges` CHECKs pass. M1 `w13` (whole run): `BlockEvent$NeighborNotifyEvent` within tolerance of Forge's.
- `HORIZON_PROFILE` with a 64-repeater clock running: the `level_event` op's time per tick reported to the lead before
  this lands (it runs per neighbour update while listened).

## Live test (lead, the pack whose census listed the listener, `HORIZON_JAVA_DEBUG=1`)
1. A redstone clock, a piston door and a hopper line: behaviour unchanged; `NeighborNotifyEvent` lines appear.
2. The listening mod's own feature that reacts to neighbour updates (its docs say which) works.
