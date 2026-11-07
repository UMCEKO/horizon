# SPEC-5 (W4): the view job: `ComputeCameraAngles`, `CalculateDetachedCameraDistanceEvent`, `ComputeFov`, `CalculatePlayerTurnEvent`

Design: `DESIGN.md` §1.1, §2.3, D5, D6, D8. Size: ~500 changed lines (Java ~170, Rust ~330). Depends on: nothing (W8 SPEC-3 sends
the same real camera for the world recorder; if it landed, reuse its `java_world_camera` for the position). Independent of
SPEC-1…4.

Read first: `docs/java-mods-map.md` §4 "HUD timing" and §5 (status, `client_hud_frame`); `B/ClientBridge.java` (`cameraAndPick`,
`noted`, `hudFrame`, `status`), `B/HudAnimation.java`, `B/WorldFrame.java` (`Cam`, `camera`), `B/mixin/CameraAccess.java`;
`G/java_screens.rs` (`FrameJob`, `FrameDone`, `java_client_frame`, `hud_frame_wanted`, `java_hud_frame_apply`, `Client`);
`G/perspective.rs` 280–345 (the align call), `R/camera.rs` 470–500 and 690–800 (`view_matrix`, `set_rotation`,
`align_with_entity_distance`), `R/gfx/view.rs` (`View::screen`), `R/gfx.rs` `Frame` (line 841), `screen_view`, `camera_frustum`;
`G/fov.rs` (`fov_degrees`, `hud_fov_degrees`), `G/game.rs` `look_options` (8640). Vanilla: `Camera.update`, `alignWithEntity`,
`calculateFov`, `calculateHudFov`, `MouseHandler.turnPlayer`. Patched (javap): `ComputeCameraAngles` posted in `alignWithEntity`'s
non-minecart branch with roll 0, then `setRotation(yaw, pitch, roll)` = `rotationYXZ(π − yaw·d, −pitch·d, −roll·d)`; the mirrored
front view re-sets `setRotation(yRot + 180, −xRot, −roll)`; the sleeping and minecart branches use the two-angle overload (roll 0).

Code comments: one line at most.

## Goal

