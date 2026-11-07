# SPEC-1c2c-2: one accessor for the player's vehicle

Index: `SPEC-1c2c.md` (D4). Size: ~150 lines, mechanical. Depends on nothing. **No behaviour change.**

## Why
`Game.riding: Option<u32>` (game.rs:2215) has ~110 reads and ~25 writes over 35 files, and it means three things:
- the local player's vehicle on a client (fed by `SetPassengers`);
- the acting guest's vehicle on a server (swapped in by `as_guest`, `lan_actions.rs:445`/`702`);
- the host's vehicle in-frame.

Steps 3 and 7 change where each of those lives. With one accessor, those steps touch two functions instead of 135 sites.

## Change (game.rs, beside the field)
```rust
/// `Entity.getVehicle` of the player in place.
pub(super) fn vehicle(&self) -> Option<u32> { self.riding }
/// Only `startRiding`/`stopRiding` ports call this (steps 3 and 7 narrow it further).
pub(super) fn set_vehicle(&mut self, v: Option<u32>) { self.riding = v; }
```
- Every read `self.riding` → `self.vehicle()`. Every write (`self.riding = X`, `self.riding.take()`) → `set_vehicle`. For a `take()`, read `vehicle()` first, then `set_vehicle(None)`.
- **Leave the two `mem::replace`s in `lan_actions.rs` (445, 702) as they are.** They are the guest swap, which step 7 deletes.
- `grep -rn "\.riding\b" crates/horizon-client/src/game*` must then show only the field, the two accessors and those two lines.
- Rename the field to `riding_raw` to let the compiler find every site, then name it back to `riding`. Don't touch `riding_cart_sounds`, `riding_sound`, or guest `g.vehicle`.

## Tests
- `cargo build --profile fast -p horizon-client`; clippy `-D warnings` on horizon-client.
- `cargo test -p horizon-client --bins` for the modules you touched (they compile; nothing should change).
- `python3 tools/lan_e2e.py --check boat` and `--check horse`.
