# SPEC-1c2c-8: players board and leave by vanilla's rules

Index: `SPEC-1c2c.md`. Size: ~180 lines. Needs step 7.

## Why
After step 7, each player boarding site calls `start_riding(acting, v, true, false)` behind its own checks (room, cooldown, crouching), and emits its own mount event and trigger. Vanilla's interact methods call `player.startRiding(this)` = `(false, true)`, and the rules live in the vehicle (step 5) and in `startRiding` itself (step 6).

## Sites (from step 7's list; vanilla call in brackets)
| Site | Vanilla |
|---|---|
| `mounts.rs:80`, `235` (horses, pig, strider, camel) | `AbstractHorse.doPlayerRide`, `Pig.mobInteract`, `Strider.mobInteract`, `Camel`… → `player.startRiding(this)` |
| `boats.rs:860` | `AbstractBoat.interact` → `player.startRiding(this)`, after its own `isSecondaryUseActive` / `outOfControlTicks < 60` tests (they stay: they are the boat's) |
| `minecarts.rs:1103` | `Minecart.interact` → `player.startRiding(this)` |
| `llamas.rs:138`, `ghasts.rs:80`, `nautili.rs:132` | `doPlayerRide` / `mobInteract` of each |
| `cushions.rs:339` | the cushion's interact → `startRiding` (read its class for the arguments) |

Each site becomes:
- `let ok = self.start_riding(self.acting_ent(), Ent::Id(v), false, true);`
- then the site's kind-specific work, only when `ok` and only what vanilla does after the call in that method: rider yaw, `MountState` reset, sounds the client side starts.
- **Delete** from the site whatever duplicates `canRide` (sneaking, cooldown), `canAddPassenger` (room), `couldAcceptPassenger`, the loop test, or the `entity_mount` game event and `started_riding` trigger it emitted itself. They now happen once, in `start_riding`.
- What a site checks *before* calling `startRiding` in vanilla stays: tame, saddle, baby, `isSecondaryUseActive`.

**Getting off:**
- `Player.rideTick`'s `wantsToStopRiding()` (sneak) → `stop_riding(acting)` then `setShiftKeyDown(false)` (vanilla order).
- The places that handled the sneak per kind (`mounts.rs` `dismount_mount`, `minecarts.rs` `dismount_cart`, the boat's) keep only kind work that vanilla does in the vehicle's own `removePassenger` override (`HappyGhast.removePassenger`, `AbstractHorse`?). Check with `grep -rln "void removePassenger" world/entity`.

## Tests
- `passengers.rs`:
  - `a_sneaking_player_cannot_board`;
  - `a_player_cannot_reboard_for_0_ticks_but_a_mob_waits_60` (`Player.removeVehicle` zeroes it);
  - `a_full_happy_ghast_refuses_the_fifth`;
  - `a_camel_takes_three` (vanilla `<= 2`).
- `python3 tools/lan_e2e.py`, every riding check (step 7's list).
- `python3 tools/parity.py dedicated_server`.

## User steps
1. Sneak and right-click a saddled horse: nothing happens.
2. Get off a horse and right-click it again at once: you mount (players have no cooldown).
3. Put a friend and a villager in one boat, then try a third: it refuses.