Mods' per-frame camera listeners run with native's values as inputs and their results drive native's camera, following the HUD
rule (the user's rule for per-frame Java): one firing in each client tick, and one per frame only while the results keep changing.
Vanilla's `Camera.update` and `turnPlayer` bodies never run in Java. `ClientBridge.cameraAndPick` stops posting
`ComputeCameraAngles` at tick time.

Accept: the test mod's roll tilts the picture 20°; its zoom listener halves the FOV; its turn listener halves the mouse
sensitivity; its distance listener puts the third-person camera 8 blocks back.

## Wire
- `Input` kind **36** `ViewInputs` (in the tick batch when changed) and the request of op `client_view_frame` (same layout):
  `float partial`; 3 `double` camera position (the eye before any detached back-off); `float yaw`, `float pitch` (the entity's view
  rotation at `partial`, as `alignWithEntity` reads it); `byte mode` (0 first person, 1 back, 2 front, as `CameraType`);
  `bool minecartBranch`; when `mode != 0`: 4 `float` (scale, distance, mount scale, mount distance); `float fov` (vanilla's
  `calculateFov` result, death and fluid included); `float hudFov`; `double sensitivity`; `bool smoothCamera`; `bool grabbed`.
- Status, appended: `bool view`; if set: the outputs; then `bool viewAnimating`. `client_view_frame` replies `bool animating`,
  `bool changed`, the outputs.
- Outputs: `int mask` (bit 0 angles, 1 distance, 2 fov, 3 hudFov, 4 turn: a bit is set when a listener changed that value), then
  in bit order: angles 3 `float`; distance `float`; fov `float`; hudFov `float`; turn `double` + `bool`.

## Java

### `B/ViewFrame.java` (new, ~130 lines)
- `EVENTS = List.of(ViewportEvent.ComputeCameraAngles.class, CalculateDetachedCameraDistanceEvent.class, ViewportEvent.ComputeFov.class,
  CalculatePlayerTurnEvent.class)`; `static boolean wanted()` = `Listeners.modListens(NeoForge.EVENT_BUS, EVENTS, …, "view events")`
  (SPEC-6 ORs `fogWanted()`); computed at the end of every tick job.
- `static Inputs last` (from `Input` 36), `static byte[] take(Inputs in)` fires, in vanilla's frame order:
  1. `grabbed` → `var t = ClientHooks.getTurnPlayerValues(in.sensitivity, in.smoothCamera)`;
  2. fill the main camera (below), then unless `minecartBranch`: `var a = new ViewportEvent.ComputeCameraAngles(camera, partial,
     yaw, pitch, 0f); NeoForge.EVENT_BUS.post(a);` then `CameraAccess.horizon$rotation3(a.getYaw(), a.getPitch(), a.getRoll())`;
  3. `mode != 0` → `ClientHooks.getDetachedCameraDistance(camera, mode == 2, scale, distance, mountScale, mountDistance)`;
  4. `ClientHooks.getFieldOfView(minecraft.gameRenderer, camera, partial, fov)`, then the same for `hudFov`; write both into the
     camera (`CameraAccess.horizon$fov`, `horizon$hudFov`) so a mod reading `camera.getFov()` later sees them.
  Each output's mask bit is set when the value read back differs (`!=`, `Double.compare` for the double) from its input.
- Fill: `CameraAccess.horizon$position(new Vec3(...))`, `horizon$rotation3(yaw, pitch, 0)` (new `@Invoker("setRotation")` with
  three floats), `horizon$detached(mode != 0)` (new `@Accessor("detached")`), `setEntity(minecraft.player)` when null,
  `horizon$initialized(true)`. No `alignWithEntity`, no `update`.
- Animation: `static boolean noted(byte[] outputs)` as `ClientBridge.noted` (reuse `HudAnimation`): a reply differing from the last
  starts it; two alike end it only once real time passed.
- `ClientBridge.tick`: after `hud()`, `byte[] view = ViewFrame.wanted ? ViewFrame.take(ViewFrame.last) : null` (null also while
  `last` is null); status appends it. `client_view_frame` (new `case`, Render thread): read inputs, `take`, reply.
- `ClientBridge.cameraAndPick`: replace `horizon$align(1.0F)` with `ViewFrame.fillCamera(ViewFrame.last)` when `last` is set, else
  position at the player's eye and rotation `(getYRot(), getXRot(), 0)`; no event fires here any more (vanilla's camera between
  frames holds the last frame's alignment, which is what mods aim from).

## Rust

### 1. Roll in native's camera (`R/camera.rs`, `R/gfx/view.rs`, `R/gfx.rs`, ~110 lines)
- `ViewState` gains `roll: f32`; `set_rotation(y, x)` becomes `set_rotation_roll(y, x, 0.0)`; `set_rotation_roll(y, x, roll)` sets the
  quaternion `rotation_yxz(PI − y·DEG, −x·DEG, −roll·DEG)` and `forwards`, `up`, `left` from it.
- `view_matrix_roll(yaw, pitch, roll)` and `view_rotation_matrix_roll(...)`: today's two with the third angle; the old names call them
  with 0 (every existing caller unchanged).
- `align_with_entity_distance` gains `angles: Option<[f32; 3]>`: in its first `set_rotation` (the non-minecart path this fn ports)
  use `angles` when `Some`, else `(view_y_rot, view_x_rot, 0)`; the mirrored branch becomes `set_rotation_roll(y_rot + 180, −x_rot,
  −roll)`; the sleeping branch stays roll 0. `align_with_entity` passes `None`.
- `View::screen` gains `roll` (its 3 test callers pass 0.0); `Frame` gains `pub roll: f32` beside `yaw`, `screen_view` and
  `camera_frustum` take it, so culling and the world view rotate together. `R/gfx/pack_uniforms.rs:700` and `R/gfx/native_post.rs:1087`
  build their matrices with `frame.roll`. The first-person hand keeps roll 0 (vanilla draws it with its own projection).
- `G/game.rs`, where `Frame { yaw, pitch, … }` is filled from `self.view`: `roll: self.view.roll`.

### 2. The view job (`G/java_view.rs`, new, cfg `java-mods`, ~150 lines; declared in `G/game.rs` beside `java_screens`)
```rust
#[derive(Clone, Copy, PartialEq, Default)]
pub(in crate::game) struct ViewInputs { partial: f32, camera: [f64; 3], yaw: f32, pitch: f32, mode: u8, minecart: bool,
    detached: Option<[f32; 4]>, fov: f32, hud_fov: f32, sensitivity: f64, smooth: bool, grabbed: bool }
#[derive(Clone, Copy, Default)]
pub(in crate::game) struct ViewEffects { angles: Option<[f32; 3]>, distance: Option<f32>, fov: Option<f32>, hud_fov: Option<f32>,
    turn: Option<(f64, bool)> }
pub(in crate::game) fn read_view_effects(reply: &mut In) -> Option<ViewEffects>;
impl ViewInputs { pub(in crate::game) fn write(&self, out: &mut Out); }
```
- `Client` (`java_screens.rs`) gains `view_inputs: ViewInputs` (this frame's, built as the seams run), `sent.view_inputs`,
  `view: ViewEffects`, `view_animating: bool`, `view_recorded: bool`.
- `java_client_batch` writes `Input::ViewInputs` (kind 36) when it differs from `sent.view_inputs`.
- `java_client_status` reads the appended view block into `view`, `view_animating`, `view_recorded = true`.
- `FrameJob` / `FrameDone` gain `view: Option<Vec<u8>>` / `Option<(Vec<u8>, u128)>`; `FrameJob::run` asks `client_view_frame`;
  `java_client_collect` applies it with `java_view_frame_apply` (sets `view` when `changed`, and `view_animating`).
- `java_client_frame`: `let view = hud_frame_wanted(view_animating, take(view_recorded), busy).then(|| inputs bytes)`; the frame job
  goes out when any of hud, world, view is `Some` (same rule: one in flight, none while a tick runs).

### 3. Seams (`G/java_hooks.rs` two-arm wrappers; ~70 lines)
- `G/perspective.rs` (the align at 331): before it, record `view_inputs` (camera = the entity's interpolated eye, yaw/pitch =
  `entity.view_y_rot(partial)`/`view_x_rot(partial)`, mode, minecart branch = whichever path native takes, detached values from
  `entity.living`/`entity.mount` as `align_with_entity_distance` reads them) with `java_hook_view_camera(&inputs)`; pass
  `self.java_hook_view_angles()` as `angles`; in the distance closure apply `self.java_hook_view_distance(distance)` **first**, then
  `* pull`, then `native_camera_distance` (vanilla's event is the vanilla value; the emote pull and native mods are Horizon's).
- `G/fov.rs` `fov_degrees`: right after `self.view.calculate_fov(...)`, `let fov = self.java_hook_view_fov(fov, false);` before the
  zoom extra and `native_compute_fov`. `hud_fov_degrees`: the same with `true`. Record the input only from the frame's own call (add
  a `record: bool` argument used by `render_tick.rs:275` and the projection's call; `zoom_magnification` and `lod_aim` pass false).
- `G/game.rs` `look_options`: `self.java_hook_view_turn(&mut options)` after building it (sensitivity and smooth camera from the
  effects when set); record `sensitivity`, `smooth`, `grabbed` (the `grabbed` computed in `apply_mouse_look`).
- Each `java_hook_view_*` applies a value only when its `ViewEffects` field is `Some`; without the feature they return the input.

## Test mod (`ClientView.java`, new, ~60 lines; registered with `ClientKeys`)
Items `horizon_java_test:tilt`, `zoom`, `slow_mouse`, `far_camera` (new), each acting only while held:
- `ComputeCameraAngles`: `setRoll(20f)`; record (yaw, pitch) received. Check `angles_inputs_native`: the received yaw/pitch equal
  the window's look (`Lockstep`-independent: compare with the player's `getViewYRot(partial)` filled by `turnPlayer` within 1e-3).
- `ComputeFov`: `setFOV(getFOV() * 0.5)`. `CalculatePlayerTurnEvent`: `setMouseSensitivity(getMouseSensitivity() * 0.5)`.
  `CalculateDetachedCameraDistanceEvent`: `setDistance(8f)`.
- Check `view_events_tick_rate`: while nothing is held and the camera still in first person, `ComputeCameraAngles` fires once and
  `ComputeFov` twice per client tick (20 and 40 in 20 ticks: the tick job's firing, no frame firings while nothing changes).
- Drive: `select` each item, `wait 20`, `screenshot` (for the user), `key f5 tap` for `far_camera`.

## Tests
- Rust `R/camera.rs`: `set_rotation_roll(30, 10, 20)` → `forwards`, `up`, `left` equal the vectors from a quaternion built **in
  the test** by the textbook YXZ product `qy(π − 30°)·qx(−10°)·qz(−20°)` applied to (0,0,−1), (0,1,0), (−1,0,0) (within 1e-6);
  roll 0 gives today's values bit for bit; `view_matrix_roll(y, p, 0) == view_matrix(y, p)`; the mirrored detached view negates roll.
- Rust `G/java_view.rs`: inputs bytes in the documented order (typed expected bytes); `read_view_effects` with mask 0b10101 reads
  angles, fov and turn; a frame request is wanted on the same rule as the HUD's (reuse `hud_frame_wanted`'s cases).
- Java unit `ViewFrameCheck` (no game: a stub bus with one listener per event): `take` posts in the order turn, angles, distance,
  fov, hudFov; mask bits only for changed values; minecart branch posts no angles; first person posts no distance.
- `cargo test -p horizon-client --features java-mods --lib java_view java_screens`, `cargo test -p horizon-client-render camera
  gfx::view`, clippy with and without the feature, `java_mods.py build` (private dir), `unit`, `bridges`.
- Harnesses (camera math): `camera` / `third_person` or what `parity.py --changed-files` names for `R/camera.rs` (at most two):
  unchanged.

## Done bar
Tests pass; the camera harnesses unchanged; `bridges` checks ok; `ComputeCameraAngles` no longer appears at tick time
(`HORIZON_JAVA_DEBUG` line `Horizon: view events heard by …` only when a mod listens).

## Live test (lead; test-mod jar in the play install)
1. Hold the tilt item: the world tilts 20° (horizon slanted), particles and the block outline stay attached to their blocks; the
   hand is not tilted. F5: the third-person view tilts the other way in the front view (vanilla negates roll there).
2. Zoom item: the field of view halves at once; switching away restores it. Slow mouse: turning needs twice the mouse travel.
3. Far camera, F5: the camera sits 8 blocks back (pulled in by walls as usual).
4. With no item held and no mouse movement, the profile shows no `client_view_frame` calls (per-frame Java only while changing).
