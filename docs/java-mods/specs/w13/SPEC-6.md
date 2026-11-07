# SPEC-6 (W13): placements Java runs honour NeoForge's snapshot capture

Design: `DESIGN.md` §1.1 (bridged bodies), §1.4, D3. Read first: SPEC-1, SPEC-5 (the capture in horizon-world),
`G/java_bridges/world.rs` (`level_set_block`, the `ChunkViews.load` bridge as the model of a bridged static on a host
class), `J/plugin/.../BridgeProcessor.java` (what a bridge does to a method head), `B/BootFacts.java` (`ownsBlock`),
`B/mixin/` (accessor and inject mixins), `J/boot/res/horizon_java.mixins.json`, `docs/java-mods.md` rows "`Item.use`,
`Item.useOn`..." and "Mods' blocks".

Depends on SPEC-1 and SPEC-5. About 250 changed lines. Code comments: one line at most, only for a non-obvious why.

## The gap
A mod item's `useOn` runs in Java (`ItemCode`, the `use_item_on` op): `ItemStack.useOn` → NeoForge's
`CommonHooks.onPlaceItemIntoWorld` sets `level.captureBlockSnapshots = true` and calls `Item.useOn`. Every
`Level.setBlock` it makes is **bridged** to native (`level_set_block`), so NeoForge's capture code in `setBlock`'s body
never runs: no snapshot is recorded, native applies each write with full updates at once, and the loop after `useOn`
finds no snapshot, so no `EntityPlaceEvent`/`EntityMultiPlaceEvent` fires (Building Gadgets 2's gadgets, every mod block
item). Real NeoForge captures, posts, then runs `onPlace` and `markAndNotifyBlock` per snapshot.

