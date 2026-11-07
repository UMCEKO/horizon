# SPEC-4 (W3): native draws the Java particles' recording every frame

Design: `DESIGN.md` §2 step 3, D3, D6. Paths: DESIGN's short-hands. Depends on: SPEC-3. Size: ~300 lines (Rust, one
small Java oracle check). First step a player sees.
Comments in code: one line at most, only for a non-obvious why (RULES).

## Goal

Each frame, `ParticleEngine::render`/`render_culled` also emit the recorded Java quads: interpolated between the two
samples with the frame's partial tick, turned by the frame's camera per facing, lit by the recorded light coords, in the
opaque or translucent particle layer, culled as native particles are. Vertices match vanilla's
`QuadParticleRenderState.renderRotatedQuad`.

## Rust: `R/particles/java.rs`

```rust
impl JavaParticles {
    /// Appends the quads at `partial`, seen from `camera` turned by `base` (the frame's `Camera.rotation()`).
    pub(super) fn draw(
        &self,
        camera: DVec3,
        base: Quat,
        partial: f32,
        atlas: &HashMap<String, (Rect, bool)>,
        keep: &mut dyn FnMut(Vec3, f32) -> bool,   // culling: camera-relative centre, size; true to draw
        out: &mut Vec<Vertex>,
    );
}
```
Per quad:
1. Span: `FirstHalf` only when `partial < 0.5`, `SecondHalf` only when `partial >= 0.5`.
2. `t = partial`. Centre: `(anchor + lerp(a.pos, b.pos, t)) − camera`, in `f64` then `as_vec3()` (as native particles).
3. `size = lerp(a.size, b.size, t)`.
4. Rotation: `r = a.rot.normalize().slerp(b.rot.normalize(), t)` (vanilla lerps the roll angle; a slerp between two
   rotations about z is that lerp). Then by facing:
   - `Camera`: `base * r`;
   - `CameraYaw`: `yaw(base) * r`, `yaw(q) = Quat::from_xyzw(0.0, q.y, 0.0, q.w).normalize()`;
   - `Fixed`: `r`.
   The oracle test below decides whether `yaw` normalises (JOML's `Vector3f.rotate` over a non-unit quaternion): match
   the oracle's corners, keep whichever form does.
5. The sample for discrete values: `s = if t < 0.5 { a } else { b }`. UVs: sprite `rect = atlas[key]` with
   `key = names[sprite].strip_prefix("minecraft:")` (as `world_renders` resolves names); a missing key skips the quad
   and logs `Horizon: a Java particle's sprite <name> isn't in the atlas` once per name. `u = rect.u0 + (rect.u1 − rect.u0) · s.uv[k]`.
6. Colour: lerp each ARGB channel of `a.color` and `b.color` by `t`, then native's packing (`a << 24 | b << 16 | g << 8 | r`,
   as `render_inner` builds `color`).
7. Light: from `s.light` (vanilla coords): `block = (l & 0xFFFF) >> 4`, `sky = (l >> 16) >> 4`, each clamped to 15;
   native's `light = sky * 16 | (block * 16) << 8` (the form `render_inner` writes).
8. `keep(centre, size)`; when false skip.
9. Four vertices in vanilla's order, as `render_inner`'s loop: `(1, −1) → (u1, v1)`, `(1, 1) → (u1, v0)`,
   `(−1, 1) → (u0, v0)`, `(−1, −1) → (u0, v1)`; position `centre + rotation * Vec3(n.0, n.1, 0) * size`;
   `layer = if translucent { TRANSLUCENT_PARTICLE_OVERLAY } else { 0 }` (hdr tier 0), `normal [0; 4]`,
   `glint_uv [0, 0]`.

## Rust: `R/particles.rs`

- `render_inner`: after the native particles' loop, `self.java.draw(camera, base, partial, &self.atlas, &mut keep, out)`
  where `base` is the quaternion the function already builds. `keep`:
  - without culling (`CULL == false`): always true;
  - with culling: the same three tests a native particle gets (radius `size·√2 + 0.05`): `frustum.intersects_sphere`,
    `below_half_pixel`, `occluded(min, max)` on the world box around the centre. Factor the three tests out of the
    native loop into a helper used by both (no behaviour change for native particles; their section cache stays theirs).

