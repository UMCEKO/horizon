# SPEC-11 (W7): NeoForge's custom sky, cloud and weather renderers, recorded and drawn natively

Read first: `DESIGN.md` §0, §1.2 G9, §2.5, D7, wire allocations; `java-mods-host/boot/.../WorldFrame.java` (stages, passes, `frame()`, `record()`, `drawn()`, `Recording`, `noted`/`ticked` animation rule, `RecordingRenderPass`), `RecordingDevice.java`, `ClientBridge.java` (`status`, the tick job), `crates/horizon-client/src/game/java_world_renders.rs` (`java_world_status`, `java_world_frame_request`, `java_world_frame_apply`) and where native draws W1's passes (`gfx`); vanilla `LevelRenderer` (the sky, clouds and weather passes and their arguments), `SkyRenderer.extractRenderState`, `WeatherEffectRenderer` (`extractRenderState`, `tickRainParticles`), `CloudRenderer`; javap of NeoForge's `CustomSkyboxRenderer`, `CustomCloudsRenderer`, `CustomWeatherEffectRenderer`, `CustomEnvironmentEffectsRendererManager` (§0).

Depends on: W1's recorder (present). Independent of the fluid specs. In-game acceptance on a mod biome needs W10 SPEC-4 (mod biomes in native chunks); on AE2's spatial storage needs Q1. Comments in code: one line max, only a non-obvious why.

## Goal

When the camera stands where a `neoforge:custom_skybox`/`custom_clouds`/`custom_weather_effects` attribute names a registered renderer, the mod's renderer runs in the Java client against W1's recording device; native draws what it recorded in the matching place and skips its own sky, clouds or weather exactly when the renderer says it replaced them. No renderer registered (both packs except AE2): nothing runs, no cost.

## Java

### `Environments.java` (new, boot)

- `static boolean active`: set once after the client's boot (where `ClientResources.take` runs): any of `CustomEnvironmentEffectsRendererManager`'s three maps (private static fields `CUSTOM_SKYBOX_RENDERERS`, `CUSTOM_CLOUD_RENDERERS`, `CUSTOM_WEATHER_EFFECT_RENDERERS`, names from javap) is non-empty. Log `Horizon: custom environment renderers: sky N, clouds N, weather N` once.
- `static void tick(Minecraft mc)`, called from the Java client's tick job right after `minecraft.tick()`, only when `active` and a level and player exist:
  1. `Vec3 cam` from `WorldFrame`'s window camera; `sky = getCustomSkyboxRenderer(level, cam)`, `clouds = getCustomCloudsRenderer(level, cam)`, `weather = getCustomWeatherEffectRenderer(level, cam)` (NeoForge's real selection).
  2. `weather != null`: `rainTickReplaced = weather.tickRain(level, ticks, camera)` with the Java client's main camera prepared as `WorldFrame.frame` prepares it, `ticks` the level renderer's tick counter vanilla passes there (read `LevelRenderer.tick`).
  3. Record (below) when any of the three is non-null; otherwise clear the last recording once.
- `static Recording record(float partial)`: inside `WorldFrame`'s recording scope (`recording = true`, `DEVICE.draws.clear()`, `stage` set per call), with `frame(...)`'s `LevelRenderState` and camera:
  - sky (`stage = SKY_STAGE`): a `SkyRenderState` filled as vanilla's `SkyRenderer.extractRenderState` fills it (call it on `Sprites.unallocated(SkyRenderer.class)` if it touches no GPU field, else port its few lines), the model-view rotation only, a `GpuBufferSlice` from `WorldFrame.transform(...)` for vanilla's sky transform; `skyReplaced = sky.renderSky(...)`.
  - clouds: `clouds.prepare(levelState, cam, cloudStatus, cloudColor, cloudHeight, ticks, modelView)` with vanilla's values for this frame (as `LevelRenderer` passes them to `CloudRenderer`), then `renderClouds(levelState, cloudStatus, modelView, new RecordingRenderPass(DEVICE))`, or `renderCloudsOit(...)` when `WorldFrame`'s frame uses OIT (W1's flag); `cloudsReplaced` = its result.
  - weather: `prepare(levelState, weatherState, cam)` with a `WeatherRenderState` filled as vanilla's `WeatherEffectRenderer.extractRenderState` fills it, then `renderSnowAndRain(...)` / `renderSnowAndRainOit(...)`; `weatherReplaced`.
  - Draws on `DEVICE` tagged by stage go through `WorldFrame.drawn(...)` into pieces with pass `SKY = 7`, `CLOUDS = 8`, `WEATHER = 9` and the new piece flag `ANCHORED` (positions relative to the camera, not translated to world space).
- Timing: W1's rule. Per tick in the tick job; per frame only while changing: `WorldFrame.noted(identical, ...)` on these pieces, and the frame request path re-records them while animating (a sky that moves with time will animate; that is vanilla's per-frame cost, accepted by D7).

