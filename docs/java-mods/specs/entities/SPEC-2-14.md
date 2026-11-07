# SPEC-2-14: riding special cases go, now that riders tick in their vehicle's turn

Index: `SPEC-2-8.md` (E3; user decision 2026-11-04: passengers tick inside their vehicle's turn). Size: ~200 lines, mostly deletions. Needs 2-13, which already runs `tickPassenger`/`rideTick` for every pair.

## Why
Before the merged loop, native compensated for riders ticking in their own kind's pass:
- `tick_boats` and `tick_carts` skip "the ridden one, which ticks with its rider" (`boats.rs:544`, `minecarts.rs:824`);
- `position_rider` (`mounts.rs:399`) seats the player after its mount moved, from `tick_mobs`;
- `game.rs` `tick_mobs` calls it after `tick_creatures`;
- 1c2c-9 zeroes a rider's velocity in each kind's loop "before the entity's own tick";
- `Mob.rider` (the controlling player's input) is applied in the vehicle's lib tick.

With 2-13, the vehicle's turn ticks the vehicle, then each passenger in order (`rideTick`: zero motion, tick, `positionRider`). Each of the above is now either a duplicate or in the wrong place.

## Change
| Native | Becomes |
|---|---|
| boat/cart "skip the ridden one" | deleted: every vehicle ticks in its own turn. For a vehicle a player controls, vanilla's server tick still runs; its movement comes from the client (`ServerGamePacketListenerImpl.handleMoveVehicle`), and `AbstractBoat.tick`/`AbstractMinecart.tick` check `isLocalInstanceAuthoritative()` / `isClientAuthoritative()`: port those checks where native doesn't have them, and list them |
| `position_rider` (`mounts.rs:399`) and its call in `tick_mobs` | deleted: the player's `rideTick` (`turn(.., riding: true)` for `Ent::Guest`) places it |
| 1c2c-9's per-kind "zero velocity before the tick if passenger" | deleted: `rideTick` does it once |
| any `tick_passengers` remnant, `sync_riders` | deleted (2-13 removed the call; remove the functions) |
| in-frame host riding paths | untouched (E6) |

Check with `grep -rn "is_passenger\|vehicle_of\|ridden" crates/horizon-client/src/game/{boats,minecarts,mounts,creatures}.rs` that no other per-kind riding special case remains; list each one you keep with its vanilla source.

## Tests
- `passengers.rs`:
  - `a_boat_with_a_player_ticks_in_its_own_turn`;
  - `a_rider_is_placed_once_per_tick` (a counter on `position_rider`);
  - `a_mob_rider_has_zero_velocity_before_its_tick` (1c2c-9's test, now through `rideTick`).
- `python3 tools/lan_e2e.py`, every riding check (2-13's list).
- `python3 tools/parity.py --changed-files … --list` for the touched files (expect `boat`, `minecart`, `mob_ai`, `entity_move`).
- Scenario `tools/scenarios/riding.json` (from 1c2c-6 and -10): unchanged result.

## User steps
1. Ride a horse, a boat with a pig in it, and a minecart; each follows you smoothly, with no jitter at the seat.
2. A friend (LAN) rides a camel behind you: both seats render in place.
