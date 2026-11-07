# SPEC-11 (W4): M1 for W4's events, and the W4 scenarios

Design: `DESIGN.md` §1.5, D5, Q2. Size: ~200 changed lines (Python ~120, JSON ~80). Depends on: nothing for §1 (tooling);
SPEC-1…6 for §2's expected results.

Read first: `T/mod_parity.py` (`compare_events` at 344, `mismatch`, `TOLERANCE`, `SLACK`, `compare`, `compare_command`, `run`),
`tests/test_mod_parity.py`, `T/parity_scenarios/basic.json`, `T/hz_scenario.py` (step kinds: `press`/`release`, `key`, `tp`,
`command`, `check` with `log`/`no_exceptions`), COVERAGE-SPEC §2 M1.

Code comments: one line at most.

## 1. Frame events in M1 (`T/mod_parity.py`, ~70 lines)

Per the user's HUD rule, Horizon fires per-frame events once per client tick and once per frame only while their result changes,
and only when a mod listens; Forge fires them every frame. Their counts can't match and needn't.
- `FRAME_EVENTS = {'net/neoforged/neoforge/client/event/ViewportEvent$ComputeCameraAngles', '…$ComputeFov', '…$ComputeFogColor',
  '…$RenderFog', 'net/neoforged/neoforge/client/event/CalculatePlayerTurnEvent',
  'net/neoforged/neoforge/client/event/CalculateDetachedCameraDistanceEvent'}` (the names as the comparison stores them; check the
  stored form in a run's JSON first).
- `compare_events(forge, horizon, frame_listened=None)`: for a class in `FRAME_EVENTS`, no `count_mismatches` row; it is `missing`
  only when Horizon has none in the whole run **and** it is in `frame_listened`; otherwise it goes to a new list `frame_unlistened`
  (informational, in the report's own section). Order comparison unchanged (first-seen order of common classes).
- `frame_listened`: the event keys of `INSTALL/analysis/replaces.json` with at least one `changes` entry, intersected with
  `FRAME_EVENTS`. `run` and `compare_command` pass it when the install has that file; without it, `frame_listened = FRAME_EVENTS`
  (strict).
- Report (`replaces`-style markdown the tool writes): a line `Frame events (by presence, HUD rule): …` listing each class as
  `present`, `missing (listened)` or `not fired (no listener)`.
- Tests (`tests/test_mod_parity.py`): Forge 600 `ComputeFov`, Horizon 40, listened → no mismatch, not missing; Horizon 0, listened →
  missing; Horizon 0, not listened → `frame_unlistened`; a non-frame class keeps today's tolerance rule (600 vs 40 → mismatch).

## 2. Scenarios (~80 lines JSON)

### `T/parity_scenarios/w4_motion.json` (M1, Forge vs Horizon; format of `basic.json`)
Steps, each named: `join` (wait 60); `survival`; `platform` (`/fill ~-8 ~-1 ~-8 ~8 ~-1 ~8 minecraft:stone`, a 1-block step at
`~2 ~ ~`); `walk` (`press w` 40, release); `sprint` (`press ctrl`, `press w` 40, release both); `jump` (`key space` ×3 with waits);
`step` (walk into the 1-block step 20); `swim` (`/fill` water 3×3×3 at the player, wait 60, clear it); `fly` (`/gamemode creative`,
double `space`, `press w` 20, release, `/gamemode survival`); `third person` (`key f5`, wait 40, `key f5`, `key f5`); `fall`
(`/tp ~ ~8 ~`, wait 60); `quit`. Mekanism pack only (same jar both sides).

### `T/scenarios/w4_motion.json` (Horizon only, `hz_scenario.py`, for the lead's smoke runs)
The same steps without `quit`, then checks: `no_exceptions`; `{"log": "Horizon: lockstep M"}` (SPEC-1, with Mekanism);
`{"log": "Horizon: fog events heard by mekanism"}` (SPEC-6); `{"near": …}` after `walk` (the player moved: 3.5–4.5 blocks, SPEC-3's
independent value) and after `step` (y + 1 only with the step-assist gear, so here unchanged: native's 0.6 step).

## 3. Expected M1 result once SPEC-1…6 landed (the lead checks the run's report against this)

| Event | Forge | Horizon | Why |
|---|---|---|---|
| `ClientTickEvent.Pre/Post`, `PlayerTickEvent.Pre/Post` (client) | per tick | per tick, order `Pre, PlayerTick.Pre, PlayerTick.Post, Post` | SPEC-1 |
| `MovementInputUpdateEvent` | per tick | per tick | SPEC-3 (every lockstep tick) |
| `LivingJumpEvent`, `LivingFallEvent` (client) | per jump / landing | same | SPEC-3 |
| `ComputeFovModifierEvent` | per tick | per tick | SPEC-4 |
| `ViewportEvent.ComputeFogColor`, `RenderFog` | per frame | present (listened by Mekanism) | SPEC-6, §1 |
| `ComputeCameraAngles`, `ComputeFov`, `CalculatePlayerTurnEvent`, `CalculateDetachedCameraDistanceEvent` | per frame | not fired (no listener in the pack) | §1 `frame_unlistened` |

## Tests
`python3 -m unittest tests.test_mod_parity`; `python3 T/hz_scenario.py run --install … --script T/scenarios/w4_motion.json …`
once (through the gate, headless) after SPEC-6.

## Done bar
Unit tests pass; the Horizon-only scenario passes; the lead's M1 run (`mod_parity.py run --install install-rs --scenario w4_motion`)
shows the table in §3: no `missing`, no `count_mismatches` among W4's classes.

## Live test (lead)
Run §2's M1 scenario on both sides and read the report's W4 rows against §3. User steps: none (no visual change in this spec).
