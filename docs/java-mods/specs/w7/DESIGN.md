# W7: mod fluids, sky, weather and dimension effects (design, revision 1)

Status: design only, nothing implemented. Binding: `RULES.md`, `COVERAGE-SPEC.md` principles 0–5, `w12/DESIGN.md` (routing), W1's `WorldFrame` recorder.
Base: `java-mods` at cc1b066b6. Evidence: javap on `play/install-rs` and `w12/install` jars (classes extracted to a temp folder, since deleted), `w12/replaces*/replaces.json`, `tools/java_routing/deferred.tsv`, vanilla 26.3 sources, Horizon's code.
Live run: `live/` (one attempt, failed for environment reasons, see §1.4).

## 0. The 26.3 surface (corrects COVERAGE-SPEC W7's premise)

COVERAGE-SPEC names `DimensionSpecialEffects` and fluid "textures, tint, fog" in `IClientFluidTypeExtensions`. Neither exists in 26.3:

| What COVERAGE-SPEC assumed | What 26.3 / NeoForge 26.3.0.101 has (javap, vanilla source) |
|---|---|
| `DimensionSpecialEffects` | Gone from vanilla. A dimension's look is data: `DimensionType.skybox` (`NONE`/`OVERWORLD`/`END`), `cardinal_light`, `ambient_light`, `has_fixed_time`, `attributes` (`EnvironmentAttributeMap`), `timelines` (`DimensionType.java`) |
| Custom sky/cloud/weather renderers | NeoForge `CustomSkyboxRenderer.renderSky`, `CustomCloudsRenderer.prepare/renderClouds/renderCloudsOit`, `CustomWeatherEffectRenderer.prepare/renderSnowAndRain/renderSnowAndRainOit/tickRain`, registered by `RegisterCustomEnvironmentEffectRendererEvent` (mod bus) under an `Identifier`, **selected per position** by the environment attributes `neoforge:custom_skybox`, `neoforge:custom_clouds`, `neoforge:custom_weather_effects` (`NeoForgeEnvironmentAttributes`, `CustomEnvironmentEffectsRendererManager.get*Renderer(Level, Vec3)`). Each `render*` returns whether vanilla's own layer is replaced |
| Fluid textures and tint in `IClientFluidTypeExtensions` | Vanilla `FluidModel(layer, stillMaterial, flowingMaterial, overlayMaterial, tintSource)` baked by `ModelManager` into `FluidStateModelSet`; mods add theirs with `RegisterFluidModelsEvent.register(FluidModel.Unbaked, fluids...)`; tint is a `BlockTintSource`, NeoForge's `FluidTintSource` (`color(FluidState)`, `colorInWorld(FluidState, BlockState, BlockAndTintGetter, BlockPos)`, `FluidTintSources.constant/water/of`) |
| `IClientFluidTypeExtensions` | Only `getRenderOverlayTexture(Minecraft)`, `extractOverlay(...)` (the screen overlay with the eye in the fluid), `modifyFogColor(...)`, `modifyFogRender(...)` |
| Fluid behaviour | Vanilla `FlowingFluid` + NeoForge `BaseFlowingFluid` (properties `tickRate`, `levelDecreasePerBlock`, `slopeFindDistance`, `explosionResistance`) + `FluidType` (motion, swim, drown, extinguish, hydrate, light, sounds...) |

## 1. The gap, with evidence

### 1.1 Native knows two fluids

- **G1. No mod fluid exists natively.** `horizon-world/src/fluid.rs:12` `enum FluidKind { Empty, FlowingWater, Water, FlowingLava, Lava }`. `FluidState::from_block` (`fluid.rs:197`) and `StateFacts.fluid` (`horizon-items/src/blocks.rs:409`) derive a state's fluid from its block **name** (`minecraft:water`, `minecraft:lava`, five water plants, `waterlogged=true`). So:
  - a mod `LiquidBlock` (`mekanism:brine`, `biomesoplenty:blood`) holds no fluid in Horizon;
  - Mekanism's fluid-loggable blocks (`IStateFluidLoggable`, property `fluid_logged` of `FluidLogType` {`empty`,`water`,`lava`}, javap) hold no water or lava when logged.
