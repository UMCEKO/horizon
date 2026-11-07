# SPEC-3 (W3): the Java client records its quad particles each tick

Design: `DESIGN.md` §2 step 3, D3, D6. Paths: DESIGN's short-hands. Depends on: SPEC-2. Size: ~400 lines
(Java ~250, Rust ~150). SPEC-4 draws what this records.
Comments in code: one line at most, only for a non-obvious why (RULES).

## Goal

After each Java client tick, every live particle in a `QuadParticleGroup` of the Java engine is turned into camera-free
quads with two samples (partial 0 and 1), sent in the tick's status, and stored in Rust's particle engine. No Java call
per frame: the frame interpolates (SPEC-4).

## What one quad holds (the wire, big-endian)

The status gains, right after `ClientParticles.take(out)`: `bool present`, and when true `int len` + the blob:
```
double ax, ay, az                 the recording camera's position (the window's eye at the tick)
int nNames, nNames × (int len + UTF-8)   sprite names, `SubmitCapture.file` form (`minecraft:particle/flame`)
int nQuads, nQuads ×:
  byte flags     bit 0 translucent (the Layer's), bits 1–2 facing (0 camera, 1 camera yaw, 2 fixed), bits 3–4 span (0 the whole tick, 1 partial < 0.5 only, 2 partial ≥ 0.5 only)
  int sprite     index into the names
  sample A then sample B, each:
    float x, y, z           relative to (ax, ay, az)
    float qx, qy, qz, qw    the rotation with the camera taken out (see facing)
    float size              getQuadSize(partial), as passed to QuadParticleRenderState.add
    float u0, u1, v0, v1    relative to the sprite (0..1)
    int color               ARGB as vanilla packs it (ARGB.colorFromFloat)
    int light               light coords as vanilla packs them (block << 4 | sky << 20)
```
`present` is true when there are quads, and once more on the first tick after there were some (an empty blob), so
Rust clears.

## Java

### New `B/ParticleRecorder.java`
```java
/** The Java engine's quad particles after a tick, as camera-free quads at partial 0 and 1, for the window to draw. */
public final class ParticleRecorder {
    static byte[] take(WorldFrame.Cam cam)          // null when nothing to send (see `present`)
    static int facing(Quaternionf b, Vector3f bPos, Quaternionf c, Vector3f cPos, Quaternionf probe) // pure: 0, 1, 2, or -1 (as recorded)
    static byte[] encode(double[] anchor, List<String> names, List<Rec> quads) // pure
    record Sample(float x, float y, float z, float qx, float qy, float qz, float qw, float size, float u0, float u1, float v0, float v1, int color, int light) {}
    record Rec(int flags, int sprite, Sample a, Sample b) {}
}
```

**`take`:**
1. `engine = Minecraft.getInstance().particleEngine`; groups and order through `ParticleEngineAccess` (SPEC-2).
   Walk `horizon$order()`; for each type whose group is a `QuadParticleGroup`, each particle of
   `((ParticleGroupAccess) group).horizon$particles()` (all are `SingleQuadParticle`s).
2. The recording camera: one `new Camera()` kept in a static (the class has an implicit public constructor), placed
   with `CameraAccess.horizon$position(new Vec3(cam.x(), cam.y(), cam.z()))`. Never the main camera: mods read that one.
3. Per particle:
   - rotation `horizon$rotation(180, 0)` (identity: `Camera.setRotation` is `rotationYXZ(π − yRot, −xRot, 0)`), then
     `A = extract(p, 0)`, `B = extract(p, 1)`;
   - rotation `horizon$rotation(240, 35)` (the probe), `C = extract(p, 1)`;
   - `extract(p, partial)`: a fresh `Capture` (below) and `p.extract(capture, camera, partial)`; it returns the captured
     entries in call order. A `Throwable` logs `Horizon: a Java particle's extract failed: <class>` once per class and
     skips the particle this tick.
   - facing per quad `i` of `B`: if `C` has as many entries, `facing(B[i].q, B[i].pos, C[i].q, C[i].pos, PROBE)`, else -1.
     If any quad is -1, re-extract `A` and `B` with the window's own rotation (`horizon$rotation(cam.yaw(), cam.pitch())`)
     and mark every quad of this particle facing 2 (drawn as it was at the tick, D3).
   - pairing: if `A` and `B` have equal counts, quad `i` is `(A[i], B[i])`, span 0; otherwise each `A[i]` is `(A[i], A[i])`
     span 1 and each `B[i]` is `(B[i], B[i])` span 2.
4. Sprites: for each sample, `Sprites.locate(layer.textureAtlasLocation(), (u0 + u1) / 2, (v0 + v1) / 2)`; the name is
   `SubmitCapture.file(Identifier.parse(spot.file()))`, the UVs become `(u − sprite.getU0()) / (sprite.getU1() − sprite.getU0())`
   (and likewise for v), exactly as `SubmitCapture.textured` does. No sprite there: log `Horizon: a Java particle's quad
   has no sprite in <atlas>` once per atlas and drop the quad. Both samples must name the same sprite; when they differ,
   keep the pair as span 1/2 like unequal counts.
5. `encode` and track `present`.
6. With `Profile.ON`, `Profile.event("particles.record", nanos)`.

**`Capture extends QuadParticleRenderState`** (nested): overrides
`add(Layer layer, float x, float y, float z, float xRot, float yRot, float zRot, float wRot, float scale, float u0, float u1, float v0, float v1, int color, int lightCoords)`
to append one entry (layer, the 14 values) to a list; never calls `super`.

