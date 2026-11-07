# SPEC-2 (W5): the shell's recording cut at vanilla's draw steps (revision 2)

Read first: `CLAUDE.md`, `/home/umceko/hz-poc/agents/RULES.md`, `docs/java-mods-map.md` sections 4 and 5,
`/home/umceko/hz-poc/agents/w5/DESIGN.md` (section 2, Decisions D1–D12, "Decisions D5–D12"), `SPEC-1.md`.
Clean room: `javap` only for NeoForge and mods; vanilla's decompiled sources are fine (`.local/minecraft/26.3/sources/client/`).

Paths: `B/` = `mods/horizon/src/java-mods-host/boot/dev/horizonlab/javamods/boot/`, `G/` = `crates/horizon-client/src/game/`,
`RES/` = `mods/horizon/src/java-mods-host/boot/res/`, `U/` = `mods/horizon/src/java-mods-host/unit/dev/horizonlab/javamods/boot/`.

Depends on: SPEC-1 (merged). Needed by every later W5 screen spec.

## Rule (user, D5/D7)

A shell runs vanilla's `init()` and nothing else of vanilla's screen code, except the vanilla methods a loaded mod is
detected to patch (W12's `routes.bin`) and the events mods listen to. So `Shells.record` keeps posting the screen events
itself (SPEC-1), never calls `extractRenderState` or `extractContents`, and calls a vanilla draw method only when it is
routed (SPEC-7). This spec makes the recording line up with native's draw steps so a routed method can later drop in at its
exact place.

## Goal

Replace SPEC-1's five fixed segments by segments keyed by vanilla's draw steps ("leaves"), in vanilla's order. Native draws
each of its own steps through `shell_leaf`, and paints what mods drew after that step right after it. Nothing visible
changes; no vanilla method runs in Java that does not run today.

## The leaf table (shared by Java and Rust; ids are wire values)

Positions follow vanilla's order (`Screen.extractRenderStateWithTooltipAndSubtitles`, `AbstractContainerScreen.extractRenderState`/
`extractContents`, `AbstractRecipeBookScreen.extractRenderState`, NeoForge's events where SPEC-1 measured them).

| id | Leaf | Vanilla method at that step (the route key SPEC-7 checks) | Native counterpart |
|---|---|---|---|
| 0 | `START` | none: `Render.Pre` content | before everything |
| 1 | `BACKGROUND` | the screen's `extractBackground` | dim fill, panel, progress sprites, player model |
| 2 | `EFFECTS` | `EffectsInInventory.extractRenderState` (inventory, creative) | `draw_inventory_effects` |
| 3 | `WIDGETS` | each vanilla widget's `extractRenderState` | sort buttons, recipe-book toggle; then Java paints mods' widgets |
| 4 | `LABELS` | the screen's `extractLabels` | titles |
| 5 | `HIGHLIGHT_BACK` | `AbstractContainerScreen.extractSlotHighlightBack` | none |
| 6 | `SLOTS` | the screen's `extractSlots` / `extractSlot` | `draw_inventory_slots` |
| 7 | `HIGHLIGHT_FRONT` | `AbstractContainerScreen.extractSlotHighlightFront` | the hovered-slot fill |
| 8 | `RECIPE_BOOK` | `RecipeBookComponent.extractRenderState` | `draw_recipe_book` (panel) |
| 9 | `CARRIED` | `AbstractContainerScreen.extractCarriedItem` | `draw_carried` |
| 10 | `TOOLTIP` | the screen's `extractTooltip` | `draw_slot_tooltip` |
| 11 | `DEFERRED` | `GuiGraphicsExtractor.extractDeferredElements` (always run in Java: it draws only what mods deferred) | none |
| 12 | `EXTRA` | a screen class's own drawing in its `extractRenderState`/`extractContents` override (SPEC-5: the merchant's offers) | that screen's own parts |
| 13 | `BOOK_TOOLTIP` | `RecipeBookComponent.extractTooltip` | the recipe book's hover tooltip |

Each leaf has two segments: `part 0` = the gap **after** it (what mods drew between this step and the next), `part 1` = the
leaf's own content when Java drew it (a body-routed method, SPEC-7). Index `leaf * 2 + part`: 28 byte arrays, each a complete
GUI frame (`int n` + entries). The events land in gaps: `Render.Pre` in `START`'s gap, `Render.Background` after
`BACKGROUND`, mods' widgets in `WIDGETS`' content (native always draws its widgets first, then paints it), `Render.Foreground`
after `HIGHLIGHT_FRONT`, deferred elements and `Render.Post` after `DEFERRED`. An edge-routed method (SPEC-7) puts its HEAD
handlers' drawing in the gap before its leaf and its RETURN/TAIL handlers' drawing in its gap after, with native's own step
in between: no extra segment is needed.

## Wire

1. Status tail (`ClientBridge.status`, Rust `read_shell_tail`): `int shell`, `bool recorded` (+ `int n` = 28, n x `bytes`),
   `int routed` (bit `1 << leaf`: Java drew that step, native skips its own), `bool shellAnimating`.
2. `ShellReply`: `byte owner`, `bool recorded` (+ 28 x `bytes`, no count), `int routed`, `bool animating`.

## Java

### J1. `B/ShellLeaves.java`
`static final int START = 0 ... BOOK_TOOLTIP = 13, COUNT = 14`, `static String name(int)`.

