# SPEC-1c2c-12: the rest of 1c2b (riding bridges, `move`, `setOwner`, cross-level `teleport`)

Index: `SPEC-1c2c.md`. Three parts, one review each, ~250 lines each. The bodies are `SPEC-1c2b.md`'s sections named below; this file only fixes what changed and the order.

Already done at 454aa7b3a: §1.1 malus, §1.2 base damage, §1.4 `shootFromRotation`, §1.5 `releaseUsingItem`, §1.6 `actuallyHurt`.

`SPEC-1c2b.md`'s common rules apply to every part: the backed guard, an absent entity answering as removed, `after(game, id, GROUPS)`, and `Flat.raw(Entity)` returning 0 for `null`.

## 12a: riding bridges (`SPEC-1c2b.md` §3). Needs step 11.
- Delete `java_ride` / `java_dismount` (`java_entities.rs:887`, `898`).
- `entity_start_riding`, `entity_stop_riding`, `living_stop_riding` call `start_riding(ent_of(java_target(id)), ent_of(java_target(v)), force, events)` and `stop_riding`.
  - Mapping `Target` → `Ent` is step 6's `ent_of`.
  - A `null` vehicle (raw 0) → `false` (vanilla NPEs; Java mods never pass null here, and a bridge must not crash the server thread).
  - Refresh `RIDING | POSITION` on both sides.
- **Player entries** (`if "net/minecraft/server/level/ServerPlayer"`) on `Entity.startRiding(Entity, ZZ)Z`, `stopRiding()V` and `removeVehicle()V`:
  - first check with `javap -p` of the patched jar which of `ServerPlayer`, `Player`, `LivingEntity` declare each, and bridge each declaration;
  - the receiver is a network id; map it with the inverse of `network_entity_id` to `Guest(conn)` (or `Host` in-frame);
  - then call the same functions, then `java_refresh_player()`.
  - Step 7's `ServerPlayer` effects run inside `start_riding`/`stop_riding`, so the bridge adds none.
- The `riding` view group (1c §Wire: `int vehicle`, `int n` × passenger) is written from the server table, as network ids.
- Tests: `SPEC-1c2b.md`'s `ride_boat_and_back` and `player_rides_boat`; the old `SPEC-1c2c.md`'s `item_rides_stand` (`item.startRiding(stand, true, true)` → true, `getVehicle() == stand`, and in `later` the item is at the stand's passenger point). Then `python3 tools/java_mods.py check --analysis …` → `0 problems`.

## 12b: `Entity.move` and `Projectile.setOwner` bridges (`SPEC-1c2b.md` §1.7, §1.3). Needs SPEC-1c2d.
- §1.7 unchanged. Every kind's arm exists after 1c2d: TNT, mobs, carts, items, frames and paintings through their own ports; the rest through `KindMoveWorld`.
- `EntityMoveWorld::passenger` is `self.is_passenger(Ent::Id(id))` (step 6).
- §1.3 is SPEC-1c2d §2.3's `EntUuid` entry, unchanged.
- Tests: `move_writes_through`, `projectile_owner_writes_through` (`SPEC-1c2b.md` Test mod).

## 12c: cross-level `Entity.teleport` (`SPEC-1c2b.md` §2). Needs step 10.
`horizon-core` `teleport.rs` (`calculate_absolute`) and `game/entity_travel.rs` `teleport_entity`, as §2. Changes from §2's text:
- **Riding calls:**
  - `stop_riding(ent)` when `!as_passenger`;
  - `passengers_of(ent)` for the passenger list;
  - `eject_passengers(ent)` for "eject them";
  - `start_riding(p, new, true, false)` for the re-mount.
- **The single-entity tag** is the kind's own tag *without* `Passengers` (`entity_tree_nbt` minus that key): vanilla `teleportCrossDimension` moves passengers one by one, not as a tree.
- **`level_remove(.., ChangedDimension)`:** step 6's `riding_removed` then frees the passengers without a dismount event (non-destroying reason) and keeps the entity's own link quiet. That is vanilla's `setRemoved(CHANGED_DIMENSION)`. The passengers were already ejected in vanilla's order before it, so it finds none.
- `/tp` of a non-player into another level calls `teleport_entity`.
- Tests: §2's `cargo test -p horizon-core teleport`; `game_tests`' `a_mob_teleported_across_levels_leaves_with_changed_dimension_and_joins_there`, `passengers_travel_first_and_remount`; test-mod `teleport_across_levels`, `teleport_runs_mod_callback`; scenario `tools/scenarios/entity_travel.json` and the user's AE2 spatial IO steps, as in `SPEC-1c2b.md`.
