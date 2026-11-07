# SPEC-2 (W7): client facts for fluids: models, tints, client extensions, side-overlay states

Read first: `DESIGN.md` §0, §2.3, D5; `mods/horizon/src/facts-shared-client/java/dev/horizonlab/facts/BakedWriter.java`; `java-mods-host/boot/.../ClientResources.java` (`baked(Path)`, `take()`); `neoforge-oracle-26-client/.../Baker.java`; vanilla `FluidModel.java`, `FluidStateModelSet.java`, `FluidRenderer.java` (`tesselate`, the overlay test at line ~294), `ModelManager.getFluidStateModelSet()`; `crates/horizon-port-pack/src/oracle.rs` `BakedModels`.

Depends on: SPEC-1 (the test fluid). Comments in code: one line max, only a non-obvious why.

## Goal

What the Java client's real reload baked for each mod fluid (sprites, layer, tint) and what its client extension says (overlay texture, fog code) reach `baked-models.json`, so the carry can declare them (SPEC-3) and native can draw them (SPEC-7, SPEC-8) from the first frame. Also the mod block states that want the side overlay (`shouldDisplayFluidOverlay`).

## Java

### `facts-shared-client/.../FluidModelWriter.java` (new)

`static int write(JsonWriter w, FluidStateModelSet models, Predicate<Fluid> owned) throws IOException`, writing `"fluids": { id: {...} }` for each `Fluid` in `BuiltInRegistries.FLUID` with `owned.test(fluid)`:
- `model = models.get(fluid.defaultFluidState())`; skip (and count) a fluid whose model is the missing model (`models.get` of an unknown fluid returns the set's `missingModel`: compare by identity with `models.get(Fluids.EMPTY.defaultFluidState())` only if that is the missing one; read `FluidStateModelSet` to pick the right test).
- `"layer"`: `model.layer().name()`.
- `"still"`, `"flowing"`, `"overlay"` (null when absent): `material.sprite().contents().name().toString()`; `"force_translucent"`: each material's `forceTranslucent()` as an object.
- `"tint"`: one of
  - `{"kind": "none"}` when `model.tintSource() == null`;
  - `{"kind": "water"}` when the source is `BlockTintSources.water()`'s class, `FluidTintSources.water()`'s class, or `FluidTintSources.of(x)` wrapping `BlockTintSources.water()` (compare classes; the wrapper's field is `val$tintSource`, javap);
  - `{"kind": "constant", "states": {"<SPEC-1 state key>": argb}}` when `colorInWorld` doesn't read the world: for each fluid state `fs`, call `FluidTintSource.colorInWorld(fs, fs.createLegacyBlock(), THROWING, BlockPos.ZERO)` (or the `BlockTintSource.colorInWorld(state, THROWING, ZERO)` overload for a plain `BlockTintSource`) where `THROWING` is a `BlockAndTintGetter` whose every method throws a private `Touched` error;
  - `{"kind": "code"}` when `Touched` was thrown: also print `Horizon: fluid <id> tints from the world in code; Horizon draws it untinted (W7 D5)` once.
- From `IClientFluidTypeExtensions.of(fluid)`:
  - `"overlay_texture"`: `getRenderOverlayTexture(Minecraft.getInstance())` as a string, or null;
  - `"fog_code"`: true when the extension's class declares `modifyFogColor` or `modifyFogRender` (resolve through `getClass().getMethod(...)`'s declaring class ≠ `IClientFluidTypeExtensions.class`);
  - `"overlay_code"`: the same test for `extractOverlay`; when true print `Horizon: fluid <id> draws its own screen overlay; Horizon draws the overlay texture (W7 out of scope)` once.

`static void writeOverlayStates(JsonWriter w, Predicate<Block> owned)`: `"fluid_overlay": { block id: [state keys] }` for owned blocks whose class re-declares `shouldDisplayFluidOverlay(BlockState, BlockAndTintGetter, BlockPos, FluidState)` outside `net.minecraft.`/`net.neoforged.`, listing the states for which it returns true with `EmptyBlockGetter`-like arguments: use a `BlockAndTintGetter` that answers air everywhere (not `THROWING`: the answer may read the state only), `BlockPos.ZERO`, `Fluids.WATER.defaultFluidState()`. State keys as `BakedWriter.key`.

### `BakedWriter.write(...)`

Gains `FluidStateModelSet fluids, Predicate<Fluid> ownedFluids, Predicate<Block> ownedBlocks` and writes `fluids` and `fluid_overlay` after `blocks`, inside the same top-level object; `"version"` 1 → 2. Returns a fourth count (fluids written).

### Callers

- `ClientResources.baked(Path out)`: pass `Minecraft.getInstance().getModelManager().getFluidStateModelSet()`, `BootFacts`' owned-fluid and owned-block tests (add `BootFacts.ownsFluid(Fluid)` beside `ownsBlock`; the registry is already in `REGISTRIES`). Write the file when there are code-built states **or** owned fluids (today it returns early with no states). Extend the log line with the fluid count.
- `neoforge-oracle-26-client/.../Baker.java`: same call, with the oracle's own owned tests (whatever it uses for `codeBuilt`'s blocks: an id outside `minecraft`).

