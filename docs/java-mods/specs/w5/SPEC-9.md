# SPEC-9 (W5): the vanilla HUD around mods' layers: order, cancels, wraps, bar heights (revision 2)

Revision 2: unchanged by the D5–D12 rulings except wording: a vanilla layer is drawn by Java only while a mod is
detected to change it (a cancel, a wrap, a moved bar height this frame); everything else stays native.

Read first: `CLAUDE.md`, `RULES.md`, `docs/java-mods-map.md` sections 4 ("HUD timing") and 5, DESIGN 2.7, `B/HudLayers.java`,
`B/mixin/HudLayersMixin.java`, `ClientBridge.hud`/`record`/`hudFrame`, `G/java_screens.rs` `draw_java_hud`, `java_hud_frame*`,
and `G/game.rs` lines ~7150–7380 (native's HUD). `javap` NeoForge's `VanillaGuiLayers`, `RenderGuiLayerEvent`, `GuiLayerManager`.
Paths as SPEC-2.

Independent of SPEC-2..8.

## Goal

Today vanilla's HUD layers run in Java into a discarding target (for their bar-height bookkeeping) and every mod layer is
painted on top of native's whole HUD. Make the HUD vanilla's order: each mod layer is painted between the vanilla layers it
was registered between, a vanilla layer a mod cancels is not drawn, and a vanilla layer a mod changes is drawn **from Java**
only for as long as the change is detected (the user's rule). Accept:
- AppleSkin (showcase): its exhaustion underlay sits **under** the food icons, its saturation overlay and the hunger/health it
  would restore **over** them, and its overlays move with the bars when armor or air shows;
- a mod that cancels a vanilla layer hides it (travelersbackpack and Mekanism listen to `RenderGuiLayerEvent.Pre` in the packs);
- a mod that pushes the pose in `Pre` and pops it in `Post` (a wrap) moves that vanilla layer, drawn by Java for those frames.

Out of scope: layers whose vanilla methods a mod patches (SPEC-10), chat and the tab list's content (W6).

## The layer ids (wire values; `VanillaGuiLayers` order)

0 `CAMERA_OVERLAYS`, 1 `CROSSHAIR`, 2 `AFTER_CAMERA_DECORATIONS`, 3 `HOTBAR`, 4 `PLAYER_HEALTH`, 5 `ARMOR_LEVEL`, 6 `FOOD_LEVEL`,
7 `VEHICLE_HEALTH`, 8 `AIR_LEVEL`, 9 `CONTEXTUAL_INFO_BAR_BACKGROUND`, 10 `EXPERIENCE_LEVEL`, 11 `CONTEXTUAL_INFO_BAR`,
12 `SELECTED_ITEM_NAME`, 13 `SPECTATOR_TOOLTIP`, 14 `EFFECTS`, 15 `BOSS_OVERLAY`, 16 `SLEEP_OVERLAY`, 17 `DEMO_OVERLAY`,
18 `SCOREBOARD_SIDEBAR`, 19 `OVERLAY_MESSAGE`, 20 `TITLE`, 21 `CHAT`, 22 `TAB_LIST`, 23 `SUBTITLE_OVERLAY`.
Java maps a layer's `Identifier` to its id by the `VanillaGuiLayers` field values; check at install (debug log) that the
manager's vanilla layers come in this order.

## Wire

The HUD recording (status `bool hud` + frame, and `client_hud_frame`'s reply) becomes: `int n` = 49 segments (`bytes` each:
0 = before layer 0; `1 + 2*id` = layer `id`'s own content when Java drew it; `2 + 2*id` = the gap after layer `id`, where mod
layers registered after it and its `Post` listeners land), then `int cancelled` (bit per id), `int drawn` (bit per id: Java
drew it, native must not). Rust keeps them in the `CLIENT` HUD state beside `animating`.

## Java (`B/HudLayers.java`, `B/ClientBridge.java`)

Each vanilla layer's wrapper (today's `discarding`) becomes, for layer `id`, at call time (NeoForge calls it only when the
layer's condition holds and no `Pre` listener cancelled it):
1. Cut the HUD recorder: the open gap ends (`HudLayers.cut`, same mechanism as `Shells`: one `Recorder` per HUD recording,
   `Recorder.cut()`, `Shells.concat` for repeats).
2. Decide whether Java draws it this time (`drawn` bit):
   - **wrap**: our `RenderGuiLayerEvent.Pre` listener at `EventPriority.HIGHEST` copied `graphics.pose()` (the `Matrix3x2f`
     and the stack depth) before any mod's listener; if it differs now, a mod's `Pre` moved it;
   - **height**: NeoForge's left/right bar heights (find the fields with `javap` on `Gui`/`Hud`; mods bump them in `Pre`/`Post`)
     differ from what vanilla's own layers produced so far this frame (sum the deltas each vanilla layer's run made, measured
     by this wrapper around its own call): a mod changed the space this layer is placed in;
   - otherwise discard, as today.
3. Run the layer: into the real (recording) graphics when drawn, else into the discarding sink. Cut again: the content is
   segment `1 + 2*id` when drawn, dropped otherwise. The gap after `id` opens.
- **cancel**: a listener on `RenderGuiLayerEvent.Pre`, `EventPriority.LOWEST`, `receiveCanceled = true`, sets the `cancelled`
  bit when `event.isCanceled()` for a vanilla layer.
- Bits and segments reset per recording; a layer not called (condition false, F1) has neither bit and an empty segment.
- `noteRecorded` compares the whole blob (segments and bits), so a cancel that starts or stops starts the animation rule.

## Rust

### R1. Native's HUD in vanilla's layer order (`G/game.rs`)

Reorder native's HUD drawing into the id order above (painter's order in one batch, so this is the order vanilla's strata
give). Wrap each with
```rust
/// Native's vanilla layer, unless a mod cancelled it or Java drew it; then the mods' layers after it.
fn hud_layer(&mut self, batch: &mut HudBatch, layer: HudLayer, native: impl FnOnce(&mut Self, &mut HudBatch))
```
plus `hud_start` (segment 0). Without a Java HUD both only call `native`. Delete `draw_java_hud`'s single paint.
- Split the combined bar drawing (`health_hud.layout`, the mount hearts, the air bubbles) into `PLAYER_HEALTH`, `ARMOR_LEVEL`,
  `FOOD_LEVEL`, `VEHICLE_HEALTH`, `AIR_LEVEL` (tag each `GuiDraw` with its layer, or one call per layer), and split
  `draw_titles` into `OVERLAY_MESSAGE` and `TITLE`, the spyglass and other overlays into `CAMERA_OVERLAYS`.
