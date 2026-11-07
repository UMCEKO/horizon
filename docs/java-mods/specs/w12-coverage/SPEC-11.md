# SPEC-11: riding (Carry On)

Design: `DESIGN.md` §2.6 row 11; W12 `DESIGN.md` §4 T4. Needs SPEC-3 (`Vec3` return, entity args), SPEC-4 (guards), W9 3-1, W9 2-13 and 2-14 (riders tick and are positioned in their vehicle's turn). Size: ~300 changed lines.

Paths as SPEC-2. Read first: vanilla `Entity.java` (`startRiding(Entity, boolean, boolean)`, `positionRider` both overloads, `getDismountLocationForPassenger`, `onPassengerTurned`, `addPassenger`, `rideTick`), `LivingEntity.getDismountLocationForPassenger` (an override: check whether it calls `super`), `Player`/`ServerPlayer` overrides; `G/passengers.rs`, `G/mounts.rs`, `crates/horizon-client/src/mobs/riders.rs`, W9 2-13/2-14's turn and rider code, the riding bridges of W9 1c2c (`java_ride`, `java_dismount` in `G/java_entities.rs`).

Comments in code: one line at most.

## Keys (carryon, all conditional on its config plugin)
| Key | Kind | Native seam (`Game` fn, gate first) |
|---|---|---|
| `Entity#startRiding(Lnet/minecraft/world/entity/Entity;ZZ)Z` | body (`modify_expression_value` on `EntityType.canSerialize()`) | `entity_start_riding(rider, vehicle, force, send_event) -> bool`: every native mount goes through it (player right-click on a horse/boat/minecart/pig, `/ride`, boats picking up mobs, the riding table's sites) |
| `Entity#positionRider(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/entity/Entity$MoveFunction;)V` | edge HEAD | `entity_position_rider(vehicle, rider)`: native's passenger placement each tick (`riders.rs` is lib: the call site in the vehicle's turn is the `Game` side; put the gate there) |
| `Entity#getDismountLocationForPassenger(Lnet/minecraft/world/entity/LivingEntity;)Lnet/minecraft/world/phys/Vec3;` | edge HEAD | `entity_dismount_location(vehicle, rider) -> [f64; 3]` |
| `Entity#onPassengerTurned(Lnet/minecraft/world/entity/Entity;)V` | inject (SPEC-1) | `entity_on_passenger_turned(vehicle, rider)` where vanilla calls it (`Entity.rideTick`/the rotation sync; check) |

- `MoveFunction` (a functional interface) argument: Java passes `Entity::setPos` of the rider, as vanilla's one-argument `positionRider(Entity)` does; the body bridge ignores it and runs native placement. Add `MoveFunction` to SPEC-3's readers as "the rider's `setPos`", nothing on the wire.
- Overriders: vanilla subclasses override `positionRider` and `getDismountLocationForPassenger` (`AbstractHorse`, `Camel`, boats, minecarts, `LivingEntity`…). For each, W12's rule: admitted when its override calls `super` down to `Entity`'s (SPEC-1's `overriders` table says which). Native's per-kind ports must call the gated `Entity` port at the super-call point; `java_routing.py lint --ledger` checks it.
- The `startRiding` body runs in Java: its closure (17 in the report: `addPassenger`, `couldAcceptPassenger`, `isPassenger`, `setPose`, …) uses W9 1c2c's riding bridges; add those `check --analysis` still lists. Carry On's carried entity rides **the player**: a player as vehicle must be accepted by native's riding table (check `riders.rs` handles a player vehicle; if not, that is the gap this spec closes, in the riding table's data, not a special case).

## Tests
- Test mod mixins (`J/testmod/…/mixin/RidingRouteMixin.java`):
  - `positionRider` HEAD, cancellable: for a vehicle tagged `horizon_hold`, `rider.setPos(vehicle.getX(), vehicle.getY() + 3, vehicle.getZ())` and cancel;
  - `getDismountLocationForPassenger` HEAD: for `horizon_hold` vehicles, return `vehicle.position().add(0, 0, 5)`;
  - `startRiding` `@ModifyExpressionValue` on `canSerialize()`: `true` when the rider is tagged `horizon_any`.
- `ChecksEntities`: `rider_held_above`: a pig riding a `horizon_hold` minecart is 3 blocks above it after 5 ticks; `dismount_location_used`: `/ride <pig> dismount` puts it 5 blocks south; `start_riding_body`: a `horizon_any` rider of a type that can't serialize (an `area_effect_cloud`, check `canSerialize` in vanilla) mounts.
- `lan.sh riding` and `python3 tools/parity.py --changed-files <files> --list` (the riding harnesses): without mods nothing moves.
- `cargo test` of the touched modules; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run.

## Done bar
Checks pass; four gates; lint and `--ledger` lint pass.

## For the user
Carry On (showcase install): shift-right-click a pig with empty hands: you carry it in front of you; walk, then shift-right-click the ground: it is placed where you look. Carry a chest with items, place it, open it: the items are there. Same as NeoForge. Try on a dedicated server with a second player watching: they see the pig held in front of you.