- **G2. A mod fluid never flows.** A mod `LiquidBlock.onPlace` runs in Java (owned block) and schedules its fluid tick through the bridge `level_schedule_fluid_tick` → `java_schedule_fluid` (`game/java_bridges/level_api.rs:35`), which returns silently when `FluidKind::from_name` doesn't know the name. The tick is dropped; no log.
- **G3. Nothing describes mod fluids to native.** `BootFacts` snapshots `BuiltInRegistries.FLUID` only for ownership (`BootFacts.java:30`); `FactsWriter` exports no fluid data (no `fluid` in `facts-shared`). The carry gives a mod liquid block lava's gameplay rows as its template (`carry.rs:1144`, test `a_fluid_block_with_an_item_but_a_template_without_one_declares_the_item_alone`).
- **G4. Mod fluids are invisible.** The mesher draws only water and lava (`horizon-client-render/src/mesher/fluid.rs:235` water test, `:316-321` sprites `water_still`/`lava_still`...); a `LiquidBlock`'s render shape is `INVISIBLE`, so its block model draws nothing either.
- **G5. Buckets.** Mod bucket items (`BucketItem` instances) already run in Java: `ItemCode.ownCode` counts `BucketItem.use` as code beyond `Item`/`BlockItem` (`ItemCode.java:59-71`). Their placed source then hits G2. An empty vanilla bucket on a mod fluid is native (`horizon-world/src/level/buckets.rs:41-48`: water/lava only) and does nothing.
- **G6. The camera in a mod fluid.** Native's screen overlay is fixed to `misc/underwater` (`horizon-client-render/src/screen_effects.rs:186`). Mekanism (`ClientRegistrationUtil$2`) and Biomes O' Plenty (`FluidClientHandler$1`, `$2`) override `getRenderOverlayTexture`, `modifyFogColor` and `modifyFogRender` (javap).
- **G7. Entities in a mod fluid** move as in air: native fluid motion is by tag (`motion/fluids.rs:117` `is_tagged(kind, FluidTagId)`), with no `FluidType` (motion scale, swim, push, drown, extinguish, fall distance).

### 1.2 Dimensions, sky, weather

- **G8. Native has three dimensions.** `enum Dim { Overworld, Nether, End }` (`game.rs:3332`, `from_id` maps any other id to `Overworld`), 265 `Dim::` uses in 52 files; the sky is `self.dim == Dim::Overworld` (`game.rs:16888`, `render_tick.rs:133`); the environment loads `data/minecraft/dimension_type/{dim}.json` and `timeline/day.json` from the vanilla root (`environment.rs:152-160`); `hasFixedTime` is answered for vanilla's three (`level_api.rs:52`). A mod dimension has no level, so nothing of its look can show. W10 deferred mod dimension levels (`w10/DESIGN.md` §8).
- **G9. AE2's spatial storage** is the only mod dimension and the only environment-renderer user in both packs: `data/ae2/dimension_type/spatial_storage.json` (fixed time, no skylight, ambient 1.0), `data/ae2/worldgen/biome/spatial_storage.json` sets `neoforge:custom_skybox|custom_clouds|custom_weather_effects = ae2:spatial_storage`; `AppEngClient` registers `SpatialStorageSkyRenderer`, `SpatialStorageCloudsRenderer`, `SpatialStorageWeatherEffectsRenderer`. Nothing in Horizon reads these attributes or runs these renderers.
- **G10. Environment-attribute mixins.** lithostitched patches the attribute system on both sides (`EnvironmentAttributeSystem#addDynamicLayers` TAIL and INVOKE, `ValueSampler#computeValue*` `modify_expression_value` + `wrap_operation`, `EnvironmentAttributeProbe#tick` HEAD/TAIL: biome timelines and structure attributes). W12 routed them `java`/body and parked them in `deferred.tsv` as seams (`environment.rs`, `biome.rs`). Native samples sky, fog and cloud colours itself, so these patches have no effect (M2 gap).