- A cancelled bar layer must not take height in native's layout either (vanilla's rows above it move down): pass the
  cancelled mask into `HealthInputs` and skip that row's height.
- Horizon's own extras (`draw_extras`) keep their place after `BOSS_OVERLAY`; they are not vanilla layers.
- F1 (`hide_gui`) keeps hiding what it hides today.

### R2. Timing

Unchanged HUD rule (per tick, per frame while animating). A Java-drawn vanilla layer also changes on input: after a hotbar
selection change or a wheel step while `drawn` has `HOTBAR`, ask one `client_hud_frame` on the next frame (as when animating).

## Tests

- Java unit `U/HudLayersCheck.java`: with a fake layer list (two vanilla layers, one mod layer between, as `HudAnimationCheck`
  builds state without a GPU), the recording has the mod's fill in the gap after the first vanilla layer; a `Pre` that pushes
  and translates the pose sets `drawn` for that layer and keeps its content; a cancel sets `cancelled`.
- Rust: the 49-segment blob and the two masks read back; `hud_layer` with `cancelled` skips `native`; the bar layout with
  `FOOD_LEVEL` cancelled puts the air row where food was (compare with vanilla's rule in `Hud.extractPlayerHealth`).
- `java_mods.py unit`, `check`, `cargo test ... --bins java_`, `cargo test -p horizon-client-render hud_layout` (the split),
  clippy, default build.

## Scenario (`scn-9.json`, showcase install, `HORIZON_JAVA_DEBUG=1`)

SPEC-2's floor, survival, `/effect give @s minecraft:hunger 30 2`, `{"wait": 100}`, `{"screenshot": "hud-hunger"}`,
`/effect give @s minecraft:water_breathing 1`, `/fill 0 100 0 0 101 0 water`, `{"wait": 60}`, `{"screenshot": "hud-air"}`,
checks: no exceptions; `log` `HUD layers in vanilla order` present (the install-time debug line).

## User steps (showcase install, survival)

1. Look at the food bar with AppleSkin: the yellow saturation outline sits on the drumsticks, not under them; the darker
   exhaustion band is behind the bar. Hold bread: the drumsticks it would restore flash over the bar.
2. Take damage, hold a golden apple: the hearts it would restore flash on the health bar.
3. Dive (air bubbles show above food): AppleSkin's overlays stay on the food row; nothing overlaps the bubbles.
4. Put armor on: the armor row shows above the hearts; AppleSkin's health overlay stays on the hearts.
5. Equip a traveler's backpack (travelersbackpack) or a Meka-suit piece and look for what that mod hides or moves in the HUD
   (its own docs): the vanilla piece it hides is gone, nothing is drawn twice.
6. F1 hides the HUD as before; the hotbar, crosshair, effects and boss bars look exactly as before.
