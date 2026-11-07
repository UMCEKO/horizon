# SPEC-2-13: one vanilla-ordered loop over the level's entities

Index: `SPEC-2-8.md` (E1, E3). Size: ~300 lines (mostly deletions of loops). Needs 2-5 to 2-12. **Changes behaviour toward vanilla**: the order of entity ticks across kinds, and where riders tick.

## Vanilla (`ServerLevel.tick`, `ServerLevel.java:421–455`)
```java
if (emptyTime < 300) {
  if (dragonFight != null && runs) dragonFight.tick();
  entityTickList.forEach(entity -> {
    if (!entity.isRemoved()) {
      if (!tickRateManager.isEntityFrozen(entity)) {
        entity.checkDespawn();
        if (entity instanceof ServerPlayer || inEntityTickingRange(entity.chunkPosition())) {
          Entity vehicle = entity.getVehicle();
          if (vehicle != null) {
            if (!vehicle.isRemoved() && vehicle.hasPassenger(entity)) return;
            entity.stopRiding();
          }
          guardEntityTick(this::tickNonPassenger, entity);
        }
      }
    }
  });
  tickBlockEntities();
}
entityManager.tick();
```
- `tickNonPassenger(e)` (`:829`): `commonTick()`, `tick()`, then `tickPassenger(e, p)` for each passenger in list order.
- `tickPassenger(v, e)` (`:842`): if `e.isRemoved() || e.getVehicle() != v` → `e.stopRiding()`; else if `e instanceof Player || entityTickList.contains(e)` → `commonTick()`, `rideTick()`, then its own passengers recursively.
- `Entity.rideTick` (`Entity.java:2433`): `setDeltaMovement(ZERO)`, `tick()`, then `getVehicle().positionRider(this)` if still a passenger. `LivingEntity.rideTick` adds `resetFallDistance()`; `Player.rideTick` (`Player.java:435`) first stops riding if it wants to (server side).
- `isEntityFrozen` (`TickRateManager.java:63`): `!runsNormally() && !(e instanceof Player) && e.countPlayerPassengers() <= 0`.
- `checkDespawn`: `Entity`'s is empty; overrides `Mob`, `WitherBoss`, `EnderDragon`, `ShulkerBullet` (peaceful discard). It runs **before** the range test, and a despawned entity **still ticks this turn** if in range: vanilla doesn't test `isRemoved()` again.

## 1. `game/tick_entities.rs` (new)
```rust
pub(super) fn tick_entities(&mut self) -> Result<()>          // the block above
fn tick_non_passenger(&mut self, pass: &mut TickPass, e: Ent) -> Result<()>
fn tick_passenger(&mut self, pass: &mut TickPass, v: Ent, e: Ent) -> Result<()>
fn turn(&mut self, pass: &mut TickPass, e: Ent, riding: bool) -> Result<()>  // commonTick + tick, by kind: turn_mob, turn_item, … turn_player
fn check_despawn(&mut self, pass: &mut TickPass, e: Ent)      // Mob (lib `despawn_check`), Wither, Dragon, ShulkerBullet arms
fn is_frozen(&self, e: Ent) -> bool
```
- `turn(.., riding: true)` is `rideTick`: velocity zero first, the kind's turn, then `position_rider(v, e)` (1c2c-9's per-pair placement) if still a passenger, then `fall_distance = 0` for living kinds. `Player.rideTick`'s "wants to stop riding" (sneak) → `stop_riding` instead, as 1c2c-8 wired it.
- **`commonTick`** (`Entity.java:517`: `invulnerableTime--`, old position and rotation, `tickCount++`) is the first part of each kind's turn. Where a kind's turn does those inside its body today, keep them there but make sure they run before anything else of the turn; the event step 2-16 needs `commonTick` and `tick` as one call it can skip.
- **Despawn before the tick:** move the despawn decision out of the mob's lib tick (`ai.rs` `check_despawn`, called from inside `Mob::tick`) to `check_despawn` here; `Mob::tick` no longer calls it. A discarded entity is removed at once (`level_remove(.., Discarded)`: leave event, unpairing), and if it is in range its turn still runs on the **detached** value: `level_remove` gains a variant `level_detach(kind, id, why) -> Option<NewEntity>` that returns the removed entity; the pass keeps it for that turn and drops it after. Its effects apply as for any turn.
- **Range:** `entity_ticking_area()` per entity; players always pass. The unloaded-chunk branch and the non-ticking despawn branch of `turn_mob` become this structure (delete them from `turn_mob`).
- **Dragon fight:** the dragon-fight part of `tick_end` (`EnderDragonFight.tick` port; find it in `dragon.rs`/`end.rs`) is called first, when `running`.
- `TickPass` is built once (`begin_pass`) before the loop and ended (`end_pass`) after it.

## 2. `tick_level`
- Delete every per-kind loop call that 2-6 to 2-12 kept: `tick_paintings`, `tick_item_frames`, `tick_armor_stands`, `tick_cushions`, `tick_boats`, `tick_carts`, `tick_bolts`, `tick_fishing`, `tick_tridents`, `tick_rockets`, `tick_ominous_item_spawners`, `tick_items`, `tick_creature_entities`' loop, `tick_experience`, `tick_falling`, `tick_tnt`, the projectile, potion, fang, cloud, eye and knot loops, and the player turn loop of 2-12.
- `tick_entities()` runs where `tick_paintings` runs today (after `tick_blocks` and the spawning, raid and thunder calls, which are `ServerChunkCache.tick` and `raids.tick`), and `tick_block_entities` follows the work that stays between them, unchanged.
- `spawn_loaded_entities(FromDisk)` and `restore_root_vehicle` move after the block entities: vanilla's `entityManager.tick()` (loads processed after the loop).
- `sync_riders`/`tick_passengers` (the placement pass) is deleted: placement happens in `rideTick`.

## Tests
- `game_tests`:
  - `kinds_tick_in_the_order_they_joined` (pig, item, armor stand, boat, then a second pig: a test hook on `turn` records `[pig1, item, stand, boat, pig2]`);
  - `a_rider_ticks_right_after_its_vehicle` (`[horse, zombie_rider, next]`), and `a_rider_of_a_rider_ticks_depth_first`;
  - `a_despawned_mob_still_ticks_this_turn_and_leaves_first` (leave hook before its turn hook);
  - `a_frozen_tick_skips_all_but_players_and_their_vehicles`;
  - `an_entity_out_of_simulation_distance_only_checks_despawn`.
- Every entity harness: `python3 tools/parity.py --changed-files <files> --list`, then run them all (expect most of the ~100 for game files). Moved numbers only with the oracle agreeing; list them.
- `python3 tools/lan_e2e.py` riding checks (`boat boats boat_shoved camel cart minecarts happy_ghast horse llama mounts occupied_horse other_horse strider`) and `chest`.
- Timing: the 360-mob scenario, the `tick_entities` span against the base's sum of the removed spans; within ±5%, report both.

## Scenario (parent): `tools/scenarios/entity_order.json`
Floor, `tp`, `near`; summon a pig, drop an item, summon an armor stand and a boat, in that order; `wait 40`; save, quit, relaunch; `wait 40`; `check {"no_exceptions": true}`; then the order test's log line `Horizon: tick order …` (a `HORIZON_DEBUG_TICK_ORDER=1` log of the first 8 turns, added for this check and kept: one line per tick when the variable is set).
