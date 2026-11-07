# SPEC-14 (W13): cascade crossings, and `CropGrowEvent.Pre` / `.Post`

Design: `DESIGN.md` §4.4 ("Level cascade"), D1 (**the user's answer to D1 gates this spec**; land it only once SPEC-1's
census on the bar packs shows `CropGrowEvent` or `NeighborNotifyEvent` listened, as balm may). Read first: SPEC-1,
`crates/horizon-java-host/src/bridge.rs` (`ACTIVE`, `with_active`, `enter`, `java_bridges!`), `G/java_bridges/world.rs`,
`G/game.rs` (`struct GameLevel`, `impl LevelAccess for GameLevel`, `with_level`), `crates/horizon-world/src/level/mod.rs`
(`LevelAccess`), `crates/horizon-world/src/level/tick.rs` (the grower random ticks: `crop_random_tick`,
`stem_random_tick`, `cactus_random_tick`, `bamboo_grow`, `chorus_flower_random_tick`, and the cocoa, nether wart, sugar
cane, sweet berry bush and growing plant head ticks), `docs/java-mods-map.md` §8 ("Calls don't nest from Rust").

Depends on SPEC-1. About 480 changed lines. Code comments: one line at most, only for a non-obvious why.

## Part A: cascade crossings (infrastructure)
A level cascade runs inside `with_level`: `GameLevel` holds `&mut World` and the level's `&mut Ticks` for as long as it
runs, so a crossing there may not hand Java `&mut Game`. Vanilla's listeners inside a block update call back into the
level (a nested `setBlock` joins the running update). A cascade crossing gives Java exactly that and nothing else.

