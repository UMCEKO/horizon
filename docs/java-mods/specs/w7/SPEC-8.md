# SPEC-8 (W7): the camera in a mod fluid: overlay texture, fog type, the fog route predicate

Read first: `DESIGN.md` §1.1 G6, §2.4, D6; `crates/horizon-client/src/game/render_tick.rs` (~300-370: `in_water` from `fog_type`, `water`, `eye_brightness`, `ScreenInputs`), `crates/horizon-client-render/src/screen_effects.rs` (`ScreenInputs.water`, the overlay quad at ~186), `fog.rs` (`fog_type` at 141), the GUI atlas loader that holds `misc/underwater` (`hud.rs`; find where `misc/` textures enter `gui.atlas.sprites`) and how mod GUI textures are added for Java screens (`java_screens.rs`/`Sprites`); vanilla `ScreenEffectRenderer` (water overlay), `Entity.updateFluidOnEyes`, `Camera.getFluidInCamera`; SPEC-4 `ModFluid.overlay_texture`, `fog_code`.

Depends on: SPEC-4. The fog route part needs W4's fog seam (see below); the overlay part doesn't. Comments in code: one line max, only a non-obvious why.

## Goal

- With the eyes in a mod fluid, the screen overlay is that fluid's `getRenderOverlayTexture` (Mekanism's, BOP's), drawn as vanilla's water overlay is (NeoForge's default `extractOverlay` passes the texture to vanilla's `WaterOverlay` state: javap of its lambda's signature).
- The fog type with the camera in a mod fluid is vanilla's: `WATER` only for a fluid in `#minecraft:water`, `LAVA` for `#minecraft:lava`, else atmospheric. SPEC-4 already made the tag class right; this spec proves it.
- W4's fog route learns when the eye's fluid type has fog code.

## Rust

1. **The eye's fluid** (`render_tick.rs`): `fn eye_fluid(&self, eye: [f64; 3]) -> FluidKind`: vanilla `Entity.updateFluidOnEyes`: the fluid state at the eye's block, kept when `pos.y + height(fluid, pos) > eye.y` (height as `FluidState.getHeight`: 1.0 with the same fluid above, else `amount / 9`; native has this for the motion code: reuse it), else `Empty`. A player in a boat keeps today's water exception: read `updateFluidOnEyes` and mirror it.
2. **Overlay**: `ScreenInputs.water: Option<(f32, f32, f32)>` becomes `fluid_overlay: Option<(String, f32, f32, f32)>` (sprite name, brightness, yaw, pitch). For `Water`: `("misc/underwater", ...)` exactly as today (`in_water`, spectators excluded). For a mod `eye_fluid` with `overlay_texture = Some(t)`: `(atlas_name(t), eye_brightness(eye), yaw, pitch)`. `screen_effects.rs` draws whichever sprite it is given with today's quad, UVs and `[b, b, b, 0.1]` colour; `texture_sampling(name)` for its flags.
3. **The textures in the GUI atlas**: every `overlay_texture` in the installed `FluidTable` is loaded into the GUI atlas where `misc/underwater` is, under the name the atlas uses for a mod texture (follow the Java-screen texture path; a missing file → no overlay for that fluid and one log line).
4. **Fog type**: no code change expected; `fog.rs` `fog_type` reads `blocks::Fluid` (the tag class). Add the test below; if it fails, fix `fog_type` to read the tag class.
5. **The fog route predicate** (for W4): `pub(super) fn java_fluid_fog_code(&self) -> bool` in `java_mods.rs`: true when `self.camera_fluid()` (the fluid at the camera position whose height covers it, as `Camera.getFluidInCamera` decides, i.e. the same test as `fog_type` uses for water) is a mod fluid with `fog_code`. W4's spec ORs it into "run the real `FogRenderer.setupFog` in Java this tick". Until W4 lands, nothing calls it except its test; say so in the report.

## Tests

- `render_tick.rs` (or a `game` test with a fixture world): eye at y = 64.5 in a mod source cell at y = 64 (height 8/9 ≈ 0.889 → covers 64.5) → `eye_fluid` is the mod kind; a mod flowing cell with amount 2 (height 2/9 ≈ 0.222) → `Empty`. Expected from vanilla's `getHeight` formula, computed in the test comment.
- `screen_effects.rs`: a `fluid_overlay` with a mod sprite name emits one quad sampling that sprite with colour `[b, b, b, 0.1]`, UVs as the water quad's for the same yaw/pitch (compare with the existing water test's numbers).
- `fog.rs`: a mod fluid not in `#minecraft:water` → `FogType::Atmospheric`; the same fluid with `water_tagged` → `FogType::Water`.
- `java_fluid_fog_code`: true with a `fog_code` mod fluid at the camera, false with water.

## Done bar

- `cargo test -p horizon-client-render screen_effects fog`, `-p horizon-client --features java-mods --lib java_`; clippy both ways; `pr_ready.py --quick`.

## Live test (user: visual)

1. `play/install-rs`, after SPEC-7: a 3-deep brine pool; swim down until the eyes are under the surface: Mekanism's overlay texture tints the view (compare a Forge screenshot: same texture, faint, scrolling with the view).
2. Leave the pool: the overlay disappears at the same eye height as on Forge.
3. Water: unchanged underwater overlay and blue fog.
4. Fog colour inside brine: atmospheric until W4's fog seam lands; then Mekanism's fog colour (W4's acceptance).
