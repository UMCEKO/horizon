# W1 rest: the block-outline override, and the BG2 ghost-preview check

Base: `java-mods` at cc1b066b6. Evidence is static (javap, the reports in `agents/finish/` and `agents/w12/`, the code). No live
boot was used: the gap is a missing call site, which a trace would only show as zero calls. Scratch: `javap/` (javap of the
patched client classes and the NeoForge event, signatures of the BG2 render classes), `scan/` (the jar scan script and
its output, `javap/outline-users-showcase.txt`).

## 1. The gap, with evidence

### 1.1 What the real client does (javap of `minecraft-client-patched-26.3.0.101.jar` and `neoforge-26.3.0.101-universal.jar`)

| Step | Where (patched) | What |
|---|---|---|
| a | `Minecraft.runTick` → `pick(partial)` every frame, then `GameRenderer.extract` | `hitResult` is picked per frame before the level is extracted (vanilla source, `Minecraft.java:1279`) |
| b | `LevelExtractor.extract` → `extractBlockOutline(camera, state)` (bytecode 682), before `blockBreaking`, weather, sky, particles... and before `ExtractLevelRenderStateEvent` (bytecode 1205, the last step) | `state.blockOutlineRenderState = null`; only for a `BlockHitResult` that is not `MISS`, a non-air state and a position inside the world border: it computes `isTranslucent` (`BlockStateModel.hasMaterialFlag(level,pos,state,1)`), `highContrast` (the option), `CollisionContext.of(camera.entity())`, then **posts `ExtractBlockOutlineRenderStateEvent(this, level, pos, state, hit, context, isTranslucent, highContrast, camera, levelRenderState)`**. Canceled: returns, the state stays null (no outline at all). Else `new BlockOutlineRenderState(pos, translucent, highContrast, getShape(level,pos,context), event.getCustomRenderers())` |
| c | `GameRenderer.renderLevel` → `shouldRenderBlockOutline()` | false with F1, for a camera that isn't a player, for adventure players without a matching `CAN_BREAK`/`CAN_PLACE_ON` item, and for spectators unless the block is interactable. **The extract in (b) runs regardless**: only the submit is gated |
| d | `LevelRenderer.submitFeatures` → `submitBlockOutline(poseStack, submitNodeStorage, state)` (bytecode 92) when (c), after particles and before **`SubmitCustomGeometryEvent`** (bytecode 113) | NeoForge's patch: `replaced = false; for (r : state.customRenderers()) replaced |= r.render(state, collector, poseStack, levelRenderState)` (an `ior`: every renderer runs, no short circuit); if `replaced`, return; else vanilla's lines (`submitHitOutline`, high contrast adds the secondary 7 px black pass). The pose stack is `submitFeatures`' fresh one: camera-relative |
| e | — | `RenderHighlightEvent` does not exist in 26.3 (no such class in the NeoForge jar); `ExtractBlockOutlineRenderStateEvent` + `CustomBlockOutlineRenderer` replace it |

### 1.2 What Horizon does

- `WorldFrame.fire` (`B/WorldFrame.java:156`) posts `ExtractLevelRenderStateEvent`, `SubmitCustomGeometryEvent`,
  `PrepareRenderBuffersEvent` and the stages. **No step of (b) or (d) runs**: `grep -rn ExtractBlockOutline` over
  `mods/horizon/src/java-mods-host` and `crates` finds nothing. Every outline listener in the packs is dead.
