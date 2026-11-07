# SPEC-2-12: a player's turn is `ServerPlayer.tick`

Index: `SPEC-2-8.md` (E1, E3). Size: ~250 lines. Needs 2-5 (players are in the list). **Changes behaviour toward vanilla** (when in the tick these happen).

## Vanilla
A `ServerPlayer` is in its level's tick list (`isAlwaysTicking`). Its turn in `ServerLevel.tick`'s loop runs `ServerPlayer.tick()` (`ServerPlayer.java:587–626`), the **light** part:
1. `connection.tickClientLoadTimeout()`;
2. `gameMode.tick()` (destroy progress, the delayed destroy);
3. `wardenSpawnTracker.tick()`;
4. `damageCooldownTime--`;
5. `containerMenu.broadcastChanges()`, then `stillValid` → `closeContainer`;
6. the camera follow (spectating another entity);
7. `CriteriaTriggers.TICK`, then `LEVITATION`;
8. `trackStartFallingPosition`, `trackEnteredOrExitedLavaOnVehicle`, `updatePlayerAttributes`;
9. `advancements.flushDirty(this, true)`;
10. `sendPostEffects` if dirty.

The heavy part, `doTick()` (`Player.tick`: `LivingEntity.tick`, `aiStep`, food, the item cooldowns), runs from `ServerGamePacketListenerImpl.tick()` when the server ticks its connections, **after** the levels (`MinecraftServer.tickChildren`). Native's `tick_guests` (`lan_actions.rs:2621`) and `guest_server_tick` (`2988`) are that part, and stay where they are.

## Where native has the light part today (port ledger, `where ServerPlayer.tick`)
`advancements.rs:896` `tick_marks`, `commands/extra.rs:287` `tick_camera`, `lan_actions.rs:1041` `lan_containers_tick`, parts of `lan_actions.rs:2963` `guest_server_tick`, `lan_camera.rs:215` `tick_guest_cameras`, `lan_crafting.rs:524` `lan_tables_tick`, `stats.rs:198` `tick_stats`, and the warden tracker in `sculk.rs:770` `tick_sculk_entities`. Read each and sort its per-guest work into the list above, or out of it (whatever belongs to `doTick` or to the connection stays).

## Change
- `fn turn_player(&mut self, pass: &mut TickPass, conn: u32)` in `game/lan_actions.rs`: under `as_guest(host, conn, …)`, steps 1–10 in vanilla's order, each the existing native code moved out of the passes above. A step native doesn't have is ported if the ledger says `missing` and a harness or test shows it; otherwise list it in the report with its verdict (no new port without evidence, W17).
- Until 2-13 the turns are called from a loop over the level's players at the place `tick_guests` ran the moved parts, so this step is testable alone. 2-13 puts the call in the merged loop.
- The passes keep only their non-player work (screens of the in-frame host, the client side); delete what moved.
- The in-frame host gets no turn (E6).

## Tests
- `game_tests`:
  - `a_players_container_changes_go_out_in_its_turn` (a chest slot changed by a hopper earlier in the tick reaches the guest's menu at its turn);
  - `the_tick_trigger_fires_once_per_tick_per_player`;
  - `destroy_progress_advances_in_the_players_turn`.
- `python3 tools/parity.py dedicated_server server_player_damage` and what `--changed-files` lists.
- `python3 tools/lan_e2e.py --check chest --check furnace` and the advancement or camera checks `--list` names.