### `crates/horizon-java-host/src/bridge.rs`
- Beside `ACTIVE`: `static CASCADE: Cell<(*mut (), &'static str)>` (the context and the event's name).
  ```rust
  /// Runs `f`, a call into Java from inside a level cascade: bridges with a cascade body act on `cx`, the others throw.
  pub fn with_cascade<C, R>(cx: &mut C, event: &'static str, f: impl FnOnce() -> R) -> R
  ```
  It sets `CASCADE`, sets `ACTIVE` to null for the call (no `&mut Game` may exist), and restores both after.
- `enter`: when `ACTIVE` is null and `CASCADE` is set, throw `IllegalStateException("Horizon: <native name> isn't served
  inside a block update (<event>)")` instead of "outside a call from the game"; log it once per native name.
- `java_bridges!` gains an optional header part and an optional per-entry part:
  `java_bridges! { table for Game<'static>, cascade GameCascade { ... entry ... cascade(cx) { body } } }`. An entry with
  `cascade(cx) { ... }` generates a native that runs that body with `cx: &mut GameCascade` (from `CASCADE`) when a cascade
  is active, else the normal body through `enter`. Families without the header part are unchanged. Add a macro test
  (beside the module's existing tests) with a dummy game type, a dummy cascade type and one entry with each kind.

### `G/java_cascade.rs` (new, java-mods only)
```rust
/// The running cascade as Java's bridges see it inside a crossing: the level, its ticks, the behaviours.
pub(crate) struct GameCascade { pub b: *const Behaviours, pub level: *mut GameLevel<'static>, pub ticks: *mut Ticks }
```
with safe accessors used only inside cascade bodies (the pointers live for the `with_cascade` call; one SAFETY comment
line on the accessor). And:
```rust
/// One `level_event` crossing from inside a cascade: `(canceled, result)`, Java's sounds emitted as level events.
pub(super) fn cascade_event(level: &mut GameLevel<'_>, ticks: &mut Ticks, ev: JavaEvent, body: impl FnOnce(&mut Out)) -> Option<(bool, Vec<u8>)>
```
It writes the `level_event` header from `GameLevel` (dimension id, `game_time()`), the id, the body; calls
`host.call_with("level_event", ...)` inside `bridge::with_cascade`; reads `canceled`, the result, and the effects block,
turning each sound into `level.emit(LevelEvent::Sound { .. })` (no `Game` to play it on: the level's events are played
after the cascade, in order). A free fn `java_listens_now(ev) -> bool` reads the same bit as `Game::java_listens`
(the thread-local), since `GameLevel` has no `Game`.

### Cascade bodies (`G/java_bridges/world.rs`, header `cascade GameCascade`)
`getBlockState` (`level.block_state`), `isLoaded`, `setBlock`, `removeBlock`, `destroyBlock`,
`updateNeighbourForOutputSignal`, `updateNeighborsAt`, `scheduleTick`, `hasNeighborSignal`, `getBestNeighborSignal`,
`getSignal`, `getRawBrightness`, `levelEvent` (emit `LevelEvent::LevelEvent`), `gameEvent` (emit
`LevelEvent::GameEvent`): each the same `Behaviours` call its `with_level` body makes, on `cx`'s level and ticks. That is
re-entrant by construction: a listener's `setBlock` runs inside the cascade that asked, as vanilla's does. Every other
bridge throws inside a crossing (D1).

## Part B: `CropGrowEvent`
### Vanilla + NeoForge
In each of the ten growers (`CropBlock`, `StemBlock`, `CactusBlock`, `SugarCaneBlock`, `BambooStalkBlock`,
`CocoaBlock`, `NetherWartBlock`, `SweetBerryBushBlock`, `ChorusFlowerBlock`, `GrowingPlantHeadBlock`, all
`randomTick`), NeoForge wraps the growth decision: `if (CommonHooks.canCropGrow(level, pos, state, <vanilla's
condition>)) { <vanilla's growth> ; CommonHooks.fireCropGrowPost(level, pos, state); }`. `canCropGrow` posts
`CropGrowEvent.Pre` and answers `GROW` → true, `DO_NOT_GROW` → false, `DEFAULT` → the vanilla condition. `CropBlock`:
light ≥ 9, `age < max`, then the condition is `random.nextInt((int) (25 / speed) + 1) == 0` (the draw always happens
before the event). Where the hook sits differs per block: **for each, run `javap -c -p` on the class in the patched
server jar** (`javap -c -p -cp /home/umceko/hz-poc/play/install-rs/libraries/net/neoforged/minecraft-server-patched/26.3.0.101/minecraft-server-patched-26.3.0.101.jar net.minecraft.world.level.block.<Class>`, inside `nix-shell -p jdk25`; or search `javap/callers.javap`) and place the native hook at
the same point relative to the vanilla calls around it (which draws happen before it, what the condition is, what the
growth is, where `fireCropGrowPost` sits). Report each block's placement (block, hook position, condition passed) as a
table in the report to the lead, not in code comments.

### horizon-world
`LevelAccess` gets two hooks (defaults keep today's behaviour):
```rust
/// `CommonHooks.canCropGrow`: a mod may force or refuse a grower's growth; `vanilla` is the block's own condition.
fn crop_grow_pre(&mut self, _ticks: &mut Ticks, _pos: BlockPos, _state: BlockStateId, vanilla: bool) -> bool { vanilla }
/// `CommonHooks.fireCropGrowPost`, after a grower grew.
fn crop_grow_post(&mut self, _ticks: &mut Ticks, _pos: BlockPos, _state: BlockStateId) {}
```
Each of the ten native growers calls them at the patched jar's points (example, `crop_random_tick`):
```rust
let draw = self.rand_int((25.0 / speed) as i32 + 1) == 0;
if self.level.crop_grow_pre(self.ticks, pos, state, draw) {
    let next = self.crop_state_for_age(state, age + 1);
    self.set_block(pos, next, 2);
    self.level.crop_grow_post(self.ticks, pos, state);
}
```
With the default hooks every grower behaves exactly as today (the growth harnesses prove it).

### `GameLevel` (`G/game.rs`, java-mods arm in `G/java_cascade.rs`)
`crop_grow_pre`: when `native_authoritative` and `java_listens_now(CropGrowPre)`: `cascade_event(self, ticks,
CropGrowPre, |out| { long pos, int javaState, bool vanilla })` → result `bool grow`; else `vanilla`.
`crop_grow_post`: when listened, `cascade_event` id 26 with `long pos`, `int javaState`. States cross as Java ids through
the bound `StateMap` (`STATES`).

### Java (`ServerEvents` cases 25, 26)
25: `result = CommonHooks.canCropGrow(level, pos, state, vanilla)`. 26: `CommonHooks.fireCropGrowPost(level, pos, state)`.
Debug line (per call, so only under `HORIZON_JAVA_DEBUG`): `Horizon: server event CropGrowEvent.Pre <pos> <in>-><out>`.

`JavaEvent::CropGrowPre`, `CropGrowPost` into `SITED`.

## Tests
- `bridge.rs`: the macro test above; `a_bridge_without_a_cascade_body_throws_inside_a_cascade` (call the generated
  native's dispatch with `CASCADE` set and no body: the error names the native and the event).
- horizon-world (`level/tests.rs` test level, which implements `LevelAccess`): `a_refused_crop_stays` (hook returns
  false: wheat at age 0 under light 15 never grows over 2,000 random ticks at that position), `a_forced_crop_grows`
  (hook returns true: grows on the first tick whatever the draw), and the existing growth tests unchanged with the
  defaults. Expected values from vanilla's rule, not from the port.
- Test mod (`ChecksServerEvents`): `CropGrowEvent.Pre` listener: `DO_NOT_GROW` for wheat, `GROW` for carrots;
  `CropGrowEvent.Post` listener: for potatoes, `level.setBlock(pos.above(2), Blocks.GOLD_BLOCK.defaultBlockState(), 3)`
  (a nested write inside the cascade); `Pre` for beetroots calls a bridged entity method on a view (`discard()` on a
  marker armor stand the check spawned) inside a try and records the exception message.
  `drive`: a lit farmland patch with wheat, carrots, potatoes, beetroots at age 0, `/gamerule randomTickSpeed 300`,
  wait 100 ticks. Checks: `crop_pre_refuses_wheat` (all wheat age 0), `crop_pre_forces_carrots` (all carrots at max age),
  `crop_post_nested_write` (a gold block above each potato that grew), `crop_cascade_refuses_entities` (the recorded
  message contains `isn't served inside a block update (CropGrowEvent.Pre)`).

## Done bar
- `cargo test -p horizon-java-host bridge`, `cargo test -p horizon-world` (growth tests), `cargo test -p horizon-client
  --features java-mods --lib java_cascade`, clippy both ways, fmt; the growth harnesses `tools/parity.py
  --changed-files` names for `tick.rs` (at most two, `verify/parity.sh`) unchanged.
- `bridges` CHECKs pass. M1 `w13` step `crops`: `CropGrowEvent$Pre` and `$Post` counts within tolerance of Forge's.
- `HORIZON_PROFILE`: with a 32×32 wheat farm at `randomTickSpeed` 3, the `level_event` op's time per tick reported.

## Live test (lead, showcase jars, `HORIZON_JAVA_DEBUG=1`)
1. A wheat farm under light, `/gamerule randomTickSpeed 100`: crops grow as before; `CropGrowEvent.Pre` lines appear.
2. Sugar cane, cactus, bamboo, kelp, cocoa: each grows as before.
3. No `isn't served inside a block update` line in a normal session (a line there names a mod that needs more than
   level bridges inside a block update: report it, it is D1's open edge).
