# SPEC-2 (W13): the M1 measure for server events (scenario and the listened filter)

Design: `DESIGN.md` §1.2, §4.1. Read first: `RULES.md` (gate, headless, test budget), `tools/mod_parity.py`
(`compare_events`, `flatten`, `ctl_commands`, `run_command`, `compare_command`), `tools/parity_scenarios/basic.json`,
`tests/test_mod_parity.py`, `tools/scenarios/*.json` and `tools/hz_scenario.py` (its `check` kinds: `log`,
`no_exceptions`). Python 3 standard library only; tests in `tests/`.

Depends on SPEC-1 (the census lines). About 250 changed lines. Code comments: one line at most, only for a
non-obvious why.

## Goal
Each later W13 spec's done bar is "this event's count on Horizon matches Forge's for the step that triggers it". That
needs (1) a scenario whose steps trigger every W13 event on both sides, and (2) a comparison that doesn't count an event
nobody listens to as missing (Horizon builds no unlistened event, by design; Forge builds all).

## 1. `tools/parity_scenarios/w13.json` (new)
Steps (names are the step labels; every `command` works on vanilla and Horizon; build the floor before any `tp`, with
`/forceload` first, as `RULES.md` says):
1. `join` (`wait` 60); `creative` (`/gamemode creative`); `area` (`/forceload add ~-16 ~-16 ~16 ~16`, `wait` 40);
   `floor` (`/fill ~-8 ~-1 ~-8 ~8 ~-1 ~8 minecraft:stone`).