### Status (`ClientBridge.status`, after the shell block)

`out.writeInt(bits)` (1 sky replaced, 2 clouds replaced, 4 weather replaced, 8 rain tick replaced), `out.writeBoolean(rec != null)`, then the recording as W1 writes its world recording. Bits are 0 when no renderer applies.

### Test mod

`testmod/.../Skies.java`, client side: registers on `RegisterCustomEnvironmentEffectRendererEvent` under `horizon_java_test:test`: a skybox renderer that draws one quad (`RenderPipelines` position-colour pipeline, a 2×2 square at distance 100 straight up, colour 0xFF00FF00) and returns true; a clouds renderer returning true with no draw; a weather renderer returning true for both `renderSnowAndRain` and `tickRain`. A biome `horizon_java_test:test_sky` (data in the test jar) whose attributes set the three NeoForge attributes to `horizon_java_test:test`.

## Native

- `java_world_renders.rs`: parse the environment tail (bits, recording) into `JavaEnvironment { bits, pieces }`; pass 7/8/9 pieces kept with W1's world pieces' storage, flagged anchored.
- `gfx`: draw anchored pieces with the view rotation only (no camera translation), depth test off and no depth write as vanilla's sky/cloud passes (follow each recorded pipeline's own depth state: W1 already carries it per draw):
  - SKY pieces right after the sky clear, **before** native's sky; native's sky (stars, sun, moon, sunrise) is skipped when bit 1;
  - CLOUDS pieces in native's cloud pass place; native clouds skipped when bit 2;
  - WEATHER pieces in native's weather pass place; native rain/snow skipped when bit 4;
  - native's client rain tick (splash particles, rain sounds: find the function `tick` calls for rain particles) skipped when bit 8.

## Tests

- **Java unit `EnvironmentsCheck`**: with the test renderers fetched by `getCustomSkyboxRenderer(Identifier)` (the id overload, no level), `record` produces one SKY piece of 4 vertices at (0, 100, 0) ± 1 in camera space with colour 0xFF00FF00 and bit 1; clouds bit 2 and no CLOUDS piece; weather bits 4 and 8.
- **Rust** `java_world_renders.rs`: a status tail with bits 5 and one anchored SKY piece parses; the frame plan skips native sky and weather but keeps clouds.
- **Scenario** (after W10 SPEC-4): `fillbiome` a 64-block box around the player with `horizon_java_test:test_sky`, wait 40, screenshot: the sky is black (fog colour) with one green square straight up; leave the box: vanilla sky back. Before W10, the lead runs only the unit checks.

## Done bar

- `java_mods.py build`, `unit` (the new check), `check`; `cargo test -p horizon-client --features java-mods --lib java_world`; clippy both ways.
- Boot logs of both packs: `custom environment renderers: sky 1, clouds 1, weather 1` with AE2 in the pack, nothing without; no per-tick work when inactive (profile: `HORIZON_PROFILE` shows no `Environments.tick` samples on `play/install-rs` without AE2... AE2 is in both packs: check the selection returns null outside the spatial biome and record nothing).

## Live test (user)

1. With W10 SPEC-4: the test biome box: green square in a black sky; no clouds; no rain while it rains (`/weather rain`) inside the box, rain outside.
2. With Q1: AE2's spatial storage dimension looks as on Forge (compare screenshots).