### Test fluid client side (`testmod`)

In `Goo.java`, client only (as `ClientKeys.register(bus)` is gated):
- `RegisterFluidModelsEvent`: `new FluidModel.Unbaked(new Material(minecraft:block/water_still), new Material(minecraft:block/water_flow), null, FluidTintSources.constant(0xFF80FF00))` for source and flowing.
- `RegisterClientExtensionsEvent`: an `IClientFluidTypeExtensions` for the goo type with `getRenderOverlayTexture` → `minecraft:textures/block/slime_block.png` and `modifyFogColor` setting the colour to (0.5, 1.0, 0.0, 1.0).

## Rust (`crates/horizon-port-pack/src/oracle.rs`)

`BakedModels`: `#[serde(default)] pub fluids: BTreeMap<String, FluidModelFacts>`, `#[serde(default)] pub fluid_overlay: BTreeMap<String, Vec<String>>`; `FluidModelFacts { layer, still, flowing, overlay: Option<String>, tint: TintFacts, overlay_texture: Option<String>, fog_code: bool, overlay_code: bool }`, `enum TintFacts { None, Water, Constant { states: BTreeMap<String, u32> }, Code }` (`#[serde(tag = "kind", rename_all = "lowercase")]`). `blocks` stays required; old files (version 1) read with empty maps.

## Tests

- **Java unit check `unit/.../FluidModelWriterCheck.java`** (no client needed): build a `FluidStateModelSet` by hand from two `FluidModel`s whose sprites are stand-ins (look at `BlockRendersCheck`/`SpriteMasksCheck` for how sprites are faked), one with `FluidTintSources.constant(0xFF112233)` and one with `BlockTintSources.water()`, plus a third whose tint source's `colorInWorld` calls `getter.getBlockTint`. Write and parse back: tints are `constant` with `0xFF112233` for every state, `water`, `code`.
- **Testmod check `ChecksFluids.goo_model`** (client session): the `baked-models.json` the boot wrote has `fluids["horizon_java_test:goo"]`: `still == "minecraft:block/water_still"`, `flowing == "minecraft:block/water_flow"`, `overlay == null`, `tint.kind == "constant"` with `0xFF80FF00` for all 16+1 states, `overlay_texture == "minecraft:textures/block/slime_block.png"`, `fog_code == true`, `layer == "TRANSLUCENT"` (water_still's sprite is translucent: vanilla's `WATER_MODEL` is translucent the same way; assert the layer vanilla's water gets, read from `models.get(Fluids.WATER.defaultFluidState()).layer()` in the same check).
- **Rust:** `oracle.rs` fixture with each tint kind parses; a version-1 file parses with empty `fluids`.

## Done bar

- `java_mods.py build`, `unit`, `check` pass (private jars). `cargo test -p horizon-port-pack oracle`, clippy.
- Fresh boot of `play/install-rs` and of `w12/install` (one each, through the gate, `HORIZON_JAVA_DEBUG=1`): `baked-models.json` has a `fluids` entry for every owned fluid with a model (Mekanism's, Generators', BOP's blood and liquid_null, Sophisticated Core's xp, Traveler's Backpack's); the boot log has **no** `tints from the world in code` line. If one appears, stop and report it (D5, Q3).
- The report lists `fluid_overlay`'s blocks for Mekanism (expected: its glass-like blocks; no expected list is known, just paste it).

## Live test (lead)

None visible yet. `python3 tools/logsum.py <boot log> --grep 'baked .* fluids|tints from the world|draws its own screen overlay'`: one `baked` line with a fluid count, nothing else.
