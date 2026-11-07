# SPEC-1c2c-5: the vehicle rules, one dispatch per vanilla method

Index: `SPEC-1c2c.md`. Size: ~240 lines. Needs step 1.

## Why
Vanilla answers "may it board", "who steers", "where does it get off" and "where does passenger *i* sit" with one virtual method each on the vehicle. Native spreads them over hand checks:
- `boat_has_room`, `player_boat_seat`, `seat_boat_mobs`, `eject_boat_mobs` (`boat_passengers.rs`);
- `riders::mob_controlled`, `mounts.rs` `drives`/`dismount_location`/`strider_dismount_location`;
- `minecarts.rs` `dismount_cart`;
- `riders::dismount_location`.

Steps 6–9 call these rules for any pair, so they become one dispatch now, at today's call sites. These functions **take the passenger facts as arguments**: the list or count. That way they don't care where riding is stored, and they keep working unchanged when the storage moves (steps 6, 7).

## `game/vehicle_rules.rs` (new), `impl Game<'_>`
| Function | Vanilla | Bodies |
|---|---|---|
| `can_add_passenger(v: Target, passengers: &[Ent]) -> bool` | `canAddPassenger` | see the table below |
| `could_accept_passenger(v: Target) -> bool` | `couldAcceptPassenger` | false for `OminousItemSpawner`, `Marker` (`Other` by type id); else true |
| `controlling_passenger(v: Target, passengers: &[Ent]) -> Option<Ent>` | `getControllingPassenger` | every override (`grep -rn "LivingEntity getControllingPassenger()" world/entity`): `Mob` (first passenger if it is a `Mob` and this mob has AI), `AbstractHorse`, `Pig`, `Strider`, `Ravager`, `AbstractNautilus`, `HappyGhast`, `AbstractBoat`; `Entity` default `null`. Move `riders::mob_controlled`'s and `mounts.rs`' rule bodies here; they then call this |
| `dismount_location(v: Target, rider: Target) -> Vec3` | `getDismountLocationForPassenger` | `Entity` default `(x, box.maxY, z)`; `LivingEntity`, `Animal`, `AbstractHorse`, `Strider`, `HappyGhast`, `AbstractBoat`, `AbstractMinecart`: native's ports listed in Why, moved behind this |
| `passenger_point(v: Target, index: usize, count: usize, rider: Target) -> Vec3` | `getPassengerRidingPosition` | step 1's `passenger_attachment` for mobs; `AbstractBoat.getPassengerAttachmentPoint` (seat by index and count, the raft and chest-boat offsets) replacing `player_boat_seat` and `seat_boat_mobs`' offsets; `AbstractMinecart`'s; `Entity` default from the exported list |

**`canAddPassenger`, exactly as vanilla writes it:**
| Vehicle | Body |
|---|---|
| `Entity` (default) | `passengers.isEmpty()` |
| `AbstractBoat` | `size < getMaxPassengers()` (2; chest boats and chest rafts 1) `&& !isEyeInFluid(WATER)` |
| `Camel`, `CamelHusk` | `size <= 2`. This is vanilla's body: three fit. Don't "fix" it |
| `HappyGhast` | `size < 4` |
| `AbstractNautilus` | `!isVehicle()` |
| `Strider` | `!isVehicle() && !isEyeInFluid(LAVA)` |
| `OminousItemSpawner`, `Marker` | `false` |

Before porting, check the list with `grep -rln "boolean canAddPassenger" .local/minecraft/26.3/sources/server`. Eye-in-fluid uses the vehicle's eye height and the fluid query native's motion code has (`is_eye_in_fluid` or equivalent; grep it).

## Wiring (today's sites; storage untouched)
- `boat_has_room(i)` → `can_add_passenger(Target::Boat(id), &current)`, where `current` is the boat's riders as `Ent`s from today's storage (mobs with `extra.mount == id`, players whose vehicle is `id`), in seat order.
- Mob and player boat seats → `passenger_point(boat, index, count, rider)`. The index is the rider's position in that same list.
- Each dismount site → `dismount_location`.
- `riders::mob_controlled` and `mounts.rs`' controller checks → `controlling_passenger`.
- **On the client** (`remote_vehicles.rs` `drives`/`client_moves`), the passenger list is `remote.passengers.passengers_of(v)` (step 3).

**Behaviour that changes toward vanilla, on purpose:**
- a camel takes a third rider by `/ride` or `startRiding`;
- a boat whose eyes are under water takes nobody;
- a strider in lava takes nobody.

## Tests
- `cargo test -p horizon-client --bins vehicle_rules`:
  - `can_add_passenger_matches_each_vanilla_override` (one assertion per table row, counts 0..=4);
  - `boat_seats_by_index` (2 riders: front and back offsets at yaw 0 and 90, the values of `AbstractBoat.getPassengerAttachmentPoint`);
  - `default_dismount_is_the_top_of_the_box`.
- `python3 tools/parity.py mob_ai` and `python3 tools/lan_e2e.py --check boat --check boats --check boat_shoved --check minecarts --check strider --check happy_ghast`.
