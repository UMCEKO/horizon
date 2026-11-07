# SPEC-9 (W3): acceptance: scenario, NeoForge comparison, user checklist

Design: `DESIGN.md` §2 step 8, §4. Paths: DESIGN's short-hands. Depends on: SPEC-1..8. Size: ~250 lines (a scenario,
one comparison script, no game code).
Comments in code: one line at most, only for a non-obvious why (RULES).

## Goal

One repeatable run that shows W3 end to end on the play pack, and one comparison against a real NeoForge 26.3 client
with the same jars proving that each mod particle type's provider made as many particles in Horizon's Java client as on
NeoForge for the same commands (the oracle). Then the user's visual round.

## Scenario: `tools/scenarios/w3_particles.json` (play pack `install-rs`, `HORIZON_JAVA_DEBUG=1`, `HORIZON_PROFILE=OUT/profile`)

Steps (the floor first, as RULES require):
1. `/gamemode creative`, `wait 20`; `/forceload add -8 -8 8 8`, `wait 20`; `/fill -8 99 -8 8 99 8 minecraft:stone`;
   `/fill -8 100 -8 8 110 8 minecraft:air`; `wait 20`; `tp [0.5, 100, 0.5]`; `check near`.
2. For each of the 13 types of SPEC-1's list that need no options (`mekanism:jetpack_flame`, `mekanism:jetpack_smoke`,
   `mekanism:radiation`, `mekanism:scuba_bubble`, `ae2:energy_fx`, `ae2:lightning_fx`, `ae2:matter_cannon_fx`,
   `ae2:vibrant_fx`): `/particle <type> 2 101 0 0.2 0.2 0.2 0 10`, `wait 2`.
   And `/particle ae2:crafting{item:{id:"minecraft:diamond"}} 2 101 0 0 0 0 0 1`.
   (Types whose options need fields, `mekanism:laser`, `ae2:lightning_arc_fx` and BG2's two, are exercised by their
   machines in the user round.)
   A type of that list whose codec turns out to need fields answers `particle.invalidOptions` on both NeoForge and
   Horizon: take it out of the list and say so in the run's notes (the javap signatures don't show codecs' fields).
3. `wait 10`; `screenshot w3-burst` looking at (2, 101, 0). Machines are in the user round, not here.
4. Checks:
   - `{"log": "particle types are mods', ticked in Java", "since": "start"}`;
   - `{"log": "Java particles: \\d+ live", "since": "start"}` (SPEC-2's line; `HORIZON_JAVA_DEBUG`);
   - `{"no_exceptions": true}`;
   - the absence of `reached native, which can't draw it`, `a Java particle's extract failed`,
     `a Java particle's quad has no sprite`, `isn't in the atlas`, `particle recording didn't read`: one
     `{"log": "<text>", "present": false, "since": "start"}` each (`hz_scenario.py`'s documented form).
Run: `python3 tools/hz_scenario.py run --install /home/umceko/hz-poc/play/install-rs --script tools/scenarios/w3_particles.json --out OUT`
with `HORIZON_GAME_GATE` and `HORIZON_GFX_ENV` set as RULES say. Report `particles.record` and `client_tick` means from
`tools/profile_report.py OUT/profile`.

## NeoForge comparison: `/home/umceko/hz-poc/agents/w3/cmp/` (scratch, not committed)

Built on `/home/umceko/hz-poc/agents/bootcmp/` (`real.py` launches the real NeoForge client with JFR; `hz.sh` launches
Horizon through `live.py` with JFR options; `agg.py` aggregates `jdk.MethodTrace`). Copy, don't edit, those scripts.
- JFR filter on both: `jdk.MethodTrace#filter=` each Java-owned type's provider `createParticle` method (the provider
  classes: log them in SPEC-1's boot line as `name=<provider class>` under `HORIZON_JAVA_DEBUG`, or list them with
  javap: `mekanism.client.particle.JetpackFlameParticle$Factory`, `...JetpackSmokeParticle$Factory`,
  `...RadiationParticle$Factory`, `...ScubaBubbleParticle$Factory`, `appeng.client.render.effects.EnergyFx$Factory`,
  `...LightningFX$Factory`, `...MatterCannonFX$Factory`, `...VibrantFX$Factory`, `...CraftingParticle$Factory`), plus
  `net.minecraft.client.particle.ParticleEngine::add`.
- Same world (`saves/J` of bootcmp's reference), same position, the same commands as step 2, run on the real client by
  its probe mod (`bootcmp/probe`) and on Horizon by `horizon-ctl command`.
- Real clients open a window: run `real.py` with `DISPLAY=:99 WAYLAND_DISPLAY=` (RULES "Headless"), through the gate.
- **Pass:** for every provider, the number of `createParticle` calls is equal (10 per command with count 10, 1 for the
  crafting one), and Horizon's `ParticleEngine::add` count is at least the sum of the mod providers' (sub-particles may
  add more on both). Write `cmp/report.txt` with both columns.

## M1 and M2 for W3

- M1: the only particle events are boot-time (`RegisterParticleProvidersEvent`, `RegisterParticleGroupsEvent`); check
  both appear with the same listener counts in bootcmp's event aggregation for both clients. No gameplay event to add.
- M2: no mod mixin targets vanilla particle code in either pack (`finish/rep-*`); state it in the run's notes with the
  report's date.

## Done bar (the implementer checks)

- The scenario passes; screenshots taken (not judged by the implementer).
- `cmp/report.txt` shows equal provider counts for every listed provider.

## User round (visual; the lead sends these steps)

Play pack (Mekanism, AE2, BG2, Jade, RS), survival unless noted; compare with the same steps on NeoForge if in doubt:
1. AE2 Vibration Chamber burning coal; AE2 Charger charging Certus Quartz (Energy Acceptor + Creative Energy Cell);
   AE2 Matter Cannon shot; an AE2 Molecular Assembler crafting (pattern + interface): their particles and arcs as on Forge.
2. Mekanism: an Energized Smelter running; a Laser with a redstone signal (the beam); a jetpack in use (W4 for flight);
   a Scuba Tank and mask under water (bubbles); radiation near a leaking Nuclear Waste barrel if available.
3. Building Gadgets 2: copy a small build with the Copy-Paste Gadget and paste it; destroy with the Destruction Gadget:
   item and fluid "flow" particles fly to and from the blocks as on Forge.
4. Turn the camera fast around all of them: billboards always face the camera, nothing steps at 20 Hz.
5. AE2 cable with a terminal: mining crumbs and the break burst use the cable's textures (SPEC-7). Mekanism
   bounding-block machine broken by its top: the machine's particles.
6. Options → Particles: Minimal, Decreased, All: mods' particles thin out like vanilla's.
7. Dedicated server with two clients: client B sees A's Matter Cannon shot and AE2 machine effects within 32 blocks.
