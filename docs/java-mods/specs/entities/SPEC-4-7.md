# SPEC-4-7: spawn events

Design: `DESIGN.md` §4 row `FinalizeSpawn`, `PositionCheck`, `SpawnPlacementCheck`, `SpawnClusterSize`. Index: `SPEC-2-8.md`. Size: ~280 lines. Needs phase 2.

## Vanilla, patched (callers from `/home/umceko/hz-poc/agents/entities/p2/callers.sh`)
| Hook | Callers | Result |
|---|---|---|
| `EventHooks.getPotentialSpawns(level, category, pos, list)` | `NaturalSpawner.mobsAt` | the weighted list |
| `EventHooks.checkSpawnPlacements(type, level, reason, pos, random, vanilla)` | `SpawnPlacements.checkSpawnRules` | bool |
| `EventHooks.checkSpawnPosition(mob, level, reason)` | `NaturalSpawner.isValidPositionForMob`, `spawnMobsForChunkGeneration`, `SpawnUtil.trySpawnMob` | bool |
| `EventHooks.checkSpawnPositionSpawner(…)` | `BaseSpawner.serverTick` | bool |
| `EventHooks.getMaxSpawnClusterSize(mob)` | `NaturalSpawner.spawnCategoryForPosition` | int |
| `EventHooks.finalizeMobSpawn…` | `BaseSpawner.serverTick`, `TrialSpawner.spawnMob`; for natural spawns run `callers.sh finalizeMobSpawn finalizeSpawn` and `javap -c` `NaturalSpawner.spawnCategoryForPosition` to find what the patched code calls instead of `mob.finalizeSpawn` | `FinalizeSpawnEvent`: canceled spawn, difficulty, spawn data |

Order of calls inside `spawnCategoryForPosition` matters: the RNG draws around them must stay vanilla's; the hooks don't draw (they post), but a listener may: that's the mod's draw on the level random, which the view shares only if `RandomSource` is bridged. It isn't: a listener drawing from `level.getRandom()` draws from a Java copy. List it as a known gap in the report if the analysis shows a listener calling `getRandom` (`GROUPS` `pure` today).

## Sites
`natural_spawning.rs:522` (`spawnCategoryForPosition` port), `spawn_selection.rs:670` (`mobsAt`), `spawn_rules.rs:318` (`checkSpawnRules`), the spawner and trial spawner ports (grep `BaseSpawner`, `TrialSpawner`), `SpawnUtil.trySpawnMob` port (patrols, wardens, golems). Each: one admitted bit; the candidate mob is **not yet in the level**, so its view is unbacked: Java builds it from the saved form (as `EntityViews.join` does, without `LIVE`), posts, and returns the answer; on finalize, the result's changes (difficulty, data, and what listeners set on the mob: equipment, effects, attributes) come back as the saved form and native rebuilds the mob from it before `level_add`.

## Test mod
`FinalizeSpawnEvent`: cancel for `minecraft:zombie` spawned by a spawner, give pigs from spawners a golden helmet. `MobSpawnEvent.PositionCheck`: deny `minecraft:creeper`. Checks: a zombie spawner next to the player for 400 ticks spawns none; a pig spawner's pigs wear the helmet; natural spawning for 600 ticks at night in a dark flat area spawns no creeper but other monsters.

## Tests to run
`java_mods.py build`, `check --analysis`, `bridges`; `python3 tools/parity.py natural_spawning spawner spawn_rules` unchanged.

## User step
MekanismTools pack: zombies and skeletons spawn with Mekanism armour sometimes at night, as on Forge.
