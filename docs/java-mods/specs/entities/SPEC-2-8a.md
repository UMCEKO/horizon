# SPEC-2-8a and 2-8b: the after-pass mob work goes into the mob's turn

Index: `SPEC-2-8.md` (E3). Two reviews of ~150 lines each, from this one file: **2-8a** = the villager rows (reputation, merchant, sense, pickups) and piglins; **2-8b** = every other row. (The file name `SPEC-2-8.md` is the index's.) Needs 2-7; 2-8b needs 2-8a. **Changes behaviour toward vanilla** (when in the tick these happen).

## Why
After the mob loop, `end_pass` runs level-wide passes over all mobs. Each is part of one mob's own `tick()` in vanilla, so it belongs in that mob's turn, after its body, before `MobAfter`.

| Pass (file) | Vanilla, inside the mob's tick | Turn of |
|---|---|---|
| `tick_skeleton_traps` (`passengers.rs:600`) | `SkeletonHorse.customServerAiStep` / `SkeletonTrapGoal` | the trap horse |
| `villager_reputation_tick` (`villagers.rs:385`) | `Villager.setLastHurtByMob` → `ServerLevel.onReputationEvent`, inside the hurt | **not a turn**: the hurt site (move it to where the hit is applied, in `handle_mob_events`' damage arm and the player attack path) |
| `merchant_tick` (`trading.rs:481`) | `Villager.customServerAiStep` | the villager |
| `villager_item_sense` (`villagers.rs:248`) | `NearestItemSensor.doTick` (brain tick, in `customServerAiStep`) | the villager |
| `villager_pickups` (`villagers.rs:309`) | `Mob.aiStep` pickup | the villager |
| `tick_allays` (`allays.rs:14`) | `Allay.aiStep` / `customServerAiStep` (jukebox, item pickup) | the allay |
| `tick_pandas` (`pandas.rs:28`) | `Panda.aiStep` (pick up, `PandaSitGoal`) | the panda |
| `piglin_pickups` (`piglins.rs:189`) | `Piglin` brain `GoToWantedItem`, `Mob.aiStep` pickup | the piglin |
| `sulfur_cube_pickups`, `sulfur_cube_power_priming` (`sulfur_cubes.rs`) | the cube's goal and `primeWhenOnPoweredPosition` in its tick | the cube |
| `tick_breeze_effects` (`breezes.rs:10`) | read the function and its vanilla source: a server effect goes to the breeze's turn; a client-only effect leaves the server path (next row) | the breeze or the client |
| `tick_leads` (`leads.rs:145`) | `Leashable.tickLeash` from `Entity.tick` / `Mob.tick` (the holder resolution and the 100-tick drop) | each leashed entity (mobs here, boats in 2-10) |
| `tick_apparition` (`mob_blocks.rs:79`), `guardian_bubbles` (`creatures.rs:5346`), `tick_sniffer_effects` (`sniffers.rs:48`) | client effects (`ElderGuardian` apparition on the client, `Guardian.aiStep` on a client, `SnifferSoundInstance`) | **not server turns**: they stay where the client ticks (remote and in-frame paths); remove them from the server `end_pass` if a server `Game` runs them today, and say which did |

## Change
- Each row with a turn becomes a per-entity function: `fn allay_turn(&mut self, pass: &mut TickPass, id: u32)`, `fn villager_turn(…)` (sense, then pickups, then merchant, in vanilla's order inside `Villager.customServerAiStep` and `aiStep`: read `Villager.java` and order them), `fn piglin_turn`, `fn panda_turn`, `fn sulfur_cube_turn`, `fn skeleton_trap_turn`, `fn leash_turn`.
- Per-pass caches those passes build (allays' playing jukeboxes, `allays.rs:16`) move into `TickPass`, built lazily on first use.
- `turn_mob` calls `self.species_turn(pass, id, species)` after the body and the 2-7 effects of the body, before `MobAfter`: one `match` on `Species`.
- Inside one mob's tick, a few of these happen before the body's end in vanilla (a sensor before movement). Native runs them after the body, as it does today. List each such row in your report with its vanilla position; the lib split that would place them exactly is not in this step (it would change parity-checked AI code).
- Delete the passes from `end_pass`.

## Tests
- One `game_tests` test per row, each proving the new placement: e.g. `an_allay_picks_up_in_its_turn_before_the_next_mob` (the item is gone at the next mob's `MobBefore`), `a_piglin_takes_gold_in_its_turn`, `a_villagers_sensor_sees_items_dropped_earlier_in_the_tick`, `the_reputation_event_follows_the_hit_at_once`.
- Harnesses: `python3 tools/parity.py --changed-files <files> --list`, then run them (expect `villager`, `piglin`, `allay`, `panda`, `mob_ai`, `leash`). Moved numbers only with the oracle agreeing; name them.
- `python3 tools/lan_e2e.py --check villager_trade` and others `--list` names for these files.