## Java
- `B/mixin/LevelCaptureMixin.java` (new, server side, in `horizon_java.mixins.json`): `@Inject(method = "setBlock(Lnet/
  minecraft/core/BlockPos;Lnet/minecraft/world/level/block/state/BlockState;II)Z", at = @At("HEAD"), cancellable = true)`
  on `Level`. Mixin's HEAD lands before the bridge's jump (`docs/java-mods.md`: HEAD injections still apply). Body:
  `if (!(this instanceof ServerLevel) || !captureBlockSnapshots || !ServerEvents.placeListened()) return;` then exactly
  NeoForge's capture steps: `var snap = BlockSnapshot.create(dimension(), this, pos.immutable(), flags);`
  `capturedBlockSnapshots.add(snap);` `boolean changed = CaptureBridge.setBlock(pos, state, flags);` `if (!changed) capturedBlockSnapshots.remove(snap); else ServerEvents.CAPTURED.add(pos.asLong());`
  `cir.setReturnValue(changed);`. `ServerEvents.CAPTURED` is a `LongOpenHashSet`, cleared at the start and at the end
  of every `GameBridge.use` call (the only way a mod's `useOn` runs).
  `ServerEvents.placeListened()` reads bits 5 and 6 of `listened` (SPEC-1).
- `B/mixin/StateOnPlaceMixin.java` (new): `@Inject(method = "onPlace", at = @At("HEAD"), cancellable = true)` on
  `BlockBehaviour.BlockStateBase`: when `level instanceof ServerLevel`, `ServerEvents.CAPTURED.remove(pos.asLong())`
  is true (only NeoForge's post-event loop calls `onPlace` for a captured position) and
  `!BootFacts.ownsBlock(getBlock())`: `CaptureBridge.onPlace(pos, (BlockState) (Object) this, old, moved); ci.cancel();`. A vanilla block's `onPlace` is native's; a mod's block keeps its own Java `onPlace`; every
  other caller (`BlockEntities.blockChanged`) is untouched, so nothing runs twice.
- `B/CaptureBridge.java` (new): `static boolean setBlock(BlockPos pos, BlockState state, int flags)` and `static void
  onPlace(BlockPos pos, BlockState state, BlockState old, boolean moved)`, bodies that throw (the bridge replaces them), declared as the bridge model `ChunkViews.load` is (a host class
  method the bridge table names; check how `ChunkViews.load` is declared and copy it).
- `Level.markAndNotifyBlock` gets a native bridge (below), so NeoForge's post-event loop runs native's updates.

## Rust (`G/java_bridges/world.rs`)
```rust
"dev/horizonlab/javamods/boot/CaptureBridge".setBlock
"(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/state/BlockState;I)Z" =>
fn capture_set_block(game, pos: Pos, state: State, flags: i32) -> bool
"dev/horizonlab/javamods/boot/CaptureBridge".onPlace
"(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/block/state/BlockState;Z)V" =>
fn capture_on_place(game, pos: Pos, state: State, old: State, moved: bool) -> ()
"net/minecraft/world/level/Level".markAndNotifyBlock
"(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/chunk/LevelChunk;Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/block/state/BlockState;II)V"
if "net/minecraft/server/level/ServerLevel" =>
fn level_mark_and_notify(game, pos: Pos, _chunk: Skip, old: State, new: State, flags: i32, limit: i32) -> ()
```
- `Pos` and `State` are the existing `Arg`s (`level_set_block` uses both), so `CaptureBridge`'s natives take
  `(BlockPos, BlockState, int)` and `(BlockPos, BlockState, BlockState, boolean)`; the Java calls above pass `pos`,
  `state`, `old` as objects, not ids.
- `capture_set_block`: `with_level(|b, l, t| b.set_block_captured(l, t, pos, state, flags))`, then
  `after_java_change`. New in horizon-world (`update.rs`, beside SPEC-5's capture): `pub fn set_block_captured(&self,
  level, ticks, pos, state, flags) -> bool`: one write as `set_block_limit` does under capture (no `on_place`, no
  `mark_and_notify`). The observation hook (`observe_block_changed`) gets `flags | 512` (`UPDATE_SKIP_ON_PLACE`), so Java's
  `BlockEntities.blockChanged` skips a mod block's `onPlace` now; NeoForge's loop calls it later through
  `BlockState.onPlace` (the mod block's own Java body, not intercepted above).
- `capture_on_place`: `with_level(|b, l, t| b.on_place_at(l, t, pos, state, old, moved))`: a new `Behaviours` wrapper
  over `Sim::on_place` (`hooks.rs`). Then `after_java_change`.
- `level_mark_and_notify`: `with_level(|b, l, t| b.mark_and_notify(l, t, pos, old, new, flags, limit))`, a new
  `Behaviours` wrapper over SPEC-5's `Sim::mark_and_notify`; then `after_java_change`.
- `java_mods.py check` must pass for the three (descriptors and `Flat` overloads).

## Tests
- Java unit check `U/.../boot/CaptureMixinCheck.java` is not possible without a boot (mixins); use the test mod.
- Test mod: a test item `capture_wand` (`TestMod`, `ITEMS.registerItem`) whose `useOn` places three stone blocks in a
  row above the clicked block with `level.setBlock(pos, STONE, 3)`. Listener: records `EntityMultiPlaceEvent` snapshot
  counts; cancels when the player sneaks. Checks driven by `bridges`' `drive` (use the wand once standing, once
  sneaking):
  - `java_place_is_multi_place`: one `EntityMultiPlaceEvent` with 3 snapshots, each `getState()` air and
    `getCurrentState()` stone (independent of Horizon: NeoForge's own snapshot semantics).
  - `java_place_cancel_restores`: after the sneaking use, the three positions are air again.
  - `java_place_updates_after_event`: a redstone lamp beside the first position and the wand placing a redstone block
    instead of stone (a second mode): inside the listener the lamp is unlit (updates deferred), after the use it is lit.
- Rust: `capture_set_block_skips_updates` (horizon-world test level: a redstone block written with
  `set_block_captured` next to a lamp leaves it unlit; `mark_and_notify` lights it).

## Done bar
- `java_mods.py check` (the three bridges), `unit`, `bridges` (the three CHECKs).
- `cargo test -p horizon-world update`, `cargo test -p horizon-client --features java-mods --lib java_`, clippy both
  ways, fmt.
- Without a listener (install-rs: no architectury) the mixin returns at once: `bridges`' existing machine and wand
  checks unchanged.

## Live test (lead, showcase jars, `HORIZON_JAVA_DEBUG=1`)
1. Start with `OPTS="-XX:StartFlightRecording=jdk.MethodTrace#enabled=true,jdk.MethodTrace#filter=net.neoforged.neoforge.event.EventHooks::onMultiBlockPlace"`.
   Building Gadgets 2 building gadget, survival, build a 3x3 wall of stone: the wall appears as before. `jcmd <pid>
   JFR.dump filename=/home/umceko/hz-poc/agents/w13/bg2.jfr`, `jfr print --events jdk.MethodTrace bg2.jfr`: one call per
   gadget use. Delete the `.jfr` after.
2. A Mekanism machine placed from its item: places and works as before; one `EntityPlaceEvent`.
3. Carry On: pick up and place a chest: nothing doubles, the chest keeps its items.