**`facing`** (all comparisons per component within `1e-4`, a quaternion also equal to its negation):
- `bPos ≈ cPos` is required for 0–2; otherwise -1;
- 0 (camera) when `c ≈ probe · b`;
- 1 (camera yaw) when `c ≈ yaw(probe) · b`, `yaw(q) = normalize(0, q.y, 0, q.w)` (`FacingCameraMode.LOOKAT_Y` sets
  `(0, cam.y, 0, cam.w)`; SPEC-4's oracle test fixes whether native normalises);
- 2 (fixed) when `c ≈ b`;
- else -1.
With identity at A/B, a camera-facing quad's recorded quaternion is its roll alone (`Rz(roll)`), a yaw-facing one too,
and a fixed one its own world rotation; SPEC-4 multiplies the frame's camera back in.

### `B/ClientBridge.java`
- `tick`: after the second `cameraAndPick(minecraft)` and before `hud()`, `byte[] particles = ParticleRecorder.take(WorldFrame.camera());`
  (the window camera `Input::Camera` set this tick; `take` returns null while it is null). Pass it to
  `status(hud, world, particles)`.
- `status`: after `ClientParticles.take(out)`, write `present`/`len`/blob as above.
- `leave()`: `ParticleRecorder` forgets `present`.

### `docs/java-mods-map.md`
Client status row: insert `particle recording (bool + bytes, SPEC-3 layout)` after the particles list.

## Rust

### New `R/particles/java.rs` (declared `mod java;` in `R/particles.rs` beside `mod native;`, items re-exported as `pub use java::JavaParticles;`)
```rust
/// The Java client's quad particles of its last tick: two samples each, drawn interpolated (SPEC-4).
#[derive(Clone, Debug, Default)]
pub struct JavaParticles { pub(super) anchor: DVec3, pub(super) names: Vec<String>, pub(super) quads: Vec<JavaQuad> }
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Sample { pos: Vec3, rot: Quat, size: f32, uv: [f32; 4], color: u32, light: u32 }
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct JavaQuad { translucent: bool, facing: Facing, span: Span, sprite: u32, a: Sample, b: Sample }
pub(super) enum Facing { Camera, CameraYaw, Fixed }
pub(super) enum Span { Whole, FirstHalf, SecondHalf }
impl JavaParticles {
    /// None when the blob doesn't read to its end.
    pub fn parse(blob: &[u8]) -> Option<Self>;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```
Read with `horizon_java_host::wire::In` as `java_models.rs` does. `uv` is `[u0, u1, v0, v1]` as written. A sprite index
out of range, a facing of 3 or a span of 3 makes the whole blob unreadable (None).

### `R/particles.rs` `ParticleEngine`
- Field `java: java::JavaParticles` (in `new`, `Default`); `pub fn set_java(&mut self, java: JavaParticles)`;
  `clear()` also empties it; `pub fn java_len(&self) -> usize`.

### `G/java_screens.rs` `java_client_status`
After `self.java_client_particles(reply)?;`:
```rust
if reply.bool()? {
    let blob = reply.bytes()?;
    match horizon_client::particles::JavaParticles::parse(blob) {
        Some(java) => self.particles.set_java(java),
        None => { /* log once: "Horizon: the Java client's particle recording didn't read" */ self.particles.set_java(Default::default()) }
    }
}
```
`java_client_left`: `self.particles.set_java(Default::default())`.

## Tests

- **Java unit, new `U/ParticleRecorderCheck.java`** (no boot; JOML and vanilla math only):
  - `facing`: with `probe = rotationYXZ(π − 240°, −35°, 0)` (re-stated from `Camera.setRotation`) and `b = Rz(0.3)`:
    `c = probe·b` → 0; `c = normalize(0, probe.y, 0, probe.w)·b` → 1; `c = b` → 2; `c = Rx(0.2)·b` → -1; equal
    rotations with positions 0.01 apart → -1.
  - `encode` of one quad (anchor (1, 2, 3), names `["minecraft:particle/flame"]`, flags 0b0000_0001, sample values
    1..14 as floats and ints) equals bytes the check writes by hand from the layout above, field by field.
- **Rust unit, `R/particles/java.rs`:** `parse` of a blob built by hand in the test from the layout (two quads: one
  camera-facing span 0, one fixed span 2, two names) reads back every field; the same blob cut by one byte → None; a
  sprite index 2 with two names → None. An empty blob (anchor, 0 names, 0 quads) parses to `len() == 0`.
- `cargo test -p horizon-client-render particles::java`, `cargo test -p horizon-client --features java-mods --lib java_screens`,
  clippy both feature sets, `java_mods.py build`, `unit`.

## Done bar (the implementer checks)

- Tests pass. In the testmod world with the wand held and `HORIZON_JAVA_DEBUG=1`, a temporary log in `set_java`
  (removed before handing in) shows 1 quad, facing 0 (Camera), span 0, sprite `minecraft:particle/flame`, and the two
  samples' x differing by 0.1 (the spark's motion), `size` 0.25.
- With no particle in Java, the status carries `present` false (no blob), after one empty blob.

## Live test (lead)

None visible yet (SPEC-4). With `HORIZON_PROFILE`, `profile_report.py` shows `particles.record` under 0.3 ms mean per
tick near an active AE2 Vibration Chamber and a Mekanism machine, and no recording-related warnings in the log.
