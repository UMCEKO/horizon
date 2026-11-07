# SPEC-1c2c-11: `/ride` for any entity, vanilla `RideCommand`

Index: `SPEC-1c2c.md`. Size: ~120 lines. Needs step 10.

## Why
`cmd_ride` (`commands/world_cmds.rs:1160`, 87 lines) handles players and mobs riding mobs only, with its own branches. Vanilla's `RideCommand` takes any entity pair.

## Change: `cmd_ride`, ported from `server/commands/RideCommand.java`
**`mount(target, vehicle)`**, in vanilla's order:
1. `vehicle_of(target)` is set → `commands.ride.already_riding` (target, current vehicle).
2. The vehicle is a player → `commands.ride.mount.failure.cant_ride_players`.
3. `target` or any of its indirect passengers is `vehicle` (`getSelfAndPassengers`) → `commands.ride.mount.failure.loop`.
4. A different level → `commands.ride.mount.failure.wrong_dimension`. Entities of one command run share the level native is in, so this only fires for a selector reaching another level, if native allows that.
5. `!start_riding(ent_of(target), ent_of(vehicle), true, true)` → `commands.ride.mount.failure.generic`.
6. `commands.ride.mount.success`, return 1.

**`dismount(target)`:**
- no vehicle → `commands.ride.not_riding`;
- else `stop_riding(ent_of(target))` (a living target is placed by `dismountVehicle`, step 6);
- then `commands.ride.dismount.success`, return 1.

Delete the mob-only branches and the direct `set_vehicle`/`mount_mob` calls they made (world_cmds.rs:1238 and around).

## Tests
Command fixture (`game/command_harness.rs`), compared with vanilla's outputs:
- `/ride @e[type=armor_stand,limit=1] mount @e[type=boat,limit=1]` → success; then `… dismount` → success.
- `/ride @e[type=item,limit=1] mount @e[type=pig,limit=1]` → success.
- `/ride @s mount @e[type=pig,limit=1]` while riding → `commands.ride.already_riding`.
- `/ride @e[type=pig,limit=1] mount @s` → `cant_ride_players`.
- `/ride @e[type=boat,limit=1] mount @e[type=armor_stand,limit=1]` while the stand rides the boat → `loop`.

Also `python3 tools/parity.py --changed-files … --list` for `world_cmds.rs`; run the command harness it names.

## User steps
1. Drop an item, then `/ride @e[type=item,limit=1,sort=nearest] mount @e[type=pig,limit=1,sort=nearest]`.
   - Expected: the item sits on the pig's back and follows it.
2. `/ride @s dismount`.
   - Expected: you land beside the vehicle where vanilla puts you.
