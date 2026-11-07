# SPEC-1c2c-6: the server table, for mobs

Index: `SPEC-1c2c.md` (D1, D3). Size: ~320 lines, about 140 of them one-line rewrites by the mapping table below. Deleting the fields can't be done in halves, so this is the largest step. Needs steps 3, 4 and 5.

## Why
Mob riding lives in `Extra.mount`/`rider`/`passenger`/`boarding_cooldown` (mobs.rs:782, 952–956):
- `rider` holds **one** rider, rebuilt each tick by `sync_riders`;
- `mount` can name a boat;
- nothing else can be a rider.

After this step, mob riders live in the level's table, with vanilla's `startRiding`/`stopRiding`/`setRemoved`. Players join in step 7. Other rider kinds join in step 9.

## 1. State
- `Game.riding_table: RefCell<Riding>`, one per level: add it to `levels.rs` `Parked` and `swap_level`, beside `persist`.
- In `riding.rs`, delete `changed`, `take_changed` and their test lines. Tracking compares lists (D1).
- Delete `Extra.mount`, `Extra.rider`, `Extra.passenger`, `Extra.boarding_cooldown`, `Seen.passenger` and `Seen.vehicle`. Rename `Seen.ridden` to `player_rider: bool` = `m.rider.is_some()`.
- Delete `riders::link`, `riders::start_riding`, `riders::stop_riding`, `top_passenger` and `BOARDING_COOLDOWN` (`riding.rs` has it).

## 2. `game/passengers.rs`: vanilla's methods
```rust
pub(in crate::game) fn acting_ent(&self) -> Ent                        // Guest(lan_acting) or Host
pub(in crate::game) fn ent_of(&self, t: Target) -> Ent                  // Player → acting_ent(); Guest(c) → Guest(c); else Id
pub(in crate::game) fn target_of(&mut self, e: Ent) -> Option<Target>   // Id → java_target
pub(in crate::game) fn vehicle_of(&self, e: Ent) -> Option<Ent>         // and passengers_of, first_passenger, is_passenger, is_vehicle, root_vehicle: borrow, read, return owned
pub(in crate::game) fn start_riding(&mut self, r: Ent, v: Ent, force: bool, events: bool) -> bool
pub(in crate::game) fn stop_riding(&mut self, r: Ent)
pub(in crate::game) fn eject_passengers(&mut self, v: Ent)
pub(in crate::game) fn riding_removed(&mut self, e: Ent, why: Removal)
pub(super) fn tick_passengers(&mut self)                               // replaces sync_riders, same call site (creatures.rs:1067)
```
**`start_riding`** is `Entity.startRiding` (`Entity.java:2481`), in order:
1. `v == vehicle_of(r)` → false.
2. `!could_accept_passenger(v)` → false.
3. `v.is_player()` → false (`type.canSerialize()`).
4. `rides_through(v, r)` → false.
5. `force || can_ride(r) && can_add_passenger(v, passengers_of(v))` (step 5), else false. `can_ride` is `!shift && boarding_cooldown(r) <= 0`. Shift is the mob's crouching flag; `false` for a kind without one.
6. If `is_passenger(r)`, `stop_riding(r)`. The rider's pose becomes `STANDING` where its kind has a pose field. Then `link(r, v)`.
7. If `events`:
   - game event `minecraft:entity_mount` by `r` at `v`'s position, through native's game-event dispatch (`sculk.rs`);
   - `minecraft:started_riding` for every player in `indirect_passengers(v)`. Players join the table in step 7; the loop is final now.

**The rider kind's overrides** run after a true result: `Mob.startRiding` drops the leash (`Mob.java:1293`).

**The vehicle substitution** runs before step 1: `Piglin.startRiding` (`Piglin.java:~398`) sends a baby piglin onto a hoglin's top passenger, at most 3 deep. Move the native rule (`MAX_PASSENGERS_ON_ONE_HOGLIN`) here.

**`stop_riding(r)`** is `removeVehicle`, plus `LivingEntity.stopRiding` for living riders:
- `unlink` (cooldown 60);
- game event `minecraft:entity_dismount` at the old vehicle's position, unless `r` is being removed for a non-destroying reason (`Entity.java:2541`);
- then, for a living rider, `dismountVehicle(old)` (`LivingEntity.java`, port it): place the rider at `dismount_location(old, r)` (step 5).

**`eject_passengers(v)`**: last to first, `stop_riding`.

**`riding_removed(e, why)`** is `setRemoved` (`Entity.java:4064`):
- `if why.destroys() { stop_riding(e) }` (`Killed` and `Discarded` destroy);
- then `stop_riding(p)` for each passenger of `e`;
- then `forget(e)`.

`level_remove` (`entity_life.rs:169`) calls it for **every** kind, before its existing leave work. That is vanilla's place: `setRemoved` runs before `onRemove`.

