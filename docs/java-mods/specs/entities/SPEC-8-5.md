# SPEC-8-5: entity render events

Design: `DESIGN.md` §1.1 (`RenderLivingEvent` ≈ 8 a frame, `RenderNameTagEvent.CanRender` ≈ 9), Corrections (render events follow the HUD rule: record per tick, and per frame while the recorded result keeps changing). Index: `SPEC-2-8.md`. Size: ~220 lines. Needs 8-2 to 8-4.

## Events
`RenderLivingEvent.Pre`/`Post`, `RenderPlayerEvent.Pre`/`Post`, `RenderNameTagEvent.CanRender`/`DoRender` (signatures: `javap -p` of the client classes; positions: `callers.sh` on the patched **client** jar for `LivingEntityRenderer.submit`, `EntityRenderer.submitNameTag`/`shouldShowName`).

## Change
- Fire only when listened and admitted for the type on the client (2-1 `admits(.., client)`), for entities native draws this frame.
- **When:** at the recording pass (the Java client's tick job), for each admitted visible entity, in vanilla's per-entity order: Pre, (the recording of 8-2/8-3/8-4 for that entity, if any), Post; name tags at the name-tag point. The listeners' `PoseStack` edits and submits during Pre/Post are captured with `SubmitCapture` into that entity's recording (a mod drawing extra geometry in `RenderLivingEvent.Post` is a recording piece), at partial 0 and 1 like 8-2.
- **Results to native, per entity:** Pre canceled → native skips drawing that entity (vanilla and mod parts) until the next recording says otherwise; `CanRender` → name tag shown or hidden; `DoRender` canceled → hidden; a changed name content → the tag text native draws.
- **Per frame (HUD rule):** an entity whose event results or captured pieces differ between partial 0, 0.5 and 1 (not linear) is asked per frame, in 8-2's per-frame batch.
- Vanilla's own render of the entity stays native: no vanilla renderer runs in Java for a vanilla entity just because a mod listens.

## Tests
- Test mod (client): `RenderLivingEvent.Pre` cancels for entities tagged `horizon_hidden`; `RenderNameTagEvent.CanRender` forces the tag for tagged `horizon_named`. Client checks: the recording marks the hidden one as skipped and the named one with a visible tag.

## User steps
1. `/summon pig ~2 ~ ~ {Tags:["horizon_hidden"]}`: the pig is invisible; its shadow too (vanilla Pre cancel skips the shadow? check `EntityRenderDispatcher`: if the shadow is submitted outside the renderer's `submit`, it still shows; report what vanilla does).
2. `/summon pig ~2 ~ ~ {Tags:["horizon_named"]}`: a name tag reading "Named" shows above it (the listener forces the tag and sets its content), though the pig has no custom name.
3. Pack: a mod that hides or draws on mobs through these events behaves as on Forge.
