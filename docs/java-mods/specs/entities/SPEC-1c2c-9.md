# SPEC-1c2c-9: any kind rides and is ridden

Index: `SPEC-1c2c.md`. Size: ~220 lines. Needs step 8.

## Why
The table takes any `Ent`, but `tick_passengers` (step 6) places only mob riders, and only mobs zero their motion. Vanilla's `Entity.rideTick` and `positionRider` are the same for every kind: items, orbs, primed TNT, armor stands, frames, paintings, projectiles, boats, minecarts, end crystals, cushions, bolts and `Other`.

No game path creates such pairs yet: steps 10 and 11 add `/summon … Passengers` and `/ride`. This step makes their ticks right first.

## 1. The rider's tick (vanilla `Entity.rideTick`, `Entity.java:2433`)
Vanilla ticks a passenger as follows:
- `setDeltaMovement(ZERO)`;
- `tick()`;
- then `vehicle.positionRider(this)`;
- living riders also `resetFallDistance()` (`LivingEntity.java:3288`).

The rider ticks in its own kind's loop (user decision: phase 2 moves it into the vehicle's turn). So, per kind:
- In each kind's loop, before the entity's own tick: `if self.is_passenger(Ent::Id(id)) { velocity = ZERO }`. Kinds and loops:
  - items `game.rs` ~3188;
  - orbs;
  - `tnt.rs`;
  - `armor_stands.rs`;
  - projectiles `creatures.rs` / `projectiles.rs`;
  - `boats.rs`, `minecarts.rs`;
  - `dragon.rs` crystals;
  - `cushions.rs`;
  - bolts;
  - `Other`.
- The tick itself runs unchanged. Mobs keep their existing passenger handling. Don't change it here; it is a parity-checked path.
- `tick_passengers` then places **every** rider in `riders_in_order()`, whatever its kind:
  - `passenger_point(v, index, count, r)` (step 5; the default is the vehicle type's exported list) minus the rider's exported `vehicle` point (its type's, through step 1's `Attachments`);
  - written with the kind's `setPos`: position and box, previous position untouched, as vanilla's `MoveFunction` `Entity::setPos`;
  - living riders (mobs, stands): `fall_distance = 0`.
- **The vehicle's `positionRider` side effects**, per override (`grep -rln "void positionRider(final Entity passenger, final Entity.MoveFunction" world/entity`):
  - `AbstractBoat` (turns the rider, `clampRotation`): already native for mobs and players; extend it to any rider with a rotation;
  - `AbstractHorse`, `Chicken`, `Minecart`: port them; native has the mob ones in `riders.rs`/`equine.rs`.

**A rider's kind data:** step 1's `Attachments` per type id covers every kind. The box size comes from the type (`width`, `height`), as 1c2d's `KindMoveWorld` reads it.

## 2. Client
`position_riders`' remote branch (step 3) places every kind it tracks the same way: items, objects, projectiles, displays, interactions. The client also runs `rideTick`'s zero velocity on each tracked passenger before its interpolation step.

## 3. Removal
Already done: step 6's `riding_removed` runs in `level_remove` for every kind. Check that each kind's removal goes through `level_remove`: 1b made it the only path, and the site test in `entity_life.rs` holds.

## Tests
- `passengers.rs`:
  - `an_item_on_a_pig_sits_at_the_pigs_point`: link `Id(item)` on `Id(pig)` in the table, tick once, then compare the item's position with `pig.pos + (0, 0.86875, 0)` minus the item's vehicle point. Take both values from `entity-types.json` in the test, skipping without `.local`;
  - `an_armor_stand_in_a_boat_turns_with_it`;
  - `a_tnt_riding_a_cart_keeps_its_fuse` (its own tick ran);
  - `a_rider_has_zero_velocity_before_its_tick`.
- `python3 tools/parity.py entity_move item_entity tnt` (unchanged results).
