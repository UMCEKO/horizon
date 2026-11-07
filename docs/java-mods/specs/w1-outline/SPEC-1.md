# SPEC-1 (W1 outline): the outline hooks run in the world recording, with the window's hit

Read first: `CLAUDE.md`, `/home/umceko/hz-poc/agents/RULES.md`, DESIGN.md §1–3, `docs/java-mods-map.md` "The world recorder",
`B/WorldFrame.java` (all), `B/ClientBridge.java` `tick`, `worldFrame`, `events`, `click`, `forget` callers;
`G/java_world_renders.rs`, `G/java_screens.rs` (`Input`, `Sent`, the tick send ~1440–1530), `R/world_renders.rs` (`apply`,
`draw`), `game.rs` ~16582–16645 (native's outline). javap (allowed): `ExtractBlockOutlineRenderStateEvent`,
`CustomBlockOutlineRenderer`, `BlockOutlineRenderState` (`DESIGN/javap/`).

Paths: `B/` = `mods/horizon/src/java-mods-host/boot/dev/horizonlab/javamods/boot/`, `U/` = the same under `unit/`, `RES/` =
`mods/horizon/src/java-mods-host/boot/res/`, `G/` = `crates/horizon-client/src/game/`, `R/` = `crates/horizon-client-render/src/`.

Depends on: nothing. Size: about 500 changed lines. Comments in code: one line at most, only for a non-obvious why.

## Goal

After this spec, `ExtractBlockOutlineRenderStateEvent` fires in every world recording at its vanilla place, balm's
`extractBlockOutline` inject runs, mods' `CustomBlockOutlineRenderer`s run where NeoForge runs them, their submissions are
recorded as outline-tagged pieces, and every recording carries the outline **answer**. Rust receives it but keeps drawing
exactly what it draws today: outline pieces stay hidden until SPEC-2.

## Java

### J1. `B/mixin/LevelExtractorAccess.java` (new interface mixin), registered in `RES/horizon_java.client.mixins.json` "client"

```java
@Mixin(LevelExtractor.class)
public interface LevelExtractorAccess {
    @Accessor("level") void horizon$setLevel(ClientLevel level);
    @Invoker("extractBlockOutline") void horizon$extractBlockOutline(Camera camera, LevelRenderState state);
}
```
`LevelExtractorMixin` cancels `setLevel` (rendering is native), so the field is null; the recording sets it (J3).

### J2. `WorldFrame`: the hit

- `public record Hit(BlockPos pos, Direction face, Vec3 point, boolean render, boolean highContrast)`; `pos == null` means no
  block hit. `static final Hit NONE = new Hit(null, null, null, false, false)`.
- `static Hit readHit(DataInputStream in)` / `static void writeHit(DataOutputStream out, Hit hit)`, wire layout:
  `bool block`; if block: `long pos` (`BlockPos.asLong`), `int face` (`Direction.from3DDataValue`), 3 `double` point; then
  `bool render`, `bool highContrast`.
- `private static Hit hit = NONE; static void hit(Hit h)`: stores it; if `h.highContrast()` differs from
  `Minecraft.getInstance().options.highContrastBlockOutline().get()`, sets the option (as `PARTICLES` sets its option).
- `HitResult result(LocalPlayer player)`: a block hit is `new BlockHitResult(point, face, pos, false)`; none is
  `BlockHitResult.miss(player.getEyePosition(), Direction.getApproximateNearest(player.getLookAngle()), player.blockPosition())`
  (the same as `ClientBridge.click`).
- `forget()` also resets `hit = NONE`.

### J3. `WorldFrame`: the steps in `fire`, in the real client's order

- `Frame` gets a last component `boolean renderOutline` (from `hit.render()` in `frame(...)`).
- ```java
  interface Outline {
      void extract(Frame f);
      boolean submit(Frame f);
  }
  ```
  `static final Outline REAL` implements them as below; `fire(Frame f, Sink sink, Outline outline)` replaces `fire(f, sink)`.
- Order inside `fire` (javap of the patched jar: `extractBlockOutline` at `LevelExtractor.extract` bytecode 682, before
  `ExtractLevelRenderStateEvent` at 1205; `submitBlockOutline` at `submitFeatures` bytecode 92, before
  `SubmitCustomGeometryEvent` at 113):
  1. `stage = SUBMIT; outline.extract(f);`
  2. `ExtractLevelRenderStateEvent` (unchanged)
  3. `boolean replaced = f.renderOutline() && outline.submit(f);` keep `replaced` in a static for J5
  4. `SubmitCustomGeometryEvent`, `PrepareRenderBuffersEvent`, the stages (unchanged)
- `REAL.extract(f)`: only when `outlineNeeded` (J6); `lastOutline = null`; then
  `((LevelExtractorAccess) f.extractor()).horizon$extractBlockOutline(f.camera(), f.state())` inside a try: on a throw, log
  `Horizon: a Java mod threw during ExtractBlockOutlineRenderStateEvent` with the trace and set
  `f.state().blockOutlineRenderState = null`. Time it with `Profile.event("ExtractBlockOutlineRenderStateEvent", ns)` as
  `safely` does. Without `outlineNeeded`: `f.state().blockOutlineRenderState = null` and nothing else.
- `REAL.submit(f)`: `var st = f.state().blockOutlineRenderState; if (st == null) return false;` then `outlining = true`,
  `customOutline(st, f.collector(), new PoseStack(), f.state())`, `outlining = false` in a finally; a throw is logged
  (`Horizon: a Java mod's block outline renderer threw`) and counts as not replaced.
- ```java
  /** NeoForge's loop at `submitBlockOutline`: every renderer runs; any true skips vanilla's lines. */
  static boolean customOutline(BlockOutlineRenderState st, SubmitNodeCollector collector, PoseStack pose, LevelRenderState level) {
      boolean replaced = false;
      for (var r : st.customRenderers()) replaced |= r.render(st, collector, pose, level);
      return replaced;
  }
  ```
  `|=`, not `||`: the patched bytecode uses `ior` (DESIGN §1.1 d).
- In `frame(...)`: `((LevelExtractorAccess) minecraft.levelExtractor).horizon$setLevel(level)`, and
  `windowState.appropriateLineWidth = Math.max(2.5F, width / 1920.0F * 2.5F)` (vanilla `Window.getAppropriateLineWidth`).
- In `record(...)`: around `fire`, `var previous = minecraft.hitResult; minecraft.hitResult = hit.result(minecraft.player);`
  and restore `previous` in the existing finally.

### J4. `WorldFrame`: outline-tagged pieces

- `public static volatile boolean outlining`: true only inside `REAL.submit`'s renderer loop.
- `BlockRenders.Draw` gets `boolean outline`; `SubmitCapture.Geometry.begin(Draw d, ...)` sets `d.outline = WorldFrame.outlining`.
- `RecordingDevice.RecordedDraw` gets a last component `boolean outline`, filled with `WorldFrame.outlining` where it is
  constructed (`grep -n "new RecordedDraw(" B/`).
- Piece flag bit `4` = outline. `submitted(...)`: the group key adds `(draw.outline ? 1 << 16 : 0)`; the piece's flags add
  `4` for that group. `drawn(...)`: `flags(pipeline) | (d.outline() ? 4 : 0)`. Update the `flags` javadoc: `1 no cull, 2 writes depth, 4 outline`.

### J5. `WorldFrame`: the answer

- `static final byte NONE_KIND = 0, CANCELED = 1, VANILLA = 2, REPLACED = 3;`
- ```java
  static byte answerKind(BlockOutlineRenderState st, ExtractBlockOutlineRenderStateEvent last, boolean replaced) {
      if (st == null) return last != null && last.isCanceled() ? CANCELED : NONE_KIND;
      return replaced ? REPLACED : VANILLA;
  }
  ```
- `lastOutline` is set by a listener `NeoForge.EVENT_BUS.addListener(EventPriority.LOWEST, true, ExtractBlockOutlineRenderStateEvent.class, e -> lastOutline = e)`,
  registered in `decide()` **after** `needed`/`outlineNeeded` are computed (it must not count as a mod's listener).
- `record Answer(byte kind, Hit hit, int state)`; `state` = `Flat.raw(level.getBlockState(hit.pos()))`, 0 without a block.
- `Recording` gets a third component `Answer outline` (null when `!outlineNeeded`); `record(...)` fills it after `fire`.
- `blob(...)`: after `out.write(bytes(visible))`, the trailer: `byte 0` when the answer is null; else `byte 1`, `byte kind`,
  `writeHit(out, answer.hit())`, `int state`. The change test compares the pieces' bytes **and** the trailer's: build
  `lastPieces` from both, so an answer that changes alone sends a blob.
- With `HORIZON_JAVA_DEBUG`, when the answer's kind or position differs from the last one sent:
  `Horizon: outline <none|canceled|vanilla|replaced> at <x> <y> <z>` (or `at -` without a block).

### J6. Detection (`decide()`)

- `EVENTS` gains `ExtractBlockOutlineRenderStateEvent.class`, so `needed` covers a pack whose only frame hook is the outline.
- `static volatile boolean outlineNeeded` = the bus has a listener for `ExtractBlockOutlineRenderStateEvent` (the same
  `getListenerList` read as `listened`), **or** a mod's handler is merged into `LevelExtractor.extractBlockOutline`:
  `Routing.shapes(LevelExtractor.class)` has an `Applied` whose `targets` contains a name starting with
  `extractBlockOutline` and whose `mixin` does not start with `dev.horizonlab.`.
- Boot line, beside the existing one: `Horizon: a mod changes block outlines; outlines are recorded` or
  `Horizon: no mod changes block outlines`.

### J7. `ClientBridge`

- `private static final byte HIT = 26;` and in `events`: `case HIT -> WorldFrame.hit(WorldFrame.readHit(in));`
- `worldFrame(in)`: after reading the camera, `WorldFrame.hit(WorldFrame.readHit(in));`

## Rust

### R1. `G/java_screens.rs`

- `Input::Hit(&'a OutlineHit)` with kind `26`, written as J2's layout (`out.bool`, `out.long(BlockPos::new(..).pack())`,
  `out.int(face_index(face))`, three `out.double`, `out.bool(render)`, `out.bool(high_contrast)`).
- `Sent` gets `hit: Option<OutlineHit>`; in the tick send, beside the camera, compute `self.java_outline_hit()` and write
  `Input::Hit` when it differs from `c.sent.hit` (exact equality, point included).

### R2. `G/java_world_renders.rs`

- ```rust
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub(super) struct OutlineHit {
      pub block: Option<([i32; 3], [i32; 3], [f64; 3])>,
      pub render: bool,
      pub high_contrast: bool,
  }
  ```
  `block` is `(pos, face, point)` of `Hit`.
- `impl Game { pub(super) fn java_outline_hit(&self) -> OutlineHit }`: `block` is `None` when `self.entity_targeted()`, else
  `self.pick()`'s; `render` is `self.vanilla_outline_gate(pos)` (R4) for a block hit, false without; `high_contrast` is
  `self.settings.file.bool("highContrastBlockOutline")`.
- `java_world_frame_request`: append `java_outline_hit()` in the same layout, after the camera floats (one shared
  `fn write_hit(out: &mut Out, hit: &OutlineHit)` used by R1 too).

### R3. `R/world_renders.rs`

- `Piece` gets `outline: bool` (`flags & 4`).
- ```rust
  #[derive(Clone, Copy, Debug, PartialEq, Eq)]
  pub enum OutlineKind { None, Canceled, Vanilla, Replaced }
  #[derive(Clone, Copy, Debug, PartialEq)]
  pub struct OutlineAnswer {
      pub kind: OutlineKind,
      /// The block the answer was taken for: position, face (3D data value), state id.
      pub block: Option<([i32; 3], u8, u32)>,
      pub render: bool,
      pub high_contrast: bool,
  }
  ```
  `apply` reads the trailer after the pieces (missing trailer = old blob = `None`), keeps it in the registry;
  `pub fn outline() -> Option<OutlineAnswer>`; `clear()` resets it. The hit's point is read and dropped.
- `pub fn draw(frame: &Frame, out: &mut Outputs, outline: bool)`: a piece with `outline` is skipped unless `outline`.
  `game.rs` (~11840) passes `false` in this spec.

### R4. `game.rs`: one gate for both

Move the gate in the outline block (~16588: `!self.ui.hide_gui && (self.may_build() || adventure)` and the filter
`self.may_build() || !self.block_action_restricted(h.pos) || self.can_place_on_in_adventure(h.pos)`) into
`fn vanilla_outline_gate(&self, pos: [i32; 3]) -> bool` (doc line: `GameRenderer.shouldRenderBlockOutline`), and use it there.
Behaviour unchanged. `native_block_highlight` stays where it is (Horizon's own mods, not vanilla's gate).

### Docs

`docs/java-mods-map.md`: the world recorder section (the outline steps, the hit, the answer) and the wire table rows (input
event 26, piece flag 4, the trailer, `client_world_frame`'s hit). `docs/java-mods.md` "Block outlines" row: mods' outline
events run in the recording; native still draws (SPEC-2 changes this row again).

## Tests

- **Java `U/WorldFrameOutlineCheck.java`** (new; run by `java_mods.py unit`), independent expectations from the patched
  jar's bytecode (DESIGN §1.1):
  1. Order: an `Outline` stub that appends `Outline` in `extract` and `OutlineSubmit` in `submit`, listeners appending
     `Extract` and `Submit`; with `renderOutline` true the sequence starts `Outline, Extract, OutlineSubmit, Submit`; with
     false, `Outline, Extract, Submit`.
  2. `customOutline`: renderers answering `false, true, false`, each counting its calls: returns true and all three ran once;
     `false, false` returns false; an empty list returns false. Build the state with
     `new BlockOutlineRenderState(BlockPos.ZERO, false, false, Shapes.block(), List.of(r1, r2, r3))`; pass null collector, pose and level state
     (the stub renderers don't read them).
  3. `answerKind`: `(null, null, false)` → 0; `(null, canceledEvent, false)` → 1; `(state, null, false)` → 2;
     `(state, null, true)` → 3. The event: `new ExtractBlockOutlineRenderStateEvent(null, null, BlockPos.ZERO, null, null, null, false, false, null, null)` then `setCanceled(true)`.
  4. Hit wire: `writeHit` then `readHit` gives an equal `Hit` for a block hit and for `NONE`; `NONE` is exactly 3 bytes `0,0,0`.
  5. Tagging: a capture made as `WorldFrameSubmitCheck` makes one, one line draw submitted with `outlining` false and one
     with true; `submitted(...)` gives two pieces, flags without and with bit 4.
- **`U/WorldFrameOrderCheck.java`**: pass a no-op `Outline` and the new `Frame` argument; expectations unchanged.
- **Rust `R/world_renders.rs`**: a blob with trailer `0` → `outline()` is `None`; trailer `1, 3, block (1,2,3) face 1 point,
  render true, high contrast false, state 42` → `Replaced` at `([1,2,3], 1, 42)`; a line mesh in a piece with flags 4 adds
  nothing to `Outputs.lines` with `draw(.., false)` and its lines with `draw(.., true)`.
- **Rust `G/java_screens.rs`**: `Input::Hit` bytes for a block hit at (1, 64, -2), face up, point (1.5, 65.0, -1.5), render
  true, contrast false equal the hand-written expected bytes (kind 26, `true`, the packed long of `BlockPos::new(1,64,-2)`,
  `1`, the three doubles, `1`, `0`).

## Done bar (the implementer checks)

- `java_mods.py build` and `java_mods.py unit` pass (private `HORIZON_JAVA_MODS_DIR` until a boot works, RULES "Shared jars").
- `cargo test -p horizon-client-render world_renders`, `cargo test -p horizon-client --features java-mods --lib java_`.
- `cargo clippy -D warnings` on `horizon-client` with and without `--features java-mods`, and on `horizon-client-render`; fmt.
- `java_mods.py check` reports no new problem.
- No visible change: native's outline is drawn as before (outline pieces are hidden).

## Live test (lead)

`HORIZON_JAVA_DEBUG=1`, `live.sh start w1o` with the play pack (install-rs), creative:
1. Boot log: `a mod changes block outlines; outlines are recorded`.
2. `ctl command "setblock ~2 ~ ~ mekanism:metallurgic_infuser"`, look at it (`ctl turn` / look-at): log
   `Horizon: outline replaced at ...` (Mekanism's renderer took over) and no `threw during ExtractBlockOutlineRenderStateEvent`.
3. Look at a stone block: `Horizon: outline vanilla at ...`. Look at the sky: `outline none at -`.
4. Optional JFR filter `mekanism.client.render.RenderTickHandler::onBlockHover`: calls > 0 (was 0).
5. `live.sh stop`.
