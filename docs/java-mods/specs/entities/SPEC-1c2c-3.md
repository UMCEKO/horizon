# SPEC-1c2c-3: the client's passengers, only from packets

Index: `SPEC-1c2c.md` (D2, D3 renderers, D4). Size: ~260 lines. Needs step 2.

## Why
`remote_set_passengers` (`game/remote_vehicles.rs:117`) writes server-shaped state on the client:
- each mob's `extra.mount`, `extra.rider` (one rider) and `extra.passenger`;
- `text_displays[..].vehicle`, `interactions[..].vehicle`;
- `Game.riding`, `Remote.first_passenger`.

It also diffs the old and new lists. Vanilla's client keeps `vehicle`/`passengers` on its entities and changes them only in `handleSetEntityPassengersPacket` (`ClientPacketListener.java:1140`) and at `ClientLevel.removeEntity` (→ `Entity.setRemoved`).

## 1. `riding.rs` (lib)
- `Riding` gets a private `client: bool` and `pub fn client() -> Self`. In `link`, the player-first branch runs only when `!self.client` (vanilla `!level().isClientSide()`, `Entity.java:2566`).
- New `pub fn set_removed(&mut self, e: Ent, destroys: bool)`, vanilla `setRemoved` (`Entity.java:4064`):
  - `if destroys { unlink(e) }`;
  - then `unlink` each passenger of `e`, in list order;
  - then `forget(e)`, which drops a non-destroyed entity's own link silently.
  - It returns the unlinked `(rider, vehicle)` pairs, so the server (step 6) can emit dismount events.
- Tests: `client_links_append`, `set_removed_frees_passengers_and_keeps_an_unloaded_riders_link_quiet`.

## 2. The client table
- `Remote.passengers: horizon_client::riding::Riding` = `Riding::client()` in `Remote::new` (remote.rs:588). Delete `Remote.first_passenger`.
- `fn client_ent(&self, network_id: i32) -> Ent` returns `Ent::Host` for `remote.entity_id`, else `Ent::Id(id as u32)`.
- **`remote_set_passengers(vehicle, list)`**, ported line by line:
  1. An unknown vehicle (not in any client vector, and not the local player) → return. Vanilla logs "Received passengers for unknown entity".
  2. `was_mounted = indirect_passengers(v).contains(&Host)`.
  3. `eject_passengers(v)`: unlink from last to first.
  4. For each listed id that exists, `start_riding(p, v, force = true)` on the client: if `p` rides another vehicle, unlink it first, then `link`.
  5. If `p == Host`:
     - the existing `removedPlayerVehicleId` handling;
     - if `!was_mounted` and the vehicle is a boat, set the player's yaw, previous yaw and head yaw to the boat's;
     - the `mount.onboard` / `mount.sit` overlay message, if native shows it elsewhere, moves here;
     - then the player's `velocity = 0` and `fall_distance = 0` as now.
- Keep the happy-ghast dismount lines (`counter = 10`, `ghast_dismount`) where `Host` leaves a happy ghast. Detect it as "Host rode `v` before and not after", which is the vanilla client's `removePassenger` override (`HappyGhast.removePassenger`).
- **Removal:** `remove_entity` (remote.rs:2009) calls `remote.passengers.set_removed(client_ent(id), true)` in place of `remote_set_passengers(id, &[])`. The respawn without `KEEP_ENTITY_DATA` (remote.rs:2278) calls `set_removed(Ent::Host, true)`: the old `LocalPlayer` is removed.

## 3. Client reads
- `vehicle()` (step 2): on a remote `Game`, return `remote.passengers.vehicle_of(Ent::Host).and_then(Ent::id)`. `set_vehicle` starts with `debug_assert!(self.remote.is_none(), "the client learns riding from SetPassengers only")`.
- `Remote.first_passenger` readers → `passengers.first_passenger(Ent::Id(v)) == Some(Ent::Host)`.
- `TextDisplay.vehicle` and `Interaction.vehicle` are deleted. `position_riders`' display and interaction part reads `remote.passengers.vehicle_of(Ent::Id(id))`. In-frame has no displays.
- **Mobs on the client:**
  - `remote_set_passengers` no longer writes `extra.mount`, `extra.rider` or `extra.passenger`;
  - the client positions mob riders in `position_riders` from `remote.passengers.riders_in_order()` when remote;
  - the server's path keeps the fields until step 6.
  - Find the client readers: rename the three fields locally and look only at errors in `remote*.rs`, `tick_remote*` and the draw path (`creatures.rs` ~5600–5700, `mob_blocks.rs`, `dragon.rs` drawing). Move those to the client table, then name the fields back.
- **Renderers (D3):**
  - `anim::Ride { pub is_passenger: bool, pub is_ridden: bool }`;
  - `anim::state(m, alpha, ride)` stores it in `State`; `setup` and `piglin.rs`' pose code read `s.ride` in place of `m.extra.passenger`, `m.extra.rider` and `m.rider.is_some()` (anim.rs 197, 758, 835, 898; piglin.rs 1780).
  - `Game::ride_state(id)`: on remote, from `remote.passengers`; otherwise from the mob's fields. Step 6 switches that to the server table.
- `PlayerView` on a client is built from `vehicle()`, so it is unchanged.

## Tests
- `cargo test -p horizon-client --bins remote_vehicles`: `set_passengers_replaces_the_list_in_packet_order` (`[a, Host]` stays in that order on the client), `a_removed_vehicle_frees_its_riders`, `respawn_without_keep_data_rides_nothing`.
- `python3 tools/lan_e2e.py --check boat`, `--check camel`, `--check happy_ghast`, `--check occupied_horse`, `--check other_horse` (the guest is a client; that is this step's path).

## User steps
On a server-thread world: ride a horse, then a boat with a pig in it, then a happy ghast with a friend.
- You sit and render as before.
- The friend's riding pose shows on your screen.
- Getting off a happy ghast leaves you on top of it.
