# SPEC-5 (W1): the Building Gadgets 2 ghost-preview check

Read first: DESIGN.md §5; `/home/umceko/hz-poc/agents/rca9/SPEC.md` (what BG2 draws, why the old scenarios show nothing,
the user steps); `tools/hz_scenario.py` (`evaluate`, `run_steps`), `tests/test_hz_scenario.py`;
`tools/scenarios/gadget-preview-compare.json`; `B/WorldFrame.java` `blob`; `tools/mod_screens.py` docstring. Paths as SPEC-1.

Depends on: nothing (the rca9 fix is in the tree). Size: about 200 changed lines. Comments in code: one line at most.

## Why a new check

The ghost path is already recorded (`PrepareRenderBuffersEvent` → `VBORenderer.prepareBuffers`, then
`AfterTranslucentBlocks`/`OitTranslucent` → `VBORenderer.drawRender` on the recording device), and rca9's flags are in.
But no run has shown the ghost: the existing scenarios end with a `camera` step, which is a `tp` plus `look` that re-aims
the player, and BG2's preview follows the player's aim. The user check is still open (`agents/finish/STATUS.md` W1).

## Oracle

Independent of the recorder: BG2's **server-side build** of the same gadget state places exactly the blocks the preview
shows. The preview tesselates every ghost block with `cull = false` (rca9, from BG2's `ModelBlockRenderer(true, false, …)`
and vanilla's `ModelBlockRenderer.shouldRenderFace` returning true when not culling), so a stone ghost is the 6 faces of
vanilla's `cube_all` per block, drawn after translucent terrain. After the build, vanilla's `/fill … air replace
minecraft:stone` reports how many stone blocks were placed above the floor (N). Pass: the recording's after-translucent
quads equal 6 × N, with N > 0. If they differ, the lead looks at the screenshot first (BG2 may add its own overlays; then
note what and adjust the expected relation, never the recorder to fit).

## Java (`B/WorldFrame.java`)

In `blob(...)`, when it returns a blob and `HORIZON_JAVA_DEBUG` is set, one line with the quads and lines of the **visible**
pieces by pass, from `recording.meshes()`:
```
Horizon: world recording opaque=<q> translucent=<q> after_translucent=<q> additive=<q> see_through=<q> lines=<l>
```
(`lines` counts both line passes.) Nothing else changes.

## Tool (`tools/hz_scenario.py`)

Two optional keys on `log` and `output` checks:
- `"capture": NAME`: on a match, keep `int` of group 1 of the **last** match in the window under NAME (a dict that
  `run_steps` owns and passes to `evaluate`).
- `"equals": NAME` with optional `"times": K` (default 1): the check passes only if it matches, NAME was captured, both
  numbers are > 0, and `int(group 1 of the last match) * K == captured[NAME]`. `want`/`got` show both numbers.
Document both in the module docstring's check list.

## Scenario `tools/scenarios/w1_bg2_preview.json` (new)

From `gadget-preview-compare.json`'s setup, without its pillar and its final `camera` step:
1. setup as there (creative, `advance_time`/`advance_weather`/`spawn_mobs` false, `forceload add 160 136 180 160`, `wait 60`,
   `kill @e[type=!player]`, `fill 160 70 136 180 85 160 air`, `fill 164 69 144 175 69 155 stone`), `time set noon`,
   `weather clear`.
2. `item replace entity @s hotbar.0 with buildinggadgets2:gadget_building[buildinggadgets2:gadget_mode="buildinggadgets2:surface",buildinggadgets2:gadget_range=5]`, `slot 0`.
3. Select stone as there: `camera [170.5, 75, 139.5, 0, 45]`, `lookat [170.5, 69.95, 146.5]`, `press key.sneak`, `wait 5`,
   `use 2`, `release key.sneak`, `wait 10`, `/data get entity @s SelectedItem.components` with check `output`
   `gadget_blockstate.*stone`.
4. Aim and keep the aim: `tp [170.5, 71, 141.5]`, `lookat [170.5, 69.95, 147.5]`, `wait 60`.
5. Check `{"log": "after_translucent=(\\d+)", "since": "start", "capture": "ghost", "within": 10}`; `screenshot "preview"`.
6. Build: `use 2`, `wait 100`.
7. `/fill 160 70 136 180 80 160 air replace minecraft:stone` and check
   `{"output": "Successfully filled (\\d+) block", "equals": "ghost", "times": 6}`.
8. `{"no_exceptions": true, "since": "start"}`, `quit`.
Run with the default video options (improved transparency off; with it on BG2 draws in `OitTranslucent`, still the
after-translucent pass).

## Tests

- `tests/test_hz_scenario.py`: `capture` keeps the last match's number (log text with two lines `after_translucent=12` then
  `after_translucent=150` captures 150); `equals` with `times: 6` passes for output `Successfully filled 25 blocks` and
  captured 150, fails for 24, fails when nothing was captured, fails when both are 0.
- No Java unit test for the debug line (it only prints); its numbers are the scenario's subject.

## Done bar

`python3 -m unittest tests.test_hz_scenario`; `java_mods.py build`/`unit` (the debug line compiles); one run of the scenario
through the gate with the play pack (`hz_scenario.py run --install /home/umceko/hz-poc/play/install-rs --script tools/scenarios/w1_bg2_preview.json --out OUT --fresh`,
`HORIZON_JAVA_DEBUG=1`, `DISPLAY=:99 WAYLAND_DISPLAY=`). Report the two numbers and attach `preview.png`.

## Live test (lead)

1. The scenario above passes; open `preview.png` once: a translucent 5-wide stone ghost on the floor ahead.
2. Forge reference: `tools/mod_screens.py --install /home/umceko/hz-poc/play/install-rs --scenario tools/scenarios/w1_bg2_preview.json --reference REF --out OUT2`
   (steps 6–7 are harmless there); compare `preview-side.png`: the same ghost footprint, alpha about 0.5, no shimmer.

## User steps (real window), from rca9
1. Grass block selected in a Building Gadget, Horizontal Wall, range 8, daylight; aim at flat ground from about 3 blocks up:
   no dark strip along the far edge, no grass teeth over the tops, about half transparent, no shimmer, at most faint seams.
2. Walk around and under the preview: from below only the bottoms facing you; sides facing away are gone.
3. Compare with a real NeoForge client in the same spot: residual seams should match.
4. Enchanted armour on a zombie still glints and hides behind water as before.
5. Destruction Gadget aimed at a wall: its red destruction overlay shows (user-confirmed before; a regression check).
