# SPEC-8-1: mod entity models as boot meshes

Design: `DESIGN.md` §6 (on detection: mod renderers' overrides, mod models' `setupAnim`, geometry from `RegisterLayerDefinitions` baked at boot), §7, rule "boot data lands in native at boot", rule "all rendering native". Index: `SPEC-2-8.md`. Size: ~280 lines. Needs phase 6.

## Boot export (Java client, at the end of `ClientResources.export`)
- **Layer definitions:** `EntityModelSet`'s roots for every `ModelLayerLocation` a mod registered (`EntityRenderersEvent.RegisterLayerDefinitions`; owned = not in vanilla's `ModelLayers` set at bootstrap, as `BootFacts` decides ownership): the `ModelPart` tree from `LayerDefinition.bakeRoot()`: per part its name, its `PartPose` (x, y, z, xRot, yRot, zRot, scales), its cubes (each polygon's 4 vertices: position and UV, normal), and children. The texture size is in the UVs already.
- **Renderers:** for each owned entity type (and each vanilla type a mod re-registers), the renderer class (`EntityRenderers` map after `RegisterRenderers`), whether it is `LivingEntityRenderer` and its model class and layer location (reflection on the renderer's `model` field after construction with the real `EntityRendererProvider.Context`), its shadow radius, and which vanilla methods its class chain overrides (`extractRenderState`, `submit`, `getTextureLocation`, `shouldRender`, `scale`, `setupRotations`, `getRenderType`…) from 6-1-style class scans; the model class's overrides (`setupAnim`).
- Written as `entity-models.bin` in the content cache (fingerprinted). Format: `int version`, `int nLayers` × (`text location`, part tree), `int nRenderers` × (`text type`, `text rendererClass`, `bool living`, `text layer`, `float shadow`, `int overrides` bitmask over the methods above, `text modelClass`, `bool setupAnim`).

## Rust (`R/mod_models.rs`, new; `G/java_models.rs` reader)
- Parse into `ModModel { parts: Vec<Part> }`, built with the same `mobs::Part` mesh type native's vanilla mob models use (so the draw path, lighting and overlay match vanilla mobs'); textures resolved like vanilla entity textures (path under `textures/entity/…` from the pack, loaded through the resource loader that already reads mod assets).
- `ModRenderers: type id → ModRenderer { model, kind: ModelDriven | FreeForm, … }`: `ModelDriven` when the renderer is a `LivingEntityRenderer` (or `EntityRenderer` with a single model) and only `extractRenderState`/`setupAnim`/`getTextureLocation`/`scale` are overridden; `FreeForm` when `submit` is overridden (8-3).
- No per-frame Java here.

## Tests
- `cargo test -p horizon-client --features java-mods --bins java_models`: a hand-built part tree (two cubes, a child with a pose) round-trips and meshes to the expected vertex count and positions (compare with the vanilla `PigModel` built from exported vanilla data if the vanilla exporter has it).
- Test mod: `TestMobModel` (a cube head and body) registered for `test_mob` with `RegisterLayerDefinitions` and a `MobRenderer`. Boot check: `entity-models.bin` lists the layer with 2 parts and the renderer as `ModelDriven`.
