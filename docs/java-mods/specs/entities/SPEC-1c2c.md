# SPEC-1c2c: vanilla's passenger model, in small steps (index)

Design: `DESIGN.md` §0 decisions 1 and 4. User decision (2026-11-04): a general passenger model, so any rider and vehicle pair works, players included, and `/ride` too.
This file replaces the one-piece 1c2c. The steps are `SPEC-1c2c-1.md` … `SPEC-1c2c-12.md`. Implement them in order; each is one review, about 300 changed lines or less, and leaves the game exactly vanilla for what it covers.

Base: `java-mods` at 454aa7b3a: 1c, 1c2a, `horizon_client::riding` (`riding.rs`: `Ent`, `Riding`, unused) and five 1c2b bridges.

## Decisions every step follows

### D1. Server truth: one table per level, in `Game`
- `Game.riding_table: std::cell::RefCell<horizon_client::riding::Riding>`, added in step 6, swapped per level in `levels.rs` `swap_level` beside `persist`.
- It holds every pair of the level: `Ent::Id(id)` for every non-player (ids are game-wide), `Ent::Guest(conn)` for a server's players, `Ent::Host` for the in-frame host only (deprecated mode; the server thread has no host player).
- A vehicle is never a player (`startRiding`: `!entityToRide.type.canSerialize()` on the server), so `vehicle_of` always yields `Ent::Id`.
- Only `game/passengers.rs` mutates it, through vanilla's methods: `start_riding`, `stop_riding`, `eject_passengers`, `riding_removed` (`Entity.setRemoved`). The one exception is D3's in-turn unlink.
- `RefCell` because a mob's turn (lib code with `&MobWorld`) must read the live table and, for a piglin getting off, unlink at once as vanilla does, while `Game` is lent out. Reads go through methods that return values, never a held borrow.
- `Riding.changed` and `take_changed` are deleted in step 6: vanilla's `ServerEntity.sendChanges` compares the passenger list with `lastPassengers`, which native's tracker (`lan/entity.rs`) already does.

### D2. Client state: a second table, only fed by packets
- `Remote.passengers: Riding` (built with `Riding::client()`), added in step 3. It is `ClientLevel`'s `vehicle`/`passengers` fields.
- Keys: `Ent::Host` is the local player; every other entity is `Ent::Id(network id)`, as the client's vectors key them.
- Written only by `remote_set_passengers` (vanilla `handleSetEntityPassengersPacket`) and by client entity removal (`ClientLevel.removeEntity` → `setRemoved`). Nothing on the client predicts riding.
- The client `addPassenger` appends: the player-first rule is server-only (`!level().isClientSide()`, `Entity.java:2566`). `Riding::client()` sets a flag that `link` checks.
- Server code can't reach it by accident: it lives inside `Remote`, which a server `Game` doesn't have.

### D3. How lib code reads riding
- **Mob AI (server):** `MobWorld.riding: &'a RefCell<Riding>`, filled by `Game::mob_world` from `self.riding_table` (step 6), so it is live at each mob's turn. Methods on `MobWorld` (owned results):
  - `vehicle_id(id) -> Option<u32>`;
  - `first_passenger(id) -> Option<Ent>`;
  - `passengers_of(id) -> Vec<Ent>`;
  - `is_passenger(id)`, `is_vehicle(id)`;
  - `root_vehicle(id) -> Ent`;
  - `stop_riding(id) -> Option<u32>`: the in-turn unlink (`Riding::unlink`), whose dismount game event goes out as a `MobEvent`.
- **`Seen` carries no riding.** `Seen.passenger` and `Seen.vehicle` are deleted in step 6; `Seen.ridden` becomes `player_rider: bool` (= `Mob.rider.is_some()`) in step 6 and is deleted in step 7, once players are in the table.
- **`PlayerView`** keeps `riding`, `passenger` and `vehicle`, filled by the bin from the player's own table entry (it is a per-player value).
- **Renderers (client):** `horizon_client::mobs::anim::Ride { is_passenger: bool, is_ridden: bool }` (vanilla `HumanoidRenderState.isPassenger`, `IllagerRenderState.isRiding`, `*RenderState.isRidden`), passed into `anim::state` and `anim::setup`. The bin computes it per mob per frame with `Game::ride_state(id)` (step 3): on a remote `Game` from `Remote.passengers`, else from the server table.
- **`Mob.rider: Option<equine::Rider>`** stays: it is the controlling player's input (`PlayerInput`), not a riding relation. From step 7, nothing reads its `is_some()` as "is ridden".

