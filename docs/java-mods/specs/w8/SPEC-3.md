# SPEC-3 (W8): the camera Java is told is native's real camera

Read first: `CLAUDE.md`, RULES.md, DESIGN §1.2 G5, `G/java_world_renders.rs` (`java_world_camera`, `java_world_frame_request`),
`G/java_screens.rs` (`Input::Camera`, its send in `java_client_tick_send`), `G/fov.rs` `fov_degrees`, `R/camera.rs`
(`Camera::align_with_entity_distance`: detached back-off, mirrored rotation for the front view), `G/perspective.rs`
(`CameraType`), `B/ClientBridge.java` `events` case `CAMERA`, `B/WorldFrame.java` (`Cam`, `camera`), `B/BlockRenders.java`
`camera`. Vanilla: `Camera.setup`, `GameRenderer.getFov`, `Options.setCameraType`.

Depends on: none. W4 later changes what `fov_degrees` returns (mod FOV events); this spec sends whatever it returns.

Code comments: one line at most.

## Goal

Java's world frames (W1's `RenderLevelStageEvent` stages, `SubmitCustomGeometryEvent`) and the camera-dependent renderer
probes (`BlockRenders.camera`) use the camera the window actually draws from: in third person backed off behind (or in
front of) the player and turned round for the front view, with the frame's real field of view. Mods that ask
`options.getCameraType()` get the window's perspective.

Accept: in F5 (both third-person views), Building Gadgets 2's placement preview stays on the blocks it previews; Mekanism's
camera-facing block entity renders face the third-person camera.

## Rust (`G/java_world_renders.rs`, `G/java_screens.rs`)

- `java_world_camera` returns, when `locator_camera` is `None`: position `self.view.position`, `[self.view.y_rot,
  self.view.x_rot, fov, far]` with `fov = self.fov_degrees(self.clock.fraction() as f32, self.view.position)`. (The view's
  rotation is already turned round for the front view by `align_with_entity_distance`.) Before the first frame aligned the
  view (no player yet), keep today's eye answer. `locator_camera` stays first, unchanged.
- `Input::Camera` gains `u8 camera_type` (0 first person, 1 third person back, 2 third person front: `CameraType` order) written
  after the floats. Send it when the block, the view or the type changed (`c.sent.camera` gains the type).

## Java (`B/ClientBridge.java`)

- Case `CAMERA` reads the type byte after the floats; when it differs from `minecraft.options.getCameraType().ordinal()`, call
  `minecraft.options.setCameraType(CameraType.values()[type])` (vanilla's setter; no save: Java's options file is the scratch
  one). `WorldFrame.camera` and `BlockRenders.camera` unchanged (they take the new values).

## Tests
- Rust: with a `Game` test helper used by `java_world_renders` tests (or a pure function `camera_for_java(view: &Camera, fov, far, locator)`
  extracted for the test): a detached view at `[0, 64, 0]` backed 4 blocks gives that position, not the eye; a front view gives
  `y_rot + 180` and `-x_rot` (compare with the formula of vanilla `Camera.setup` for the mirrored case, not with the code under
  test); `locator_camera` wins.
- Rust: `Input::Camera` bytes carry the type after the 7 numbers.
- Java: `java_mods.py bridges` unchanged checks pass (no new check; the camera type is vanilla's setter).
- `cargo test -p horizon-client --features java-mods --lib java_world_renders java_screens`, clippy with and without the feature,
  `java_mods.py build` (private dir), `unit`, `check`.

## Done bar
Tests pass; one `hz_scenario.py` run with the scenario below has no exceptions.

## Live test (lead; install-rs)
Scenario `tools/scenarios/w8_camera.json`: forceload and floor as SPEC-1's scenario, `{"tp": [0.5, 100, 0.5]}`, check
`near`, survival, `/give @s buildinggadgets2:gadget_building`, `{"slot": 0}`, `{"lookat": [0.5, 99, 3.5]}`, `{"key": "f5"}`,
`{"wait": 20}`, `{"screenshot": "back"}`, `{"key": "f5"}`, `{"wait": 20}`, `{"screenshot": "front"}`; checks `no_exceptions`.
(Check the gadget's item id in the install's `analysis` before running.)
User steps: hold the BG2 building gadget aimed at the floor, F5 twice: the preview ghost stays on the targeted blocks in both
third-person views (on Forge it does; before this spec it shifts by the camera distance). Place a Mekanism block with a
camera-facing render (a Digital Miner's or a QIO's screen render): it faces the camera in F5.