### J2. `B/Recorder.java`, `B/Shells.java`
- `static byte[] concat(byte[] a, byte[] b)` (two GUI frames into one: `int n` = sum, entries in order), next to `cut()`.
- `Shells.record` keeps SPEC-1's sequence and adds the steps, in this order (each `step(id)` closes the open gap with
  `recorder.cut()` and opens the gap after `id`; `steps` without a routed method draw nothing):
  `START` (post `Render.Pre`; cancelled: keep SPEC-1's log, return), `BACKGROUND`, post `Render.Background`, `EFFECTS`,
  `WIDGETS` (mods' widgets: `renderables` not in `vanilla`, as today, into the content part), `LABELS`, `HIGHLIGHT_BACK`,
  `SLOTS`, `HIGHLIGHT_FRONT`, post `Render.Foreground`, `EXTRA`, `RECIPE_BOOK`, `CARRIED`, `TOOLTIP`, `BOOK_TOOLTIP`,
  `DEFERRED` (`graphics.extractDeferredElements`), post `Render.Post`.
- `static boolean routes(Screen shell, int leaf)`: the one place that says whether a step's vanilla method runs; false for
  every leaf in this spec (SPEC-7 fills it from `routes.bin`, SPEC-8 from tooltip events).
- `static int routed`: bits of the steps Java drew in this recording.
- `noteRecorded`, `write`, `reply`: 28 segments and `routed`.

## Rust (`G/java_screens.rs`, `G/game.rs`)

### R1. State
`#[repr(u8)] enum Leaf { Start, Background, Effects, Widgets, Labels, HighlightBack, Slots, HighlightFront, RecipeBook,
Carried, Tooltip, Deferred, Extra, BookTooltip }`; `Shell { segments: [Vec<u8>; 28], routed: u32, .. }`; `read_segments`
reads 28, `read_shell_tail` and `java_shell_apply` read `routed`.

### R2. Painting
```rust
/// Before the first step: what mods drew in `Render.Pre`.
pub(in crate::game) fn shell_start(&mut self, batch: &mut HudBatch)
/// Native's own step, or Java's when Java drew it; then what mods drew after it.
pub(in crate::game) fn shell_leaf(&mut self, batch: &mut HudBatch, leaf: Leaf, native: impl FnOnce(&mut Self, &mut HudBatch))
```
No shell open: call `native`, nothing else (no Java call, no copies). Else: `routed` bit → paint `part 1`, else `native`;
`Leaf::Widgets` always both (native first). Then paint `part 0`. Paint as `draw_java_shell` does; delete `draw_java_shell`.
Stubs in the `cfg(not(feature = "java-mods"))` arm.

### R3. `draw_inventory` in step order
The non-station path (survival inventory, chests, furnaces, crafting, crafter, mounts): `java_shell_frame()`, `shell_start`,
then `shell_leaf` around `BACKGROUND` (fill, panels, progress, player model), `EFFECTS`, `WIDGETS` (sort buttons, the book
toggle; split `draw_recipe_book` into button and panel if needed), `LABELS` (move the titles out of the background `match`),
`HIGHLIGHT_BACK` (nothing), `SLOTS`, `HIGHLIGHT_FRONT`, `EXTRA` (nothing here), `RECIPE_BOOK`, `CARRIED`, `TOOLTIP`,
`BOOK_TOOLTIP` (split the book's hover tooltip out of `draw_recipe_book` if it is drawn there), `DEFERRED` (nothing). Move
code only; the effects column moves before the widgets (vanilla's order), it overlaps nothing.

## Tests
- `U/ShellLeavesCheck.java`: `concat` of frames of 1 and 2 fills is one frame of 3 in order; empty + frame = frame.
- Rust (`java_screens.rs` tests, replacing SPEC-1's two segment tests): a status with 28 segments (non-empty at 1 and 7) and
  `routed = 1 << 10` reads back; a `ShellReply` without a recording keeps the old segments; `shell_leaf` with no shell calls
  `native` once and paints nothing else.
- `java_mods.py unit`, `check`; `cargo test -p horizon-client --features java-mods --bins java_`; clippy `-D warnings` with
  `java-mods`; the default build.

## Scenario (`/home/umceko/hz-poc/agents/w5/scn-2.json`, a copy of `install-rs`, `HORIZON_JAVA_DEBUG=1`)
```json
{"steps": [
  {"name": "floor"}, {"command": "/forceload add -16 -16 16 16"}, {"wait": 40},
  {"command": "/fill -4 99 -4 4 99 4 stone"}, {"command": "/fill -4 100 -4 4 104 4 air"},
  {"tp": [0, 100, 0, -90, 20]}, {"check": {"near": [0, 100, 0], "radius": 2}},
  {"command": "/gamemode survival"}, {"command": "/give @s minecraft:oak_log 8"}, {"wait": 200},
  {"name": "open"}, {"key": "key.inventory"}, {"wait": 40},
  {"check": {"log": "Java shell 1 opened", "present": true, "within": 5}},
  {"check": {"no_exceptions": true}}, {"screenshot": "inventory"}, {"close": true}, {"wait": 20}
]}
```
(Rename the Rust debug line to `Horizon: Java shell {kind} opened`/`closed`.) One run; report the JSON and the screenshot.

## User steps (`install-rs`, survival)
SPEC-1's nine checks, unchanged results. Also: `/effect give @s speed 60`, the effects column still shows left of the
inventory; open the recipe book: JEI stays right of it.
