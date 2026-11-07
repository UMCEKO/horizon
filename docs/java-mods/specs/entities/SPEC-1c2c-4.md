# SPEC-1c2c-4: spawn-time riding leaves lib code

Index: `SPEC-1c2c.md`. Size: ~150 lines. Depends on nothing. **No behaviour change.**

## Why
Lib code links riders while it builds mobs, before they are in any level:
- `mobs/finalize.rs` `ride` (1265) and the chicken jockey (1304);
- `mobs/conversion.rs:38–39`, which copies the vehicle;
- `vanillagen/structure_mobs.rs:183/304`, which reads `extra.mount` to build `Passengers`.

Step 6 deletes those fields, and the lib can't reach `Game`. After this step, lib code only *reports* who rides whom at spawn, and the bin links.

## Change
**`finalize.rs`:**
- `Finalizer` gains `pub rides: Vec<(u32, u32)>` (rider, vehicle), in call order.
- `ride(rider, vehicle)` pushes to it. The vanilla "leave the old vehicle" step it does by hand goes too: the bin's link does it.
- The chicken-jockey branch (1304) calls `self.ride_by_id(m.id, chicken)`, which pushes. `FinalizeLevel::ride_chicken` stays for the chicken's own `IsChickenJockey` flag only; rename it `make_chicken_jockey` if it does nothing else.

**The bin's finalize callers** (`spawn_jockeys`, natural spawning, spawners, `/summon` and spawn eggs; grep `Finalizer` / `finalize_spawn` in `game/`) apply `rides` in order with `self.mount_mob(rider, vehicle)` (`passengers.rs:29`), after both mobs are added.
- Vanilla calls `startRiding` in `finalizeSpawn`, before `addFreshEntity`. Native links after the adds, in the same tick and before any tick of either, so nothing can observe the difference.
- Step 6 turns `mount_mob` into `start_riding` with the arguments of each vanilla call. **Record them now:** for each `ride` call, add a one-line comment with the vanilla call (`startRiding(chicken)` = force false, events true; `startRiding(x, true, …)`…). Find them with `python3 tools/rea_vanilla.py method net.minecraft.world.entity.Entity startRiding` callers.

**`conversion.rs`:**
- Delete lines 38–39.
- `convert` returns the old mob's vehicle with the new mob. The bin's conversion handler applies `ConversionType.SINGLE.convert`'s riding lines (vanilla `ConversionType.java`): the first passenger moves to the new mob, and the new mob rides the old vehicle. Use `mount_mob` and the existing unlink, in vanilla's order.
- The test at 120–139 moves to the bin's conversion test.

**`structure_mobs.rs`:** `tree` takes the finalizer's `rides` and uses them in place of `extra.mount`. It finds the root as the mob that is no rider.

## Tests
- `cargo test -p horizon-client --lib finalize structure_mobs conversion` (the existing tests, adapted).
- `python3 tools/parity.py --changed-files <your files> --list`, then run what it lists (expect `mob_ai`, the finalize/spawn harnesses, structure mobs).
- `grep -rnP "extra\.(mount|rider|passenger)\s*=" crates/horizon-client/src/mobs* crates/horizon-client/src/vanillagen` shows only `riders.rs`, `piglin.rs` and `#[cfg(test)]` code.
