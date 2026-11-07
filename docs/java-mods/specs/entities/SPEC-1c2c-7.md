# SPEC-1c2c-7: players into the table

Index: `SPEC-1c2c.md` (D1, D4). Size: ~280 lines. Needs step 6.

## Why
After step 6, three stores still say what a player rides:
- `Game.riding` (the host in-frame, and the acting guest during `as_guest`);
- `GuestState.vehicle` (`lan/guest.rs:109`), swapped into `Game.riding` and diffed back after the turn (`lan_actions.rs:445`, `702–738`, `812–829`). Decision 4 forbids writing back from a diff;
- `Mob.rider.is_some()` read as "a player rides it".

After this step, the table is the only one, and `ServerPlayer`'s riding effects run inside `start_riding`/`stop_riding`.

This step **keeps each boarding site's own checks**: it only changes where the result is stored. Step 8 replaces those checks with vanilla's.

## 1. Deletions
- `Game.riding`, `Game::set_vehicle`, `GuestState.vehicle`, `Seen.player_rider`, the `|| m.rider.is_some()` terms step 6 left. `Mob.rider` stays as the controlling player's input (D3).
- **Keep** `GuestState.vehicle_good`. It is `ServerGamePacketListenerImpl.lastGoodX/Y/Z`, connection state, not riding. Clear it where vanilla does: `handleMoveVehicle` resets it, and `stop_riding` of that guest clears it.
- **Keep** `saved_vehicle` (`saveParentVehicle`'s UUID).

## 2. The accessor
`vehicle()` (step 2): on remote, the client table (step 3); otherwise `self.vehicle_of(self.acting_ent()).and_then(Ent::id)`.

## 3. Writes, site by site (`grep -rn "set_vehicle" crates/horizon-client/src/game*`)
- `set_vehicle(Some(v))` → `self.start_riding(self.acting_ent(), Ent::Id(v), true, false)`. `force` and no events, because the site has already run its own checks and effects. Step 8 changes each to vanilla's call.
- `set_vehicle(None)` → `self.stop_riding(self.acting_ent())`. A site that places the player after clearing the field drops that placement: `stop_riding` does it (`LivingEntity.dismountVehicle` → `dismount_location`, step 5).
- **The `as_guest` swap goes:**
  - delete `riding` from the saved state (lan_actions.rs:173, 445, 541, 604, 702);
  - delete the diff at 734–738 and 812–829. What it sent moves into §4.
  - `Entity.teleportTo` taking a rider off (812) is `stop_riding` at the teleport site (vanilla `ServerPlayer.teleportTo` → `stopRiding` when `isPassenger`).
- `g.vehicle` readers (`lan_vehicles.rs`, `lan_actions.rs`, `lan.rs`, `server_travel.rs`, `mounts.rs`, `boat_passengers.rs`, `minecarts.rs`, `cushions.rs`, `dedicated.rs`, `lan/hub.rs`, `lan/entity.rs`) → `self.vehicle_of(Ent::Guest(g.conn))`. Lib code in `lan/` that can't see `Game` gets the vehicle as a parameter from its bin caller.
- **Spectator** (`game_mode.rs:185`): vanilla `ServerPlayer.setGameMode` → `stopRiding()` when becoming a spectator. The other `set_vehicle(None)` sites (`screens.rs:413`, `game.rs:10911`) follow their vanilla source; name it in a comment if unclear.

## 4. `ServerPlayer` effects, inside the table functions, for `Ent::Guest(c)`
**`start_riding`, after a true result** (`ServerPlayer.java:2175`):
- `position_rider(v, r)`;
- a teleport to the player with `Relative.ROTATION` (the hub's existing teleport send);
- the vehicle's active effects if it is living (`sendActiveEffects`);
- `SetPassengers(v)` to that player at once.

**`stop_riding`, after `removeVehicle`** (`ServerPlayer.java:2191`):
- `RemoveMobEffect` for each effect of a living old vehicle;
- `SetPassengers(old)` to that player;
- then `Player.removeVehicle`'s `boardingCooldown = 0` for any player.

For `Host` (in-frame), only the cooldown applies.

## 5. Leaving and joining
- **Logout** (`PlayerList.remove`): if the guest is a passenger and its root vehicle `exactly_one_player_passenger`, the root and its indirect passengers get `level_remove(.., Removal::UnloadedWithPlayer)` after the player's save writes `RootVehicle`. Otherwise `stop_riding`. Drop the `#[allow(dead_code)]` on `UnloadedWithPlayer`.
- **Join:** the `RootVehicle` restore spawns the saved tree and links the player with `start_riding(Guest(c), root, true, false)` (verify against `ServerPlayer.loadAndSpawnParentVehicle`). In-frame, `restore_root_vehicle` (persist.rs:798) does the same with `Host`.
- **A guest's body removed after death:** `riding_removed(Guest(c), why)`. The test at `lan/guest.rs:1156` moves to the bin and asserts on the table.
- **Changing dimension** (`server_travel.rs`): as vanilla `ServerPlayer.teleport` across levels. Port the riding lines it has (it stops riding unless the transition is `asPassenger`).

## 6. Tracking
`lan_riding` becomes a projection of the table only: every vehicle's list, in order, mapped to network ids (`Host` → the host's entity id, `Guest(c)` → its `entity_id`, `Id(i)` → `i`). Delete its guests and host lines.

## Tests
- `passengers.rs`:
  - `a_guest_boarding_gets_set_passengers_and_its_effects`;
  - `a_player_leaving_has_no_boarding_cooldown`;
  - `logout_removes_a_vehicle_with_only_that_player`;
  - `a_player_and_a_mob_in_a_boat_keep_the_player_first`.
- `grep -rn "\.vehicle\b" crates/horizon-client/src/lan crates/horizon-client/src/game/lan*` → only `vehicle_good` and packets.
- `python3 tools/parity.py dedicated_server server_entities server_player_damage`.
- `python3 tools/lan_e2e.py`, every riding check: `boat boats boat_shoved camel cart minecarts happy_ghast horse llama mounts occupied_horse other_horse strider`.

## User steps (LAN, two players)
1. Both get in one boat; the guest logs out and back in.
   - Expected: the boat stays, and the guest is back in its seat.
2. Alone in a boat, log out.
   - Expected: the boat leaves with you and comes back with you (vanilla `UNLOADED_WITH_PLAYER`).
