# SPEC-4 (W4): `ComputeFovModifierEvent` at vanilla's moment, applied by native

Design: `DESIGN.md` §1.1, §1.2 (native order finding), D4. Size: ~220 changed lines (Java ~90, Rust ~130). Depends on: SPEC-1.

Read first: SPEC-1, `B/mixin/GameRendererMixin.java` (`horizon$tick` runs `mainCamera.tick()`), `B/Lockstep.java`, `G/fov.rs`
(`tick_fov`, `fov_player`, `valid_fov_modifier`), `R/camera.rs` `field_of_view_modifier` and `ViewState::tick` (lines 541, 690).
Vanilla: `Camera.tick`, `tickFov`, `AbstractClientPlayer.getFieldOfViewModifier`, `Minecraft.tick` (`gameRenderer.tick()` before
`level.tickEntities()`). Patched (javap, `w4/scratch/net.minecraft.client.player.AbstractClientPlayer.txt`): the spyglass branch
(`firstPerson && isScoping` → `0.1f`) returns before `ClientHooks.getFieldOfViewModifier(this, modifier, fovScale)`.

Code comments: one line at most.

## Goal

In a lockstep tick the event fires once per tick in Java's camera tick (vanilla's moment: before the entities tick), with the
modifier **native** computed for the state vanilla reads there (the player after the previous tick's move), and native eases toward
the event's result. Outside lockstep nothing changes (Java's `tickFov` keeps running as today).

Accept: the test mod's lens halves the field of view, easing over a few ticks exactly as vanilla eases a sprint's FOV.

## Java
- `B/FovHook.java` (new, ~45 lines): `static float base = 1f; static boolean scoping; static float scale = 1f; static Float target;`
  `static void tick(Camera camera)`: `target = scoping ? 0.1f : (Minecraft.getInstance().getCameraEntity() instanceof
  AbstractClientPlayer p ? ClientHooks.getFieldOfViewModifier(p, base, scale) : 1.0f)`; `static void write(DataOutputStream out)`:
  `bool present` + `float target`, then `target = null`.
- `B/mixin/CameraFovMixin.java` (new; client mixins list): `@Inject(method = "tickFov", at = @At("HEAD"), cancellable = true)` on
  `Camera`: `if (Lockstep.active()) { FovHook.tick((Camera) (Object) this); info.cancel(); }`. Java's own easing doesn't run in
  lockstep (native eases; Java's `Camera.getFov` is filled per frame by SPEC-5).
- `ClientBridge.events`: `Input` kind **35** `FovBase`: `float base`, `bool scoping`, `float scale` → `FovHook` fields.
- The park record (SPEC-1/2): after the motion log, `FovHook.write(out)`. `Lockstep.end()` also publishes a record (empty log and
  `FovHook.write`) when the tick ended without parking at M, and `waitFor(M)` returns it with `parkedHere = false` (a change to
  SPEC-1's "empty when not parked here").

## Rust
- `G/java_screens.rs` `Client`: `fov_base: Option<(f32, bool, f32)>` (what the window's last `tick_fov` computed), `sent.fov_base`,
  `fov_target: Option<f32>`. `java_client_batch` writes `Input::FovBase` (kind 35, new `Input` variant, `write` arm) when it changed.
- `G/java_lockstep.rs`: reading the record after the log: `bool` + `f32` → `CLIENT.fov_target` (also when `parkedHere` is false).
- `G/fov.rs` `tick_fov`:
  ```rust
  let target = self.fov_player().map_or(1.0, |player| {
      let base = field_of_view_modifier(&player, first_person, effect);
      self.java_hook_fov_base(base, first_person && player.scoping(), effect);
      let start = self.java_hook_fov_target().unwrap_or(base);
      let mut modifier = start;
      self.native_compute_fov_modifier(&player, start, &mut modifier);
      valid_fov_modifier(modifier, start)
  });
  ```
  `java_hook_fov_base` stores `CLIENT.fov_base`; `java_hook_fov_target` takes `CLIENT.fov_target` (None outside lockstep or when
  Java sent none). `player.scoping()`: add to `FovPlayer` (`using == Some((UseItem::Spyglass, _))`) if no such helper exists.
  Two-arm wrappers in `G/java_hooks.rs`.
- Timing: `CLIENT.fov_target` is set at M of window tick N; `tick_fov` runs after `self.tick()` in the same frame step
  (`G/game.rs:20051`), so tick N eases toward the event's result for tick N; the base Java used came from tick N−1's `tick_fov`,
  which is the state vanilla's camera tick reads. Note in `tick_fov`'s doc comment that native's own order (after the move) differs
  from vanilla's (Q3) and the Java path follows vanilla's.

## Test mod (`ClientMotion.java`, ~35 lines)
- `ComputeFovModifierEvent` listener: record `getFovModifier()`, `getFovScale()` per tick; while holding `horizon_java_test:lens`
  (new item) `setNewFovModifier(0.5f)`.
- `fov_event_every_tick`: 20 events in 20 ticks while standing.
- `fov_base_standing`: standing on the ground, not flying, using nothing, effect scale 1: `getFovModifier() == 1.0f` (vanilla:
  `(speed / walkingSpeed + 1) / 2` with both 0.1, then `Mth.lerp(scale, 1, m)`).
- `fov_base_sprinting`: the first event whose `getFovModifier()` isn't 1.0 after sprinting starts is one tick after the client
  `PlayerTickEvent.Post` that first sees `isSprinting()` (vanilla's camera tick reads the state the previous tick's move left), and
  equals the value computed in the check as vanilla does: `float speed = (float) (0.1 * 1.3); float m = (speed / 0.1f + 1.0f) / 2.0f;`
  compared with `==` (sprinting's attribute modifier is +30 % multiplied total).
- Drive: `select horizon_java_test:lens` 20 ticks; `key ctrl down`, `key w down`, `wait 10`, up, up.

## Tests
- Rust (`G/fov.rs` tests): with a Java target of 0.5 from a start of 1.0, three ticks of `ViewState::tick` give 0.75, 0.625, 0.5625
  (vanilla's `fovModifier += (target − fovModifier) × 0.5`, typed in the test); a Java target outside 0.01..=10 falls back to native's
  base (existing `valid_fov_modifier` rule); with no Java target the result equals today's.
- Rust: `Input::FovBase` bytes: kind 35, then the float, the bool, the float.
- `cargo test -p horizon-client --features java-mods --lib fov java_lockstep`, `cargo test -p horizon-client-render camera`,
  clippy with and without the feature, `java_mods.py build` (private dir), `unit`, `bridges` (the three checks).
- Harness: `fov` (or what `parity.py --changed-files` names for `G/fov.rs`), unchanged (no Java in it).

## Done bar
Tests pass; the FOV harness unchanged; `bridges` checks ok.

## Live test (lead)
1. Test-mod jar added to the play install, hold the lens: the view zooms to half the field of view, easing over about 5 ticks, and
   back when switching away; holding a spyglass still gives the spyglass view (no event on that branch, as vanilla).
2. Sprint and fly in creative: FOV changes as before (same as a build without java-mods).
