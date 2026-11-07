# SPEC-8-2: model-driven mod entities: poses recorded per tick, drawn natively per frame

Design: `DESIGN.md` §7 ("Model-driven: per tick, part poses from the overrides at partials 0 and 1, lerped natively, with boot meshes"), Corrections (render follows the HUD rule: per tick, and per frame while the recorded result keeps changing). Index: `SPEC-2-8.md`. Size: ~300 lines. Needs 8-1, 5-1 (client views).

## Recording (Java client worker, in `ClientBridge.tick`'s job, after the HUD and world recordings)
For each client view of a `ModelDriven` type that native drew last frame (native sends the visible ids with the tick: `int n` × `int id`, from its entity culler):
1. `renderer.createRenderState()`, then `renderer.extractRenderState(view, state, partial)` at partial **0** and **1** (the views' old and current positions and rotations are the feed's; 5-1);
2. `model.setupAnim(state)` for each, and read every part's pose (`ModelPart.x/y/z/xRot/yRot/zRot/xScale…`, `visible`);
3. the texture (`getTextureLocation(state)`), `scale`, `shadowRadius`, tint, `isInvisible`, light is native's.
Entry per entity: `int id`, texture, pose at 0, pose at 1, plus a third pose at partial 0.5.

## Per-frame rule (HUD rule)
- If the pose at 0.5 equals the linear interpolation of 0 and 1 (within 1e-4 per component), native lerps per frame from the two: no per-frame Java.
- Otherwise the entity is **animating non-linearly** this tick (a `Mth.sin(ageInTicks…)` animation): native asks the Java client, on frames of that tick, for its poses at the frame's partial (op `client_entity_poses`, batched for all such entities, gated like `hud_frame_wanted`: never while a tick runs, at most one request per frame). It stops as soon as a tick's recording is linear again.

## Native draw (`R/mod_models.rs`, the entity pass in `game.rs` beside vanilla mobs)
- Each frame: for each recorded entity, the part transforms (lerped or per-frame), the boot mesh, the native entity transform (position lerp, body yaw, `setupRotations` default: vanilla `LivingEntityRenderer.setupRotations` port native already has for vanilla mobs), hurt overlay and death rotation as vanilla mobs get them.
- `renderer` overrides of `setupRotations`/`scale` are covered by recording the final root transform per tick (pose of a synthetic root part).

## Tests
- `cargo test … java_models`: `linear_poses_lerp_without_java` and `nonlinear_poses_ask_per_frame` on recorded bytes.
- Test mod: `TestMobModel.setupAnim` rotates the head by `ageInTicks * 0.1` (linear) and bobs the body by `Mth.sin(ageInTicks)` (non-linear). Check (client) `poses_recorded`: the recording has the test mob's entry each tick.

## User steps
`/summon horizon_java_test:test_mob ~3 ~ ~`: a two-cube mob is drawn, its head turns smoothly, its body bobs smoothly (no 20 Hz stepping); F3 frame times stay where they were.
