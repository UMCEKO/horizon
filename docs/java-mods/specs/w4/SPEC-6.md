# SPEC-6 (W4): `ViewportEvent.ComputeFogColor` and `ViewportEvent.RenderFog` in the view job

Design: `DESIGN.md` §1.1, §1.4 (W7 D6), §2.3, D5, D6. Size: ~330 changed lines (Java ~120, Rust ~210). Depends on: SPEC-5 (the
view job and its wire; this spec appends to `ViewInputs` and to the outputs).

Read first: SPEC-5 (as landed), `G/render_tick.rs` `frame_fog` (101–160), `native_fog_color_now`, `native_fog_now`,
`R/fog.rs` (`FogInputs`, `FogData`, `setup_fog` at 226, `clear_distance_fog`), `B/ViewFrame.java`. Vanilla:
`FogRenderer.setupFog`, `computeFogColor`, `FOG_ENVIRONMENTS` order (Lava, PowderedSnow, Blindness, Darkness, Water, Atmospheric)
and each `FogEnvironment.isApplicable`. Patched (javap, `w4/scratch/net.minecraft.client.renderer.fog.FogRenderer.txt`):
`ClientHooks.getFogColor(camera, partial, level, renderDistanceChunks, darken, r, g, b, dest)` at the end of `computeFogColor`;
`ClientHooks.onSetupFog(environment, fogType, camera, partial, (float) renderDistanceChunks, fog)` right before `setupFog` returns.
`RenderFog` is not cancellable and carries the `FogData`.

Code comments: one line at most.

## Goal

