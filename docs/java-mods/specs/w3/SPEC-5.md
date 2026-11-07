# SPEC-5 (W3): NeoForge's custom particle groups drawn through W1's world recording

Design: `DESIGN.md` §2 step 4, D4. Paths: DESIGN's short-hands. Depends on: SPEC-2 (particles live in Java, accessors),
W1's `WorldFrame` (merged in 2fdea19cd). Independent of SPEC-3/4. Size: ~250 lines (Java and testmod; no Rust).
Comments in code: one line at most, only for a non-obvious why (RULES).

## Goal

A group a mod registered with `RegisterParticleGroupsEvent` (Mekanism's `LaserParticleGroup`, AE2's `LightningFXGroup`,
puzzleslib's) draws what its `extractRenderState(frustum, camera, partial)` returns and that state's
`submit(collector, cameraState)` submits, as on a real client. `LevelRenderer.submitFeatures` submits
`particlesRenderState` with the other features; W1's `WorldFrame` stands for that frame, so the groups submit into its
`SubmitCapture` at the `SUBMIT` stage and W1 classifies, meshes and draws the geometry (passes from the pipelines:
additive lightning, translucent beams, lines), with W1's timing (per tick, per frame only while it changes).

## Java

### `B/ParticleRecorder.java` (new in SPEC-3; create it here if SPEC-3 hasn't landed)
```java
/** Vanilla's own groups, which W3 records as quads (SPEC-3) or which only vanilla code fills. */
private static boolean vanilla(ParticleRenderType type)  // SINGLE_QUADS, ITEM_PICKUP, ELDER_GUARDIANS, NO_RENDER
/** Whether a group a mod registered holds particles: the world must be recorded. */
static boolean groupsLive()
/** `ParticlesRenderState.submit` for the mods' groups, in the engine's order, at `LevelRenderer.submitFeatures`' place. */
static void submitGroups(Frustum frustum, Camera camera, float partial, SubmitNodeCollector collector, CameraRenderState cameraState)
```
- `groupsLive()`: over `ParticleEngineAccess.horizon$order()`, any non-vanilla type whose group (`horizon$groups().get(type)`)
  is non-empty and not a `QuadParticleGroup`.
- `submitGroups`: for each such group in order: `var state = group.extractRenderState(frustum, camera, partial);
  state.submit(collector, cameraState); state.clear();` (vanilla's `ParticlesRenderState.reset` calls `clear`). A
  `Throwable` logs `Horizon: a Java mod's particle group failed: <group class>` once per class and skips that group.
- A mod's group that is a `QuadParticleGroup` (made for its own render type with vanilla's quad group) is recorded by
  SPEC-3 instead: SPEC-3's walk takes every `QuadParticleGroup` regardless of type.

### `B/WorldFrame.java`
- `fire(Frame f, Sink sink)` → `fire(Frame f, Sink sink, Runnable particles)`; right after the
  `SubmitCustomGeometryEvent` post and before `PrepareRenderBuffersEvent`, with `stage = SUBMIT`: `particles.run()`.
  The real call passes `() -> ParticleRecorder.submitGroups(f.frustum(), f.camera(), f.state().worldPartialTicks, f.collector(), f.state().cameraRenderState)`.
- `take(...)`: today it returns null when `!needed`. Record when `needed || ParticleRecorder.groupsLive() || groupsWere`,
  then set `groupsWere = ParticleRecorder.groupsLive()`, so the recording after the last group empties clears the
  geometry once. `needed` keeps its meaning (a mod listens to the frame's events); with `needed` false only the groups
  are submitted, as `fire` posts events to no listener.
- The per-frame path (`ClientBridge.worldFrame` → `take(..., true)`) needs no change: Rust asks for frames only while
  `WorldFrame.animating`.

## Testmod

In `T/TestMod.java`: `BEAM = PARTICLE_TYPES.register("beam", () -> new SimpleParticleType(true))` (always shown).
In `T/ClientSpark.java` (SPEC-2's file):
- `static final ParticleRenderType BEAM_GROUP = new ParticleRenderType("HORIZON_TEST_BEAM", "HB");`
- `RegisterParticleGroupsEvent` (mod bus): `event.register(BEAM_GROUP, BeamGroup::new)`.
- `RegisterParticleProvidersEvent`: `event.registerSpecial(TestMod.BEAM.get(), (options, level, x, y, z, xa, ya, za, random) -> new Beam(level, x, y, z))`.
- `Beam extends Particle`: lifetime 40, no physics, `getGroup()` returns `BEAM_GROUP`.
- `BeamGroup extends ParticleGroup<Beam>`: `extractRenderState` returns a state holding each live beam's camera-relative
  position (`lerp(partial, xo, x) − camera.position()`, as `SingleQuadParticle` does); its `submit` calls
  `collector.submitCustomGeometry(new PoseStack(), RenderTypes.debugQuads(), (pose, consumer) -> ...)` with one quad per
  beam: x from −0.05 to 0.05, y from 0 to 2, z 0, colour `0x8000FF00`.
- In the wand's client tick (SPEC-2): every 40 ticks, `level.addParticle(TestMod.BEAM.get(), 10.5, 101, 8.5, 0, 0, 0)`.

## Tests

- **Java unit, `U/WorldFrameOrderCheck.java`:** the existing order check gains the particle step: with a `Sink` that
  records names and a `Runnable` that records `"particle groups"`, the recorded order has `"particle groups"` right after
  `SubmitCustomGeometryEvent` and before `PrepareRenderBuffersEvent`. Expected order written out by hand in the check from
  `LevelRenderer.submitFeatures` (particles after transient blocks, before the outline and gizmos), not read from `fire`.
- `java_mods.py build`, `unit`, `bridges`.
- `cargo test -p horizon-client --features java-mods --lib java_world_renders` (unchanged, must still pass).

## Done bar (the implementer checks)

- Unit checks pass. Testmod world, wand held, `HORIZON_JAVA_DEBUG=1`: the log shows W1's
  `world animated over N frames` lines while a beam lives (every 40 ticks), and one offscreen screenshot at
  (10.5, 101, 8.5) from 4 blocks shows a thin translucent green bar 2 blocks tall.
- A testmod world without the wand: no world recordings (`needed` false and no group live): no `world animated` lines.

## Live test (user: visual)

Play pack:
1. Mekanism Laser (`mekanism:laser`) on a Creative Energy Cube, with a redstone signal: the beam shows from the laser
   to the first block, as on Forge; it disappears when the signal stops.
2. AE2 Charger charging a Certus Quartz Crystal (powered by an Energy Acceptor + Creative Energy Cell): lightning arcs
   flicker over it as on Forge.
3. Look away and back: the beam and arcs stay put in the world. Note for the lead: while one lives the world records per
   frame (D4); compare frame times with and without a laser firing (`HORIZON_PROFILE`, `client_world_frame` mean).