### D4. Player reads in the bin
- Step 2 adds `fn vehicle(&self) -> Option<u32>` and `fn set_vehicle(&mut self, v: Option<u32>)`. Every read and write of `Game.riding` goes through them.
- Step 3: on a remote `Game`, `vehicle()` reads `Remote.passengers.vehicle_of(Ent::Host)`, and `set_vehicle` there is a `debug_assert!` failure.
- Step 7: `Game.riding` is deleted. `vehicle()` reads `riding_table.vehicle_of(self.acting_ent())`, where `acting_ent()` is `Guest(lan_acting)` while a guest acts, else `Host`. `set_vehicle` is deleted for `start_riding` / `stop_riding`.

### D5. Attachment points (exporter bump), step 1
`VanillaEntityTypesExporter` exports every type's attachment points, plus the static `EntityDimensions` fields of its class (babies, poses). `OUTPUT_VERSION` 3 → 4. Step 5's `passenger_point` and every later placement are built on them.

## Steps

| # | Step | Lines | Test that proves it |
|---|---|---|---|
| 1 | Attachment points exported (`OUTPUT_VERSION` 4); mob riders placed from them | ~250 | equivalence test with the hand tables; `mob_ai`; `lan_e2e camel horse strider` |
| 2 | `vehicle()` / `set_vehicle()` for every `Game.riding` read and write (no behaviour change) | ~150, mechanical | build; `lan_e2e boat horse` |
| 3 | Client passengers: `Remote.passengers`, the `SetPassengers` port, `anim::Ride` | ~260 | handler tests; `lan_e2e boat camel happy_ghast occupied_horse other_horse` |
| 4 | Spawn-time riding leaves lib code (jockeys, conversions, structures) (no behaviour change) | ~150 | adapted lib tests; harnesses `--changed-files` lists |
| 5 | Vehicle rules as one dispatch each: `can_add_passenger`, `could_accept_passenger`, `controlling_passenger`, `dismount_location`, `passenger_point` (boat seats by index) | ~240 | one test per vanilla override; boat, cart, strider, ghast `lan_e2e` |
| 6 | The server table, for mobs: `start_riding`, `stop_riding`, `riding_removed`, `tick_passengers`, `MobWorld.riding`; mob riding fields deleted | ~320 (≈140 one-line rewrites) | table tests; `mob_ai server_entities`; `lan_e2e boat_shoved strider camel`; scenario `riding.json` |
| 7 | Players into the table: `Game.riding`, `GuestState.vehicle` and the `as_guest` diff deleted; `ServerPlayer` riding effects; logout with a vehicle | ~280 | table tests; every riding `lan_e2e`; `dedicated_server` |
| 8 | Players board and leave by vanilla's rules (`startRiding(this)` at each interact site, sneak dismount) | ~180 | per-rule tests; every riding `lan_e2e` |
| 9 | Any kind rides and is ridden: `rideTick` zero motion, placement for every kind, client too | ~220 | placement tests; `entity_move item_entity tnt` unchanged |
| 10 | Passenger trees of any kind saved and loaded; `/summon … {Passengers:[…]}` | ~250 | round trip; command fixture; scenario `riding.json` |
| 11 | `/ride`, vanilla `RideCommand` | ~120 | command fixture against vanilla's outputs |
| — | `SPEC-1c2d.md` (`Entity.move` for every kind, owners as any entity) | as written | as written |
| 12 | The rest of 1c2b: 12a riding bridges, 12b `move` + `setOwner` bridges, 12c cross-level `teleport` | 3 × ~250 | test-mod checks; `entity_travel.json` |

Order: 1 → … → 11 → SPEC-1c2d → 12a → 12b → 12c.
- Steps 1, 2 and 4 are independent of each other. 3 needs 2; 5 needs 1; 6 needs 3, 4 and 5.
- 1c2d needs 9: its `passenger` is `is_passenger`. Its owner half (§2) needs nothing here.
- 12a needs 11; 12b needs 1c2d; 12c needs 10.

## Rules for the implementer of any step
- Port each behaviour from vanilla's source named in the step (`.local/minecraft/26.3/sources/`), not from these notes, when they differ. Report the difference.
- "The compiler lists the rest" means: delete or rename the field, then fix each error by the step's mapping table. Never keep a field alive for a reader you didn't migrate.
- The in-frame mode is deprecated (CLAUDE.md). Keep it compiling. Its host is `Ent::Host` in the server table, and no step adds anything for it.
- Riders still tick in their own kind's loop (user, 2026-11-04). Phase 2's merged loop moves them into the vehicle's turn. `tick_passengers` only places them.