When a mod listens to either fog event (Mekanism's Vision Enhancement and scuba handlers), the events fire with native's vanilla fog
as their inputs, in vanilla's order (colour first, then the fog), and their results replace native's vanilla fog before Horizon's
Fog option and native mods' hooks. Timing is SPEC-5's (per tick, per frame while changing). Fluid types' `modifyFogColor` and
`modifyFogRender` (W7) run because the events go through NeoForge's own `ClientHooks`.

Accept: Mekanism's Vision Enhancement changes the fog as on Forge (night and underwater).

## Wire (appended to SPEC-5's)
- `ViewInputs`, after `grabbed`: `bool fog`; if set: `byte fogType` (0 atmospheric, 1 water, 2 lava, 3 powder snow; Java maps by
  name to `FogType`), `byte environment` (index in vanilla's `FOG_ENVIRONMENTS` order above), `int renderDistanceChunks`,
  `float darken`, 3 `float` RGB (vanilla's computed colour, before the event), 6 `float` (`environmentalStart`, `environmentalEnd`,
  `renderDistanceStart`, `renderDistanceEnd`, `skyEnd`, `cloudEnd`).
- Outputs: mask bit 5 fog colour (3 `float`), bit 6 fog (4 `float` colour RGBA + the 6 distances), after SPEC-5's in bit order.

## Java (`B/ViewFrame.java`, ~120 lines)
- `FOG_EVENTS = List.of(ViewportEvent.ComputeFogColor.class, ViewportEvent.RenderFog.class)`;
  `static boolean fogWanted()` = `Listeners.modListens(NeoForge.EVENT_BUS, FOG_EVENTS, …, "fog events")`. W7 SPEC-8 ORs its term
  ("the eye's fluid type's client extension has fog code") into this method; SPEC-7 ORs "a route on `setupFog`". `wanted()` ORs it.
- In `take`, after the FOV step (vanilla's `renderLevel` sets up the camera before the fog), when `in.fog` and `fogWanted()`:
  ```java
  var dest = new Vector4f();
  ClientHooks.getFogColor(camera, partial, minecraft.level, rd, darken, r, g, b, dest);
  var fog = new FogData();
  fog.color.set(dest.x, dest.y, dest.z, 1f); /* the six distances from the inputs */
  ClientHooks.onSetupFog(environment(in.environment), fogType(in.fogType), camera, partial, (float) rd, fog);
  ```
  Bit 5 when `dest`'s RGB differs from the input RGB; bit 6 when any of `fog`'s ten values differs from what was put in (after the
  colour event). `environment(i)`: the instance in `FogRenderer`'s list (accessor `B/mixin/FogRendererAccess.java`, static
  `FOG_ENVIRONMENTS`) whose class simple name is vanilla's i-th name; when the patched list doesn't hold it, log once
  `Horizon: fog environment <name> not in the patched list` and use the first element whose `isApplicable(type, player)` is true.

## Rust

### `R/fog.rs` (~30 lines)
`pub fn fog_environment(input: &FogInputs, entity_blind: bool, entity_dark: bool) -> u8`: vanilla's first applicable environment, in
list order, from each class's `isApplicable` (read them: Lava → fog type lava; PowderedSnow → powder snow; Blindness → the camera
entity has Blindness; Darkness → has Darkness; Water → water; Atmospheric → atmospheric). Test it against a table typed from those
`isApplicable` bodies (blind under water → 2; dark in lava → 0; open air → 5).

### `G/render_tick.rs` `frame_fog` (~70 lines)
```rust
let mut fog = horizon_client::fog::setup_fog(tables, &self.fog, &input);
if self.title.is_none() {
    fog = self.java_hook_view_fog(&input, fog);   // record inputs, apply Java's results
}
if !self.settings.horizon.fog { horizon_client::fog::clear_distance_fog(&mut fog, &input); }
if self.title.is_none() { /* native mods' colour and fog hooks, unchanged */ }
```
- `java_hook_view_fog` (two arms; not-arm returns `fog`): records `ViewInputs.fog` (type, `fog_environment`, render distance,
  `darken_world`, `fog.color` RGB, the six distances) and returns `fog` with SPEC-5's `ViewEffects` applied: bit 5 → RGB (alpha kept);
  bit 6 → all ten values. Values that aren't finite, or an end below its start, keep native's (the same validity rule as
  `native_fog_now`).
- `ViewEffects` (`G/java_view.rs`) gains `fog_color: Option<[f32; 3]>`, `fog: Option<FogOut>`; `read_view_effects` reads bits 5–6.
- Frames: `frame_fog` runs once per frame; the inputs are recorded there only (other callers of `setup_fog`, if any, don't record).

## Test mod (`ClientView.java`, ~40 lines)
Item `horizon_java_test:fog_lens`:
- `ComputeFogColor` listener: while held, `setRed(1f); setGreen(0f); setBlue(0f)`.
- `RenderFog` listener: record `getType()`, `getEnvironment().getClass().getSimpleName()`, and the `FogData`; while held,
  `getFogData().environmentalEnd = 8f` (the public field: the test doesn't depend on what NeoForge's setters map to).
- Check `fog_inputs_vanilla` (open air, no lens, render distance R from `minecraft.options.getEffectiveRenderDistance()`):
  type `ATMOSPHERIC`, environment `AtmosphericFogEnvironment`, `renderDistanceEnd == R × 16`, `renderDistanceStart == R × 16 −
  Mth.clamp(R × 16 / 10f, 4f, 64f)` (vanilla's `setupFog` lines, computed in the check).
- Check `fog_events_order`: within one `take`, `ComputeFogColor` precedes `RenderFog`, and `RenderFog`'s `FogData.color` already
  holds the listener's red (vanilla: `setupFog` stores the colour event's result before `onSetupFog`).

## Tests
- Rust: `fog_environment` table; `java_hook_view_fog` with effects bit 5 only changes RGB, bit 6 changes all ten, a non-finite
  distance keeps native's; the `ViewInputs` fog bytes in the documented order (typed expected bytes).
- Java unit `ViewFrameCheck` (SPEC-5's) gains: with stub listeners, colour then fog, bit 5/6 rules.
- `cargo test -p horizon-client --features java-mods --lib java_view render_tick`, `cargo test -p horizon-client-render fog`,
  clippy with and without the feature, `java_mods.py build` (private dir), `unit`, `bridges`.
- Harness: `fog` (or what `parity.py --changed-files` names for `G/render_tick.rs` / `R/fog.rs`, at most two): unchanged.

## Done bar
Tests pass; fog harness unchanged; `bridges` checks ok; on install-rs the log shows `Horizon: fog events heard by
mekanism.client.ClientTickHandler`.

## Live test (lead)
1. Test-mod jar: hold the fog lens: the fog turns red and closes in to 8 blocks at once; switching away restores it. Profile: per-frame
   `client_view_frame` calls only while the lens is held and the view changes.
2. Play install, Meka-Suit helmet with Vision Enhancement on, at night and underwater: the fog/visibility matches a Forge client on
   the same jar and module settings (side by side screenshots); turning the module off restores vanilla fog on the next tick.
3. Without the module: fog identical to a build without java-mods (pixel compare a screenshot at the same spot and time).
