# SPEC-8-3: free-form mod entity renderers

Design: `DESIGN.md` §7 ("Free-form submits: `BlockRenders`' collector per entity"). Index: `SPEC-2-8.md`. Size: ~250 lines. Needs 8-2.

## Change
- A `FreeForm` renderer (8-1: its class overrides `submit`): per tick, for each visible view, Java runs `renderer.extractRenderState` and `renderer.submit(state, poseStack, collector, cameraState)` with `B/SubmitCapture.java` (shared with `BlockRenders` and the world recorder) as the `SubmitNodeCollector`: model parts submitted with their textures, custom geometry, items (`submitItem`), block models, name tags (8-5 decides), shadows.
- The capture is entity-relative (pose stack starting at the entity's origin) and recorded at partial 0 and 1 like 8-2, with the same linear/per-frame rule (compare the two captures' transforms piece by piece at 0.5).
- Native draws the pieces in the entity pass with `R/dynamic_renders.rs`'s machinery (meshes keyed by content, sent once; pieces placed per frame), the same as block entity renderers, but with the entity's interpolated transform.
- A free-form renderer that extends a vanilla renderer and calls `super.submit` for the vanilla part: the vanilla part's submits are captured too (they are the vanilla renderer's code running on a mod type's view: allowed, it is the mod's renderer's behaviour), so the whole entity is one recording.

## Tests
- Test mod: `TestTntRenderer extends TntRenderer`, `submit` adds a floating item above via `submitItem`. Client check `free_form_recorded`: the recording has the TNT block model and the item.
- `java_mods.py unit`: `SubmitCaptureEntityCheck` (entity-relative capture of a cube).

## User steps
AE2 tiny TNT: primed, it draws small and flashes like on Forge. Mekanism flamethrower flames draw.
