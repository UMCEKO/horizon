# SPEC-7 (W13): `BlockEvent.FarmlandTrampleEvent` and `BonemealEvent`

Design: `DESIGN.md` §1.4. Read first: SPEC-1, `crates/horizon-world/src/block_landing.rs` (`BlockLandingWorld`,
`Rule::Farmland` near line 407), `crates/horizon-client/src/landing.rs` (the player's landing world),
`crates/horizon-client/src/mobs/landing.rs` (`MobLanding`), `G/game.rs` (the `FallEvent` match near line 8060 and
`landing_change_cancelled`), `G/creatures.rs` (`MobEvent::SetBlock` near line 2970), `G/growth.rs` `use_bone_meal`,
`G/dispenser.rs` `dispense_bone_meal`, vanilla `FarmlandBlock.fallOn`, `BoneMealItem.useOn`/`applyBonemeal`/`growCrop`.

Depends on SPEC-1. About 300 changed lines. Code comments: one line at most, only for a non-obvious why.

## Vanilla + NeoForge
- `FarmlandBlock.fallOn`: on a server level, `CommonHooks.onFarmlandTrample(level, pos, DIRT default, fallDistance,
  entity)`: `entity.canTrample(...)` (the `nextFloat() < fallDistance - 0.5` draw, living, griefing or player,
  `width * width * height > 0.512`) then posts `FarmlandTrampleEvent(level, pos, state, fallDistance, entity)`; returns
  `!canceled`. True → `turnToBaseBlock` (push entities up, `setBlock(dirt)`). Then `super.fallOn` (fall damage) either way.
  Horizon's `Rule::Farmland` already holds the draw and the conditions; the event goes between them and the push.
- `BoneMealItem.applyBonemeal(stack, level, pos, player)`: `state = level.getBlockState(pos)`, then
  `EventHooks.fireBonemealEvent(player, level, pos, state, BonemealSource.INTERACTION, stack)`; canceled → return
  `event.isSuccessful()` (nothing grows, nothing is consumed by vanilla); else vanilla (valid target, success draw,
  perform, `stack.shrink(1)`). `useOn` on a true result: `ITEM_INTERACT_FINISH` game event and level event 1505; on false,
  the water-plant branch (`growWaterPlant`) runs. A dispenser calls `growCrop(stack, level, pos)` →
  `applyBonemeal(stack, level, pos, null)`: the same event with a null player.

## horizon-world (`block_landing.rs`)
- `BlockLandingWorld` gets
  ```rust
  /// `CommonHooks.onFarmlandTrample` after `canTrample` passed: false keeps the farmland (a mod canceled the trample).
  fn allow_trample(&mut self, _position: BlockPos, _distance: f64) -> Result<bool, Self::Error> {
      Ok(true)
  }
  ```
  `Rule::Farmland`, inside its `width * width * height > 0.512` branch, first: `if !world.allow_trample(position,
  distance)? { return … }` (whatever the rule returns when it doesn't trample), then today's push, write and
  block-change event unchanged (vanilla's `turnToBaseBlock` runs after the event). With the default the results are
  identical to today's.

## Rust
- `java_hooks.rs` pairs:
  - `java_hook_trample(&mut self, pos: [i32; 3], distance: f64, who: TrampleBy) -> bool` (true = canceled).
    `TrampleBy` is `enum { Player, Mob(u32) }`. Player: `player_event` id 7 (the acting player is the entity). Mob:
    `level_event` id 7 with the entity ref. Tail `long pos`, `double distance` (+ for the level form `int mob id`).
    Result empty.
  - `java_hook_bonemeal(&mut self, pos: [i32; 3], stack: &ItemStack, player: bool) -> Option<bool>`: `None` when not
    listened or not canceled; `Some(successful)` when canceled. `player_event` id 8 when `player`, else `level_event`
    id 8. Tail `long pos`, `int hand` (player form only: 0 main, 1 off), `bytes stack` (the stack's tag, as `use` sends
    stacks). Result `bool successful`.
- Player trample: the player's landing world (`crates/horizon-client/src/landing.rs`) can't cross into Java while the
  landing runs, so its `allow_trample` returns true and records the trample (`position`, `distance`) beside the changes
  it reports. Where `game.rs` applies the player's landing (the `FallEvent` handling near line 8060, which already
  reverts a change a native mod cancels through `landing_change_cancelled`): for a recorded trample, when
  `self.java_listens(JavaEvent::FarmlandTrample)` and `self.java_hook_trample(pos, distance, TrampleBy::Player)` is true,
  revert exactly as a native-mod cancel does (the block back to farmland, no updates, no events). The native-mod check
  runs first, as it does today.
- Mob trample: `MobLanding::allow_trample` returns `Ok(true)` and remembers the distance; its `set_block` for that write
  pushes `MobEvent::Trample { pos, state, mob, distance }` (new variant in `horizon-client/src/mobs.rs`'s `MobEvent`)
  instead of `SetBlock` (the mob loop can't cross). `creatures.rs` handles it
  where it handles `MobEvent::SetBlock`: Java check first; on cancel skip (the farmland stays); else exactly what
  `SetBlock` does. Native already applies a mob's landing block change from this event queue, after the mob's turn;
  the event fires at that same moment.
- `growth.rs` `use_bone_meal`: after the target position is known and before `with_level(... use_bone_meal ...)`:
  `if self.java_listens(JavaEvent::Bonemeal) && let Some(successful) = self.java_hook_bonemeal(target, stack, true)`:
  - `successful`: the success side effects of `useOn` exactly as the `used` path runs them today (game event, 1505
    level event, swing, `used` statistic) **except** `consume_held` (vanilla doesn't shrink on a canceled event);
  - not `successful`: run only the water-plant half: replace the single `use_bone_meal` call by its two halves
    (`grow_crop` then `grow_water_plant`, as `dispense_bone_meal` already calls them; check `Behaviours::use_bone_meal`
    is exactly that pair with the face for `grow_water_plant`, and expose the halves if it isn't) and call only
    `grow_water_plant`.
- `dispenser.rs` `dispense_bone_meal`: before `grow_crop`: `java_hook_bonemeal(target, &stack, false)`; canceled →
  `grown = successful` without calling `grow_crop`, and only `grow_water_plant` when not successful (the same split).
- `JavaEvent::FarmlandTrample`, `Bonemeal` into `SITED`.

## Java (`ServerEvents`)
- case 7: `var pos = BlockPos.of(in.readLong()); double distance = in.readDouble();` entity: the acting player (player
  form) or the view for the id (level form). `canceled = NeoForge.EVENT_BUS.post(new BlockEvent.FarmlandTrampleEvent(
  level, pos, level.getBlockState(pos), distance, entity)).isCanceled()`. Post the event, not
  `CommonHooks.onFarmlandTrample`: its `canTrample` draw already happened natively and must not draw again.
- case 8: `var event = EventHooks.fireBonemealEvent(player or null, level, pos, level.getBlockState(pos),
  BonemealSource.INTERACTION, stack)`; `canceled = event.isCanceled()`, result `bool event.isSuccessful()`. The stack:
  the player form passes `player.getItemInHand(hand)` (the view's own stack, so a listener's shrink comes back through
  the use tail); the level form decodes the tag (`ItemStack.CODEC`).
- Debug lines: `Horizon: server event BlockEvent.FarmlandTrampleEvent <pos> canceled=<b>`, `Horizon: server event
  BonemealEvent <pos> canceled=<b> successful=<b>`.

## Tests
- horizon-world (`block_landing.rs` tests): `a_refused_trample_keeps_farmland_and_pushes_nothing`: a test world whose
  `allow_trample` returns false; a player falling 3 blocks onto farmland with a draw below `distance - 0.5` leaves farmland
  and no push; `the_default_allow_trample_writes_dirt` keeps today's result (the existing landing tests stay green: they pin
  vanilla).
- Rust (`growth.rs`): `a_canceled_successful_bonemeal_grows_nothing_and_keeps_the_meal` with a `Game` test fixture as
  the existing growth tests build one, the Java hook replaced by its not-arm and a forced answer (factor the decision
  into `fn bonemeal_after_event(answer: Option<bool>) -> BonemealPlan` and test that pure function: `None` → both halves,
  `Some(true)` → none and no consume, `Some(false)` → water half only).
- Test mod (`ChecksServerEvents`): listeners cancel trampling everywhere in the test area and cancel bone meal on
  `minecraft:oak_sapling` with `setSuccessful(true)`. `drive`: a 3-block fall onto farmland at the test floor
  (`/tp`), bone meal used on a sapling (horizon-ctl `place` with bone meal selected). Checks (`later`):
  `trample_cancel_keeps_farmland` (the block is still `minecraft:farmland`), `bonemeal_cancel_keeps_sapling` (still a
  sapling at stage 0, and the player's bone meal count unchanged).

## Done bar
- `cargo test -p horizon-world block_landing`, `cargo test -p horizon-client --features java-mods --lib growth`, clippy
  both ways, fmt; the landing harness `tools/parity.py --changed-files` names for `block_landing.rs` (through
  `verify/parity.sh`, at most two) unchanged.
- `bridges` CHECKs pass. M1 `w13`: `BlockEvent$FarmlandTrampleEvent` (step `farmland`) and `BonemealEvent` (step
  `bonemeal`) counts equal Forge's where listened (puzzleslib or architectury), else both in `unlistened`.
- Append to `tools/scenarios/w13-events.json`: the two debug-line checks.

## Live test (lead, showcase jars, `HORIZON_JAVA_DEBUG=1`)
1. Survival, jump off a 3-block pillar onto farmland several times: it turns to dirt as without mods; each trample has
   one `FarmlandTrampleEvent ... canceled=false` line.
2. A cow walked off a ledge onto farmland: same, with the mob form.
3. Bone meal a sapling and a wheat crop by hand and from a dispenser: growth as without mods, one `BonemealEvent` line
   each.