### 1.3 Who uses what (showcase `w12/install` ⊇ play pack `install-rs`)

| Mod | Fluids in the world | `RegisterFluidModelsEvent` | Client extension overrides | Other |
|---|---|---|---|---|
| Mekanism | brine, lithium, sulfuric_acid, hydrogen_chloride, ethene, heavy_water, hydrogen... (`BaseFlowingFluid$Source/$Flowing`, vanilla `LiquidBlock`, `BucketItem`); `MekanismFluidType` overrides `isVaporizedOnPlacement` | `ClientRegistrationUtil.registerFluidModels` | overlay texture, fog colour, fog render | `fluid_logged` blocks; `IBlockExtension#shouldDisplayFluidOverlay` override (also Generators); `ViewportEvent.RenderFog/ComputeFogColor` (W4) |
| Mekanism Generators | its fluids (`GeneratorsFluids`) | `GeneratorsClientRegistration` | | |
| Biomes O' Plenty | blood, liquid_null: own `FlowingFluid` subclasses overriding 13 methods (`canBeReplacedWith`, `getTickDelay`, `getDropOff`, `getSlopeFindDistance`, `canConvertToSource`, `beforeDestroyingBlock`, `animateTick`, `getDripParticle`...); `FluidType` subclass overrides `getBlockPathType`; a `FluidInteractionRegistry` interaction | `FluidClientHandler` | overlay texture, fog colour, fog render | |
| Sophisticated Core | xp_still/xp_flowing + bucket | `ClientEventHandler` | `getStillTexture`/`getFlowingTexture` (not in 26.3's interface: dead code) | |
| Traveler's Backpack | potion/milk-like fluids for its tanks | `ModClientEventHandler` (`PotionFluidTintSource`) | | |
| AE2 | none | | | dimension + three environment renderers (G9) |
| Jade | | | | `FogRenderer#setupFog` TAIL mixin (W4) |
| lithostitched | | | | G10 |

GUI fluid drawing (JEI `FluidHelper`, Jade `ClientProxy`, RS `FluidStackFluidRenderer`, AE2 `FluidBlitter`, Mekanism `MekanismRenderer`, BG2 `RenderFluidBlock`) reads `FluidStateModelSet` in the Java client, whose reload is real; it draws through recorded screens and HUD already. Nothing to do there.

### 1.4 The live run

One offscreen boot (`live/run.sh`, `live/cmds.txt`: place `mekanism:brine` and water, use a brine bucket, `execute in ae2:spatial_storage`, a screenshot) was gated and started once. The game exited before the world: `winit ... Failed to load one of xlib's shared libraries` (`live/ctl.log`): with `DISPLAY=:99` and `WAYLAND_DISPLAY` cleared, winit took X11, and `live.sh` puts no libX11 on the library path. Not retried (RULES: environment failures are reported). **For the lead:** `live.sh` needs `libX11.out`/`libXcursor`/`libXrandr`/`libXi` on `LD_LIBRARY_PATH` when agents follow the Xvfb rule. Everything in §1 is static; each spec's first step re-proves its own part live.

## 2. Approach

### 2.1 Fluids reach native as content (D2)

A mod fluid becomes content like a mod block: the boot exports it, the carry declares it, `mod_content` puts it in the root, native loads it once.

```
BootFacts → facts.json   "fluids": {id: class, type, source, flowing, bucket, states{amount,falling → legacy block}, code{}, replaceable, fluid_type{}}
                         per block state: "fluid": {id, amount, falling}
ClientResources.baked → baked-models.json   "fluids": {id: layer, still, flowing, overlay, tint, overlay_texture, fog_code}, "fluid_overlay": {block: [states]}
carry → data/<mod>/horizon/fluid/<name>.json   + per-state "fluid" in data/<mod>/horizon/block/<name>.json
mod_content → root: registries.json "minecraft:fluid" entries, horizon/fluids.json (the fluid table + mod states' fluids)
native → horizon_world::fluid::FluidTable (installed once) → FluidKind ids 5.. → everything that reads a state's fluid
```

### 2.2 One owner per fluid behaviour (D3, D4)

| Behaviour | Vanilla water/lava | A mod fluid |
|---|---|---|
| Scheduled fluid tick (`FlowingFluid.tick`) | native port (`level/fluids.rs`) | Java: block event kind 5 → `ServerLevel.tickFluid`'s body on the view (as mod blocks' scheduled ticks, kind 2) |
| Random fluid tick | native (lava) | Java: block event kind 6 when `isRandomlyTicking` |
| `LiquidBlock` `onPlace`/`neighborChanged`/`updateShape` | native | Java (the mod's `LiquidBlock` is an owned block: kinds 0, 1, 4 already); native's `should_spread_liquid`/`liquid_block_changed` step aside |
| Native water or lava spreading into a cell holding a mod fluid | native asks `target.canBeReplacedWith(...)`: from the mod fluid's boot table `replaceable` | |
| Native water or lava reaching a mod `LiquidBlockContainer` (Mekanism's fluid-loggable cables, glow panels) | `canPlaceLiquid` from the boot table `can_place_liquid`; the placement itself (`placeLiquid`) runs in Java at the drain (block event kind 7) | |
| A mod fluid spreading into water | | Java's tick reads water through the view |
| Bucket filled from a mod fluid | an empty vanilla bucket aimed at a mod fluid source runs the real `BucketItem.use` in Java (as a mod's bucket already does) | |
| Fluid state of a block state, light, collision, `MOTION_BLOCKING` | native, from the table | native, from the table |
| Entity motion in the fluid | native | native port of NeoForge's `FluidType` motion, proven against a real NeoForge trace (SPEC-9) |

### 2.3 Rendering

The native mesher draws any fluid from the table's model: still/flowing/overlay sprites (in the block atlas, which already holds the mods' sprites), layer, and tint (constant per fluid state, biome water, or none). The side-overlay rule is vanilla's class test for vanilla blocks and an exported per-state bit for mod blocks that override `shouldDisplayFluidOverlay`.

### 2.4 The camera in a fluid

The screen overlay takes the fluid type's `getRenderOverlayTexture` (exported). Fog stays W4's: W4 runs the real `FogRenderer.setupFog` in Java when a detected mod hook exists; W7 adds "the eye's fluid type's client extension overrides `modifyFogColor`/`modifyFogRender`" to that predicate (SPEC-8).

### 2.5 Dimension effects

- **SPEC-10** makes the render side data-driven: the window's level carries its dimension type (skybox, ambient light, fixed time, cardinal light, attributes, timelines) from the world's registries (W10 SPEC-2's export, plus `timeline`), not from `Dim`. Vanilla output is unchanged (tests compare). A mod dimension's look then follows from data once mod dimension levels exist (open question Q1).
- **SPEC-11** runs NeoForge's custom renderers in the Java client on W1's recording device: per tick, the Java client asks the real `CustomEnvironmentEffectsRendererManager` which renderers apply at the camera, runs `renderSky`/`prepare`+`renderClouds`/`prepare`+`renderSnowAndRain` into the recording, and `tickRain` at the client tick. Native draws the recordings in three new camera-anchored passes and skips each of its own layers a renderer replaced. Tested with the test mod on a vanilla biome, so it lands before mod dimensions do.
- **SPEC-12** serves the client's environment attribute values from Java's real `EnvironmentAttributeProbe` per tick while a mod patches the attribute system (W12 routes), and native lerps them per frame exactly as the probe's `get` does.

## 3. Decisions (for the user to confirm)

- **D1. Premise corrected (§0).** W7 targets 26.3's surface: fluid models, tint sources, the client extension's overlay and fog, dimension-type data, NeoForge's three environment renderers. No `DimensionSpecialEffects` work.
- **D2. Fluids as content declarations** (`data/<mod>/horizon/fluid/*.json`, per-state `fluid`), not a java-mods-only side table. Reason: blocks and items already take this road (one cache, one fingerprint, LAN guests and the dedicated server get the same tables), and native content mods gain fluids too, which upstream's docs list as missing (`9cffbd192`: "The world itself still has only vanilla's fluids"). Upstream-bound prose for these commits must not name NeoForge.
- **D3. A mod fluid's ticks run in Java**, as mod blocks' scheduled and random ticks already do (`GameBridge.blockEvents`). Native's `FlowingFluid` port stays water/lava's. Reason: the fluid and its `LiquidBlock` are owned content; their code (a mod's or NeoForge's `BaseFlowingFluid`, whose source we may not read) runs as-is, exactly; one owner. Cost: a crossing per mod fluid tick, only where a mod fluid changes. Known order caveat inherited from mod blocks: Java's tick runs after the native cascade of that tick (`docs/java-mods.md` "Not yet").
- **D4. Native fluids meeting a mod fluid read a boot table.** `canBeReplacedWith` of each mod fluid is evaluated at boot through the real `FluidState.canBeReplacedWith` on a probe getter, for every own state × incoming fluid × direction × {nothing, the same fluid's source} above. Reason: native cannot call Java inside its cascade (`docs/java-mods.md` "Not yet"). Exact for any rule that reads only its own state and the cell above (vanilla's water and lava rules do exactly that). A rule that reads more is out of reach until the cascade can call Java; the boot logs which fluids' `canBeReplacedWith` is a mod's own code, so the lead can check them by M3 (SPEC-5 scenario).
- **D5. Tints.** A tint that doesn't read the world is exported per fluid state; `FluidTintSources.water()`/`BlockTintSources.water()` map to native's biome water tint; a tint whose code reads the world (detected by a getter that throws on any read) is a boot error naming the fluid and draws untinted. None is expected in the finish set; SPEC-2's report proves it. If one appears, the fix is a per-section Java tint pass (not designed).
- **D6. Fluid fog is W4's seam.** W7 exports `fog_code` per fluid and adds it to W4's route predicate; no second fog path.
- **D7. Custom environment renderers run on W1's recording device**, recorded per tick and per frame only while changing (the HUD/W1 rule), drawn natively. Their boolean results are the only thing that disables native's sky, clouds or weather.
- **D8. Mod dimension levels are not W7.** They need `Dim` replaced across 52 files, a native level per `LevelStem`, the generator (AE2's spatial generator is code) and travel. W7 prepares the look (SPEC-10, SPEC-11) and stops there. See Q1.
- **D9. Environment-attribute routes, client side only.** The window's visual attributes come from Java's real probe while a route is active; the server's gameplay attribute seam (`game.rs`'s `computeValuePositional` site) stays with W12/W13.
- **D10. Entity motion in mod fluids is oracle-first**: a real NeoForge server trace of a mob and an item in the test fluid is the expected value; the port follows vanilla's `travelInWater`/`travelInLava` structure with the `FluidType` numbers until the trace matches.

## 4. Specs

| Spec | Title | Depends on | Lines (est.) | Accept |
|---|---|---|---|---|
| [SPEC-1](SPEC-1.md) | Server facts: fluids and every state's fluid; the test fluid `horizon_test:goo` | none | 380 | facts list Mekanism's fluids, Mekanism's logged states hold water |
| [SPEC-2](SPEC-2.md) | Client facts: fluid models, tints, client extensions, overlay states | SPEC-1 (test fluid) | 330 | `baked-models.json` `fluids` for every mod fluid; no D5 errors on both packs |
| [SPEC-3](SPEC-3.md) | Content: `fluid` declarations, per-state `fluid`, the carry, the root's fluid table | SPEC-1, SPEC-2 | 450 | the root has `horizon/fluids.json` and `minecraft:fluid` entries |
| [SPEC-4](SPEC-4.md) | Native: an open fluid set (`FluidKind` ids, `FluidTable`), mod states' fluids, saves, the schedule bridge | SPEC-3 | 480 | `/setblock mekanism:brine` → `horizon-ctl block` shows the fluid; ticks survive a save |
| [SPEC-5](SPEC-5.md) | Mod fluids tick in Java; native's liquid hooks step aside; native fluids meet mod fluids by the table | SPEC-4 | 420 | a placed brine source flows and recedes as on NeoForge |
| [SPEC-6](SPEC-6.md) | Buckets: an empty bucket on a mod fluid runs in Java | SPEC-5 | 180 | bucket in, bucket out, both ways, with sounds |
| [SPEC-7](SPEC-7.md) | Native meshing of mod fluids | SPEC-4 (SPEC-2 data) | 420 | brine, blood, goo draw with their sprites and tints |
| [SPEC-8](SPEC-8.md) | The camera in a mod fluid: overlay texture, fog predicate, fog type | SPEC-4, W4's fog seam (for fog only) | 220 | Mekanism's overlay under brine |
| [SPEC-9](SPEC-9.md) | Entities in mod fluids: NeoForge trace harness, then the native port | SPEC-5 | 2 × 350 | trace diff 0 for mob, item, falling player |
| [SPEC-10](SPEC-10.md) | Dimension effects from the level's dimension type and timelines | W10 SPEC-2 (+ `timeline`) | 400 | vanilla dims pixel-equal; a data-pack dimension type's skybox/ambient apply |
| [SPEC-11](SPEC-11.md) | NeoForge custom sky, cloud and weather renderers, recorded | W1 recorder (present) | 480 | the test renderer replaces the sky in its biome |
| [SPEC-12](SPEC-12.md) | Environment-attribute routes: the client's values from Java's probe | W12 SPEC-2 mechanism, SPEC-10 | 380 | with lithostitched, a biome timeline's sky colour shows |

Order: SPEC-1 → 2 → 3 → 4, then SPEC-5 ∥ SPEC-7 ∥ SPEC-8, SPEC-6 after 5, SPEC-9 after 5; SPEC-11 any time (independent of fluids); SPEC-10 after W10 SPEC-2; SPEC-12 after SPEC-10.
"Playable" for the finish bar (Mekanism, AE2, RS, Jade, BG2 + top-50): SPEC-1..8 and SPEC-11. SPEC-9 is player-visible but rarer (swimming in a processing fluid, BOP's Nether blood). SPEC-10 and AE2's spatial acceptance wait on Q1. SPEC-12 is the M2 bar for lithostitched.

## 5. Interfaces with other workstreams

- **W1:** SPEC-11 adds three passes to `WorldFrame`'s wire (`SKY = 7`, `CLOUDS = 8`, `WEATHER = 9`) and a camera-anchored flag; it reuses `RecordingDevice`, `RecordingRenderPass`, `RecordingPipeline` unchanged.
- **W3:** fluids' `animateTick` (BOP blood) already route through W0's `Animate` (fluid bit 2); drip particles and weather renderers' particles are W3's.
- **W4:** the fog route (D6). W4 owns `ViewportEvent.RenderFog/ComputeFogColor` (Mekanism `ClientTickHandler`) and Jade's `FogRenderer#setupFog` mixin; W7's SPEC-8 adds one predicate term and its export. If W4 lands later, SPEC-8's fog part waits; its overlay part doesn't.
- **W9:** mob pathing in mod fluids (`FluidType.getBlockPathType`, overridden by BOP), boats (`supportsBoating`).
- **W10:** SPEC-10 needs W10 SPEC-2's export to include `timeline` and `world_clock` beside `dimension_type`; mod biomes' sky/fog/water colours are W10 SPEC-4's biome table. W10 §7's P2 (`biomesoplenty:blood` aborting the carry) is fixed (0c59307cf); SPEC-3 replaces that liquid block's lava template rows with its own facts.
- **W12:** SPEC-12 serves three `deferred.tsv` seam keys with W12's routes (`routes.bin`, `Routing.route`), and moves them from `deferred.tsv` to `gates`. `FogRenderer#setupFog` stays deferred to W4.
- **W13:** `CreateFluidSourceEvent` and `FluidInteractionRegistry` interactions keyed on vanilla fluid types (native lava/water meeting a mod fluid) are block events; not W7.

## 6. Out of scope (with reasons)

- Mod dimension levels themselves (Q1), AE2's spatial chunk generator, spatial IO's teleport.
- `RegisterDimensionTransitionScreenEvent`: no mod in either pack listens.
- NeoForge `CustomFluidRenderer`: only GuideMe uses it, in its own GUI scene (`GuidebookLevelRenderer`).
- `IClientFluidTypeExtensions.extractOverlay` overrides: none in either pack; SPEC-2 reports any (`overlay_code`).
- Cauldrons holding mod fluids (`RegisterCauldronFluidContentEvent`: no listener), dispensers emptying mod buckets (Mekanism's `BUCKET_DISPENSE_BEHAVIOR`): dispenser behaviours are W12/W13.
- Interactions registered on vanilla fluid types (`FluidInteractionRegistry`, BOP has one): native's lava/water `shouldSpreadLiquid` knows only vanilla's; from the mod fluid's side they run in Java (D3). W13.
- The server's gameplay environment attributes under lithostitched (D9).
- Mod biomes' colours (W10), mods' fog events (W4), drip and weather particles (W3), boats and pathing (W9).
- A tint that reads the world (D5), a `canBeReplacedWith` that reads beyond its cell and the cell above (D4): reported, not served.

## 7. Open questions for the user

- **Q1.** Who owns mod dimension levels (native `Dim` → registry-driven levels, a `LevelStem`'s generator, travel)? Is AE2's spatial storage part of "AE2 playable end to end"? Without them SPEC-10's mod-dimension acceptance and AE2's spatial sky stay unproven in play.
- **Q2.** D4's table for native water meeting a mod fluid: accept its stated limit, or require the cascade to call Java (the larger `GameLevel`/`Sim` change `docs/java-mods.md` names)?
- **Q3.** D5: is a boot error plus an untinted draw acceptable for a world-reading fluid tint until one appears in a pack?
- **Q4.** SPEC-9 needs a real NeoForge server run of the test mod (the oracle path). Confirm that is allowed under the memory budget as `neoforge_oracle.py` runs are.

## Wire allocations (W7)

- `GameBridge.blockEvents` kinds: **5** scheduled fluid tick, **6** random fluid tick (a = block state id, b = c = 0); **7** `LiquidBlockContainer.placeLiquid` (a = the container's state id, b = the placed fluid state's legacy block state id, c = 0). Rust pushes kind 5 only when the state's fluid is exactly the scheduled tick's fluid (`ServerLevel.tickFluid`'s `fluidState.is(fluid)`), so no fluid id crosses.
- `WorldFrame` passes: **7** `SKY`, **8** `CLOUDS`, **9** `WEATHER` (camera-anchored).
- Status tail after the shell block: `environment` (SPEC-11: replaced-layer bits, then optional recordings) and `attributes` (SPEC-12: a value table when routed).
- Boot facts `schema` 4 (SPEC-1).

## Conventions for every spec

- Comments in code: **one line max**, only a non-obvious why; never restate the code.
- Names: Rust `snake_case` fns beside their neighbours, Java in the files named; no per-mod code (nothing keyed on `mekanism`, `biomesoplenty`, `ae2`): decisions come from facts, registries and own-code flags.
- Tests: independent expected values (vanilla source constants, the test mod's declared numbers, a NeoForge trace), never derived from the code under test.
- Run only the tests each spec names (RULES test budget). Builds with `nice`, private `HORIZON_JAVA_MODS_DIR` while a Java change is unproven, live runs through the gate, stopped right after.