## Java oracle check: `U/ParticleCornersCheck.java`

Uses vanilla's own code with no boot: a `QuadParticleRenderState`, `add(Layer.OPAQUE, x, y, z, q..., size, u0, u1, v0, v1, color, light)`
then `buildLayer(Layer.OPAQUE, consumer)` with a capturing `VertexConsumer` (implement `addVertex(float,float,float)`,
`setUv`, `setColor(int)`, `setLight`; the rest return `this`). Print the four corners for:
1. `Camera` mode, camera `setRotation(180, 0)` equivalent `rotationYXZ(0, 0, 0)`, roll 0, size 0.5, position (1, 2, 3);
2. `Camera` mode, camera yaw 90, pitch 30 (`rotationYXZ(π − 90°, −30°, 0)`), roll 0.3, size 0.25, position (0, 1, −2);
3. `CameraYaw` mode with the same camera: the quaternion vanilla's `LOOKAT_Y` builds, `(0, cam.y, 0, cam.w)` unnormalised,
   then `rotateZ(0.3)`, size 0.25, position (0, 1, −2).
The check asserts case 1 against hand values `(1.5, 1.5, 3), (1.5, 2.5, 3), (0.5, 2.5, 3), (0.5, 1.5, 3)` and prints
cases 2 and 3 with 6 decimals. Copy the printed numbers into the Rust test as its expected values, citing the check by
name in a one-line comment.

## Rust tests (`R/particles/java.rs`)

- Corners: one quad per oracle case (samples equal, so `t` doesn't matter), drawn with `base` from yaw/pitch exactly as
  `render_inner` builds it; the four vertex positions equal the oracle's within `1e-5`.
- Interpolation: a quad with `a.pos (0, 0, 0)`, `b.pos (1, 0, 0)`, sizes 0.2 → 0.4, colours `0xFF000000 → 0xFFFFFFFF`,
  rolls 0 → 1 rad, drawn at `partial 0.25`: centre x 0.25, size 0.25, colour channels 64 (`(255·0.25) as u8` rule as
  native's `c()`), the rotation equals `Rz(0.25)` (Fixed facing) within `1e-5`.
- Spans: `FirstHalf` drawn at 0.4, not at 0.6; `SecondHalf` the reverse.
- Light: `l = 7 << 4 | 12 << 20` gives `12 * 16 | (7 * 16) << 8`.
- Missing sprite: skipped, no panic.
- `cargo test -p horizon-client-render particles`, clippy, and `python3 /home/umceko/hz-poc/verify/parity.sh` for the
  particle harnesses touching `render_inner` (`tools/parity.py --changed-files ... --list` names them; run at most 3,
  starting with `particle_providers` and `emitter_particles`), since the culling tests were factored.

## Done bar (the implementer checks)

- Oracle and Rust tests pass; the listed harnesses pass unchanged.
- Testmod world, wand held, offscreen `horizon-ctl screenshot` looking at (8.5, 102, 8.5) from 4 blocks: a flame sprite
  that moves +x across one block in a second (the spark), next to the native flame. One screenshot only.

## Live test (user: visual)

Play pack, survival, `HORIZON_JAVA_DEBUG=1`:
1. AE2 Vibration Chamber burning coal: its particles show on the block's face and drift like on Forge.
2. Mekanism Energized Smelter running (Creative Energy Cube): active effects as on Forge.
3. Wear a Mekanism jetpack with hydrogen and use it (needs W4 for flight; the flame shows while Mekanism thinks it
   fires): the flame and smoke trail under the player, turning with the camera (they face you as you turn).
4. Turn the camera quickly around any of these: the billboards keep facing the camera every frame (no lag of a tick),
   positions move smoothly (no stepping at 20 Hz).
5. Options → Video → Particles: Minimal: mods' particles thin out as on Forge (the limiter runs in Java, fed by
   `Input::Particles`).