**`tick_passengers`:**
- `tick_cooldowns()`, exactly as `sync_riders` counted them;
- then for each `(r, v)` in `riders_in_order()` where `r` is a mob: place it at `passenger_point(v, index, count, r)` minus its vehicle attachment, previous position likewise (as `position_riders` does today);
- then the rider state `position_riders` sets today (velocity zero, `fall_distance` 0, body yaw, `nav.stop()`).

Don't port `sync_riders`' "unlink when not `alive()`" unless vanilla does it. Check `LivingEntity.die`/`tickDeath`: vanilla keeps riders on a dying vehicle until its removal, and the removal hook then frees them.

## 3. Mob AI reads (D3)
- `MobWorld.riding: &'a RefCell<Riding>`, set in `Game::mob_world` (creatures.rs:647) from `&self.riding_table`, plus the three struct literals in lib tests.
- Methods on `MobWorld`: `vehicle_id`, `first_passenger`, `passengers_of`, `is_passenger`, `is_vehicle`, `root_vehicle`, `stop_riding`. If the compiler demands `MobWorld: Sync`, stop and report.

| Old | New |
|---|---|
| `m.extra.mount` | `w.vehicle_id(m.id)` |
| `m.extra.mount.is_some()`, `m.extra.passenger` | `w.is_passenger(m.id)` |
| `m.extra.rider` | `w.first_passenger(m.id).and_then(Ent::id)` |
| `m.extra.rider.is_some()` | `w.is_vehicle(m.id)` |
| `m.rider.is_some() \|\| m.extra.rider.is_some()` | `w.is_vehicle(m.id) \|\| m.rider.is_some()` (players: step 7) |
| `s.vehicle`, `s.passenger` (a `Seen`) | `w.vehicle_id(s.id)`, `w.is_passenger(s.id)` |
| `s.ridden` | `w.is_vehicle(s.id) \|\| w.is_passenger(s.id) \|\| s.player_rider` |
| `ai.rs` `root_vehicle(w, id, mount)` | `w.root_vehicle(id)` |
| piglin `dismount` → `riders::stop_riding(m, at)` | `w.stop_riding(m.id)`, set `m.position = at` as now, push the `entity_dismount` game event as a `MobEvent` |
| a fn with no `w` | add `w: &MobWorld`; every caller is inside a turn |

- `MobEvent::StartRiding` moves from lib `handle_mob_events` (mobs.rs:2038) to the bin's (creatures.rs:1563): `start_riding(Id(rider), Id(vehicle), false, true)` (`Mount.create` calls `startRiding(vehicle)`).
- Lib tests that set `extra.mount` (camel, ai.rs:2700, parrot, enderman, conversion) build a `RefCell<Riding>`, link, and pass it in their `MobWorld`.

## 4. Bin
- `mount_mob(r, v)` → `start_riding` with the arguments step 4's comments recorded. Delete `mount_mob`.
- `grep -rnP "extra\.(mount|rider|passenger)|boarding_cooldown" crates/horizon-client/src/game` → `vehicle_of(Ent::Id(..))` and friends.
- **Boats:** `boat_contacts` pulling a mob in → `start_riding(Id(mob), Id(boat), false, true)` (vanilla `AbstractBoat.push`). `eject_boat_mobs` → `eject_passengers` on break; `riding_removed` also covers removal.
- **Persistence:**
  - `mob_tree_nbt` (persist.rs:820) writes `Passengers` from `passengers_of(Id(id))`, mobs only as today;
  - `take_mob_passengers` (627) and its loader link with `start_riding(child, root, true, false)` (verify against `EntityType.loadEntityRecursive`).
- **Tracking:** `lan_riding` (lan_entities.rs:78) adds mob riders from `riders_in_order()` in list order, mapped to network ids. The guests' and host's lines stay until step 7.
- **Rendering:** `ride_state` (step 3), non-remote branch → the server table.

## Tests
- `riding.rs`: `piglin_rides_the_top_of_a_hoglin_stack` (as a `Game` test if it needs kinds).
- `passengers.rs` tests:
  - `start_riding_refuses_a_loop_a_player_vehicle_and_a_full_vehicle`;
  - `force_skips_can_ride_and_can_add`;
  - `a_killed_vehicle_drops_its_riders_and_an_unloaded_one_keeps_quiet`;
  - `riders_of_riders_sit_on_the_new_position`.
- `cargo test -p horizon-client --lib mobs` (the adapted tests).
- `python3 tools/parity.py mob_ai server_entities`, plus what `--changed-files` lists for `passengers.rs`, `persist.rs`, `boat_passengers.rs`.
- `python3 tools/lan_e2e.py --check boat_shoved --check strider --check camel`.
- `grep -rnP "extra\.(mount|rider|passenger)" crates/` → nothing.

## Scenario (parent, `tools/scenarios/riding.json`, new)
1. Floor, `tp`, `near`.
2. `/summon chicken ~2 ~1 ~ {Passengers:[{id:zombie,IsBaby:1b}]}`. Save, quit, relaunch.
3. `/execute as @e[type=zombie] on vehicle if entity @s[type=chicken]` succeeds.