2. `tnt`: `/setblock ~4 ~ ~ minecraft:tnt`, `/setblock ~4 ~1 ~ minecraft:redstone_block`, `wait` 100.
   Triggers `ExplosionEvent.Start`, `.Detonate`, `ExplosionKnockbackEvent` (when an entity is in range: the player is 4
   blocks away, inside the blast's 8-block entity box).
3. `piston`: `/setblock ~-4 ~ ~2 minecraft:piston[facing=east]`, `/setblock ~-5 ~ ~2 minecraft:redstone_block`, `wait`
   20, `/setblock ~-5 ~ ~2 minecraft:air`, `wait` 20. One `PistonEvent.Pre/Post` pair each way.
4. `place`: `/give @s minecraft:oak_planks 4`, `slot` 0, `look` [0, 60], `use` 2, `wait` 10; `door`: `/give @s
   minecraft:oak_door`, `slot` 1, `use` 2, `wait` 10 (`EntityMultiPlaceEvent`).
5. `farmland`: `/gamemode survival`, `/setblock ~ ~-1 ~ minecraft:farmland`, `/tp @s ~ ~4 ~`, `wait` 40
   (`FarmlandTrampleEvent`; vanilla's trample draw `nextFloat() < fallDistance - 0.5` passes for a 4-block fall on most
   seeds: the comparison counts what each side did, both run the same world and seed), `/gamemode creative`.
6. `bonemeal`: `/setblock ~2 ~ ~ minecraft:oak_sapling`, `/give @s minecraft:bone_meal 3`, `slot` 2, `look` toward it
   (`look` [-90, 30]), `use` 2, `wait` 10 (`BonemealEvent`).
7. `xp`: `/summon minecraft:experience_orb ~ ~ ~ {Value:5}`, `wait` 40 (`PickupXp`, `XpChange`); `/xp add @s 3 levels`
   (`LevelChange`).
8. `toss`: `slot` 0, `key` `key.drop` (add `'key.drop': 'q'` to `KEYS`), `wait` 10 (`ItemTossEvent`).
9. `sleep`: `/time set 13000`, `/setblock ~-2 ~ ~-2 minecraft:red_bed[facing=north,part=foot]`, `look` at it, `use` 2,
   `wait` 120 (`PlayerSetSpawnEvent`, then `PlayerWakeUpEvent` when the night skips).
10. `nether`: `/execute in minecraft:the_nether run tp @s 0 120 0`, `wait` 60, `/execute in minecraft:overworld run tp
    @s ~ ~ ~`, `wait` 60 (`EntityTravelToDimensionEvent`, `PlayerChangedDimensionEvent`).
11. `death`: `/kill @s`, `wait` 20, `respawn` (new step kind: `ctl_commands` gives `[['respawn']]`; horizon-ctl's own
    respawn command, see `docs/horizon-ctl-commands.md`, and the real client's parity driver presses the death screen's
    button), `wait` 40 (`PlayerEvent.Clone`, `PlayerRespawnEvent`).
12. `rule`: `/gamerule doDaylightCycle false` (`GameRuleChangedEvent`).
13. `crops`: `/fill ~-6 ~-1 ~-6 ~-3 ~-1 ~-3 minecraft:farmland`, `/fill ~-6 ~ ~-6 ~-3 ~ ~-3 minecraft:wheat`,
    `/gamerule randomTickSpeed 300`, `wait` 100, `/gamerule randomTickSpeed 3` (`CropGrowEvent`, `NeighborNotifyEvent`
    on the way).
14. `leave`: `wait` 20.
If the real-client driver (`J/parity/...`, `run_real`) lacks a step kind used here (`respawn`, `key.drop`), add it there
the same way the existing kinds are written. Advancement progress (recipe unlocks, `AdvancementEvent.*`) and chunk ticket
level changes happen on their own during the run.

## 2. The listened filter (`tools/mod_parity.py`)
- `compare_events(forge, horizon, listened=None)`: `listened` is `None` or a dict `{class: bool}` from the census.
  A class in `listened` with `False` is moved from `missing` (and from `count_mismatches`) to a new list `unlistened`
  (`{'event', 'count', 'first_step'}`); classes not in the census are untouched.
- `census(path)`: reads a Horizon game log and returns `{name: listened}` from lines
  `Horizon: server event <binary name> listened by …` (True) and `… not listened` (False). JFR names classes with `$` for
  nested ones, as `getName()` does, so no translation.
- `compare` and `run` take `--listened LOG` (default for `run`: the Horizon side's game log, the file `run_horizon`
  writes the game's output to). `report_markdown` adds a section "Not listened (Horizon builds none)" with the list.
- Exit code and the "missing" count ignore `unlistened`.

## 3. `tools/scenarios/w13-events.json` (new, hz_scenario, Horizon only, `HORIZON_JAVA_DEBUG=1`)
The same steps as §1 as hz_scenario steps, then one `check` per sited event:
`{"check": {"log": "Horizon: server event ExplosionEvent.Start", "since": "tnt"}}` and so on. Start it with only the
census checks (`"log": "Horizon: server event .* listened by"`, `no_exceptions`); each later spec appends its own
check lines to this file (its spec says which).

## Tests (`tests/test_mod_parity.py`)
- `test_unlistened_events_are_not_missing`: forge aggregate with `A` 5 and `B` 3 in step `s`, horizon with neither,
  census `{A: False}` → `missing` is `[B]`, `unlistened` is `[A]`.
- `test_listened_events_still_count`: census `{A: True}`, horizon `A` 2 vs forge 5 → a count mismatch for `A`.
- `test_census_reads_both_line_kinds`: a three-line log (listened, not listened, an unrelated line) → the dict of two.
- `test_flatten_keeps_new_step_kinds`: `respawn` and `key` `key.drop` produce the horizon-ctl lines.

## Done bar
- `python3 -m unittest tests.test_mod_parity` green.
- `python3 tools/mod_parity.py compare --forge F --horizon H --mixins M --listened LOG --out OUT` on the old
  `tools2/explain/run2` folders (no census log there: pass a hand-written two-line census) prints the new section.
- The lead runs `mod_parity.py run --install /home/umceko/hz-poc/play/install-rs --scenario w13 --out
  /home/umceko/hz-poc/agents/w13/m1-base` once (gate, headless) to record the baseline every later spec compares against.

## Live test (lead)
The baseline run above. Expect, before SPECs 3–14: every W13 class Forge fires is either `missing` (listened) or
`unlistened`; none of them `extra`.