- `WorldFrame.EVENTS` (the boot's "does a mod listen" list) lacks the outline event, so a pack whose only frame hook is the
  outline would not even record.
- Even if Java called `extractBlockOutline`, it would throw: `LevelExtractorMixin.horizon$level` cancels `setLevel`, so
  `LevelExtractor.level` is null, and the method reads `this.level`.
- The world recording sets no `WindowRenderState.appropriateLineWidth` (`WorldFrame.frame`), so a renderer that copies
  vanilla's line width (Mekanism, AE2 take a width) would get 0.
- Native draws the outline alone (`game.rs` ~16582): its own pick, its gate (F1, `may_build`, adventure items), Horizon's
  native-mod cancel (`native_block_highlight`), and the edges from the boot facts (`f.outline_edges`). For a mod block with a
  block entity the **pick** uses Java's shape (`java_shapes.rs` `java_outline`, `client_shape`), but the **drawn edges** are
  still the facts' static ones (`game.rs` ~16636): a Refined Storage cable or Mekanism transmitter is picked by its real
  shape and outlined by its boot shape.

### 1.3 Who needs it (jar scan of both installs, `scan/scan.py`; the showcase `replaces.json`)

| Mod (pack) | Mechanism | Effect a player sees on Forge |
|---|---|---|
| Mekanism (both) | listener `RenderTickHandler.onBlockHover`; renderers `MekanismOutlineRenderer` (lines from the model, `Outlines.extract`), `IWireframeRendererHandler` (multiblock wireframes), `ConfiguratorOverlayHandler` (also a `CustomGeometryRenderer`: the configurator's coloured side overlay, a sprite) | Mekanism blocks outlined by their model's edges, not the full cube; the configurator shows the aimed side's mode colour |
| AE2 (both) | listener `RenderBlockOutlineHook.install` → `handleEvent`; renderers `PartOutlineRenderer`, `PartPlacementPreviewRenderer`, `FacadePlacementPreviewRenderer`, `FacadeOutlineRenderer`; each keeps a `cameraRelativePos` taken at extract time | On a cable bus, only the aimed part is outlined; holding a part or facade shows where it will go |
| balm (showcase) | `@Inject(method = extractBlockOutline, at = RETURN)` (`LevelExtractorMixin`); `routing: undecided` in `finish/rep-showcase/replaces.json` | balm's `BlockHighlight` callback for its dependents |
| puzzleslib (showcase) | event invoker for its own API (registered lazily, so not in the static listener list) | its dependents' highlight callbacks |
| Building Gadgets 2 | **none of the outline API.** Its selection box and destruction overlay go through `SubmitCustomGeometryEvent` (`RenderLevelLast.onSubmitCustomGeometry`, `renderSelectedBlock`, `DestructionRenderer.render`), its ghost through `PrepareRenderBuffersEvent` + `AfterTranslucentBlocks`/`OitTranslucent` (`VBORenderer.prepareBuffers`, `drawRender`) | already fired by `WorldFrame`; what is left for BG2 is a check (§5) |

M1: on Forge, `ExtractBlockOutlineRenderStateEvent` fires on every frame the crosshair is on a block; on Horizon, 0 times.
M2: balm's `LevelExtractor#extractBlockOutline` patch has no effect (the method never runs anywhere) and is `undecided`.

## 2. The approach

The real `LevelExtractor.extractBlockOutline` runs inside the world recording, at its vanilla place, with **the window's hit**
as `minecraft.hitResult`. Mods' custom renderers run at `submitBlockOutline`'s place through NeoForge's own loop, their
submissions recorded like the rest of the frame and tagged as the outline's. The recording carries an **answer**: what the
outline became for that hit (none, canceled, vanilla, replaced). Native keeps drawing vanilla's lines itself, except when the
answer for the **current** hit says canceled or replaced, and it draws the tagged pieces only while their answer matches the
current hit. When the aimed block changes, the window frame waits for the answer, so a replaced outline never flashes the
vanilla one and a stale one is never drawn.

```
window frame:  pick (native) ──hit──▶ [block changed?] ──yes──▶ wait for a world recording with this hit
                                                       ──no, point moved──▶ ask one next frame (async)
Java recording: hitResult := window hit → extractBlockOutline (event, balm's RETURN inject)
                → ExtractLevelRenderStateEvent → (gate) custom renderers, |= → SubmitCustomGeometryEvent → ...
                answer {kind, hit echo, state id} + pieces flagged OUTLINE
native draw:    answer matches hit?  canceled → nothing | replaced → pieces only | vanilla → native lines + pieces
                no match / no answer → native lines only (as today)
```

Vanilla's own lines stay native (principle 2: one owner per effect). Java runs only what mods hook (principle 0), and only
when a loaded mod listens to the outline event or patches `extractBlockOutline` (detected at boot, as `WorldFrame.decide`).
Packs without that are unchanged: no hit is sent, no wait ever happens.

## 3. Decisions (with reasons)

- **D1. The hit is the window's.** Native owns picking (it already sends its hit for clicks, `ClientBridge.click`). The Java
  client's own `pick` runs only after a tick from the player's tick position, so it lags the window's crosshair by up to a
  tick and could outline a different block than native picks. The recording sets `minecraft.hitResult` to the window's hit
  and restores it after, as `click` does. An entity hit is sent as no block hit (the Java client has no entities, W2), which
  in vanilla also yields no outline.
- **D2. The window waits for the answer when the aimed block changes** (position, face, state, gate or high contrast), at
  most once per distinct key; a move of the hit point inside the same block face asks one asynchronous frame recording, as an
  animating world does. Measured world-frame cost on the play pack: 0.24–0.5 ms per recording (`w0`, `rca7w` logs,
  `Horizon: world frame N us`), so a block change costs about that, plus the rest of a tick job if one is running.
  *Alternative for the user:* never wait; draw no outline at all until the answer comes (1–2 frames without an outline on
  each block change). Waiting is the only option with no visible difference from Forge.
- **D3. Event counts follow the world recorder's rule, not the window's frame rate.** The outline event fires once per world
  recording (each tick, each animating frame, each hit change), as every other W1 frame event already does (the HUD rule,
  W5 D2). M1 compares the outline event's presence and order (before `ExtractLevelRenderStateEvent`, after `pick`), not a
  per-frame count. This is the same standing exception W1 has today.
- **D4. Pieces from custom renderers are tagged** (piece flag bit 4) and never drawn stale. A renderer that returns false
  (extends) draws with native's lines; any true (replaces) suppresses native's lines; a canceled event suppresses both.
  Horizon's own native-mod cancel (`native_block_highlight`) wins over all of it, as a Horizon extra applied last.
- **D5. Vanilla's lines for a block whose shape comes from Java use Java's edges** (`VoxelShape.forAllEdges`), sent with the
  existing `client_shape` reply. This is the drawn half of what `java_outline` already does for the pick (SPEC-3).
- **D6. M2 bookkeeping:** `LevelExtractor.extractBlockOutline` gets the verdict `inline` (native ports it in `Game::render`),
  so a mod's patch routes `java`, and the deferral `special: world_frame` (the W12 form for "an existing mechanism runs the
  real method"): the world recording calls it every recording. No `route!` gate fits a method Java calls itself.
- **D7. No per-frame Java render path is added.** The outline rides the existing recordings; vanilla's lines never go
  through Java.

## 4. Specs

| Spec | Scope | Size | Depends on |
|---|---|---|---|
| [SPEC-1](SPEC-1.md) | Java: the window's hit, the real `extractBlockOutline` in `WorldFrame.fire`, NeoForge's custom-renderer loop at `submitBlockOutline`'s place, the cancel and replace answer, outline-tagged pieces, `appropriateLineWidth`, detection. Rust: `Input::Hit`, the frame request's hit, parsing the answer; outline pieces kept hidden | ~500 | none |
| [SPEC-2](SPEC-2.md) | Rust: native composes the answer with its outline (canceled, replaced, extended), draws tagged pieces only for the current hit, waits on a block change, asks a frame on a point move | ~350 | SPEC-1 |
| [SPEC-3](SPEC-3.md) | Vanilla's outline edges from Java's shape for mod blocks with block entities (`client_shape` + edges) | ~180 | none |
| [SPEC-4](SPEC-4.md) | M2/W12 bookkeeping: verdict, `special: world_frame` deferral, reference pack row | ~30 | SPEC-1 |
| [SPEC-5](SPEC-5.md) | The BG2 ghost-preview check: a per-pass debug line, `hz_scenario` capture/compare, scenario `w1_bg2_preview.json`, user steps | ~200 | none |

Order: SPEC-3 and SPEC-5 any time; SPEC-1, then SPEC-2 and SPEC-4. Each lands alone: after SPEC-1 the mods' outline code
runs (M1) and nothing visible changes; SPEC-2 makes it visible.

### Wire allocations (shared with W5's: W5 owns `Input` kinds 16–25)

- `Input` kind **26 `Hit`**: `bool block`; if block: `long pos`, `int face` (`Direction.get3DDataValue`), 3 `double` point;
  then `bool render` (native's `shouldRenderBlockOutline`), `bool highContrast`. Same layout appended to `client_world_frame`.
- World blob piece flags: 1 no cull, 2 writes depth, **4 outline**.
- World blob trailer after the pieces: `byte outline` (0: no mod needs outlines, nothing follows; 1: an answer follows):
  `byte kind` (0 none, 1 canceled, 2 vanilla, 3 replaced), then the hit it was taken for in the `Hit` layout, then
  `int state` (`Flat.raw(BlockState)` at the hit, 0 without a block).
- `client_shape` reply: after the boxes, `int n` × 6 `double` edges (SPEC-3).

## 5. The BG2 ghost-preview check (SPEC-5)

The rca9 fix (piece depth-write and cull flags, overlay codes 37/38) is in the tree (`WorldFramePieceFlagsCheck`,
`world_renders.rs` `depth_write`), but the user never confirmed the ghost (`RESUME.md`), and rca9 found the existing
scenarios cannot show it: their final `camera` step re-aims the player, which moves the preview off screen. The check:
- a machine check with an independent oracle: BG2's own **server-side build** of the previewed shape. The client preview is
  drawn by `VBORenderer` from the gadget's state with `cull = false` (rca9), so a stone preview is 6 quads per ghost block
  in the after-translucent pass; the build then places N stone blocks through the server, counted by vanilla's `/fill ...
  replace` reply. Pass: `after_translucent quads == 6 × N` and N > 0;
- a screenshot pair against a real NeoForge client (`tools/mod_screens.py`) with the same scenario;
- the user's visual steps (rca9's list plus the outline items of SPEC-2).

## 6. Out of scope (beyond the finish bar)

- Entity outlines and glowing (`shouldShowEntityOutlines`, the spectator outline key): W2.
- A mod patching `LevelRenderer.submitBlockOutline`/`submitHitOutline` bodies or `GameRenderer.shouldRenderBlockOutline`
  (none in either install): native keeps drawing vanilla's lines; such a patch would be reported by M2 as native-owned and
  needs W12 routing then.
- `SharedConstants.DEBUG_SHAPES` outlines (collision, occlusion, interaction shapes): a debug flag.
- Spectators' outline on interactable blocks: native never shows it today (`game.rs` comment at the gate); unchanged.
- Whether the Java client's own `pick` should take the window's hit for good (it duplicates native's pick: W0's question).
  This design only uses the window's hit during a recording.
- Mod outline lines' depth: tagged line pieces draw through the gizmo line passes like every other recorded line
  (`world_renders::draw`); vanilla's `afterTerrain` ordering of mod outline lines is not reproduced (no mod in the packs
  relies on it visibly).
- `FrameGraphSetupEvent` (needs a frame graph): unchanged W1 limit.

## 7. Open questions for the user

- D2: wait on a block change (proposed, exact) or never wait (1–2 frames without an outline per block change)?
- D3: accept the outline event counted per recording rather than per window frame, as the other frame events are?
