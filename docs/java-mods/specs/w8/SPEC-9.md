# SPEC-9 (W8, last; see DESIGN Q3): edge patches of the F3 overlay and the debug keys

Read first: `CLAUDE.md`, RULES.md, DESIGN §1.2 G11, D7 and Q3, SPEC-4, W5 `SPEC-7.md` §1 (GUI bridges: `RES/gui_bridges.tsv`,
`GuiRoutes.backed/body`, the edge stub) and §4 (handlers on input), W5 `SPEC-9.md` (HUD segments), `B/mixin/DebugKeysMixin.java`,
`B/HudLayers.java`, `B/ClientBridge.java` (`hud`, `status`), `G/debug_keys.rs` (`tick_debug_keys`, the F3 combinations),
`G/debug_lines.rs`, `G/java_screens.rs` (`draw_java_hud`/the HUD segments of W5 SPEC-9). Vanilla:
`DebugScreenOverlay.extractRenderState`, `KeyboardHandler.handleDebugKeys`, `KeyboardHandler.keyPress` (how its result is used).
Patches (javap constant pools): balm `DebugScreenOverlayMixin` (HEAD: if `RENDER_GUI_DEBUG_PRE`'s `shouldRender` is false,
cancel; TAIL: `RENDER_GUI_DEBUG_POST.afterRender`), GlitchCore `MixinKeyboardHandler` (on `handleDebugKeys`'s return: fires
`glitchcore.event.client.InputEvent$Key` and may set the return value from its `getHandledDebugKey`).

Depends on: SPEC-4; W5 SPEC-7 §1 (GUI bridges) and W5 SPEC-9 (HUD segment cut). If DESIGN Q3 is answered "inert: no consumer",
this spec is replaced by two `deferred.tsv` rows with that reason.

Code comments: one line at most.

## Goal

Only edge patches are served (both packs' are edges). While a loaded mod edge-patches:
1. `DebugScreenOverlay#extractRenderState(GuiGraphicsExtractor)`: when native shows F3, Java runs the merged method in the HUD
   recording with its body cut out (native draws F3 there): HEAD handlers' drawing lands before native's F3, TAIL handlers'
   after; a HEAD cancel means native does not draw F3 this frame.
2. `KeyboardHandler#handleDebugKeys(KeyEvent)`: native handles an F3+key combination as today; then Java runs the mods'
   RETURN/TAIL handlers with native's result as the return value; if a handler changes it from false to true, native treats the
   key as handled (vanilla `keyPress` uses the result: F3 is then not toggled on release).
A body patch of either is reported at boot as not routed.

## Java

- `RES/gui_bridges.tsv`: rows `net/minecraft/client/gui/components/DebugScreenOverlay#extractRenderState(Lnet/minecraft/client/gui/GuiGraphicsExtractor;)V \t f3`
  and `net/minecraft/client/KeyboardHandler#handleDebugKeys(Lnet/minecraft/client/input/KeyEvent;)Z \t debug_keys`.
- `GuiRoutes.body("f3", …)`: the HUD recorder's cut (W5 SPEC-9's segment mechanism) and "body ran" = true.
- `B/HudLayers.java` (or `ClientBridge.hud`): when F3 is shown (the tick input of SPEC-4 carries the overlay flag: add
  `bool overlay` to `DebugLines`) and the F3 key is on, call `minecraft.getDebugOverlay().extractRenderState(graphics)` once
  per HUD recording, after the vanilla layers where vanilla draws it (`Gui.extractRenderState` order: the debug overlay comes
  after the HUD layers); record `f3Ran` (false when a HEAD handler cancelled).
- `DebugKeysMixin`: keep the HEAD cancel for vanilla's body. Op `client_debug_key` (synchronous, on the input path, only when
  the key is on): `text key name`, `int keycode`, `int modifiers`, `bool nativeResult`; Java runs the merged RETURN/TAIL
  handlers of `handleDebugKeys` on a `CallbackInfoReturnable<Boolean>` holding `nativeResult` (through the W12 edge stub with
  `GuiRoutes.body("debug_keys")` returning `nativeResult`); reply `bool result`.

## Rust

- HUD: two segments around native's F3 (`before f3`, `after f3`) and a `f3Skipped` bit in the HUD status (W5 SPEC-9 wire:
  append after its masks). `G/debug_lines.rs` / the F3 drawing call skips F3 when the bit is set, paints the two segments around it.
  Mark with `gui_route!` for the overlay key.
- `G/debug_keys.rs`: after native handled an F3+key (`tick_debug_keys`), when the key is on, call `client_debug_key` and use its
  result as "handled". Mark with `gui_route!`.
- Remove both keys from `deferred.tsv`.

## Tests
- Test mod mixins (`TM/mixin/DebugEdgeMixin.java`): a TAIL inject on `DebugScreenOverlay#extractRenderState` filling a 4x4 quad
  at (0, 0); a RETURN inject on `KeyboardHandler#handleDebugKeys` that sets the return to true for F3+J. `bridges` checks:
  `f3_tail_drawn` (the HUD recording's after-F3 segment holds one quad while F3 is shown, none when hidden), `debug_key_result`
  (the op answers true for J and native's value for other keys).
- Rust: the HUD status with `f3Skipped` set skips native's F3; the debug key result overrides `handled` only when true.
- `java_mods.py build` (private dir), `unit`, `check`, `bridges`; `cargo test -p horizon-client --features java-mods --lib java_screens debug_`;
  `python3 tools/parity.py hud` through `verify/parity.sh`; clippy with and without the feature; `java_routing.py lint`.

## Done bar
Tests pass; with neither mod loaded, no `client_debug_key` request and no F3 segments (profile and status show none).

## Live test (lead; showcase install)
Boot log: the two keys listed as served. Scenario: floor, `{"key": "f3"}`, `{"wait": 20}`, `{"screenshot": "f3"}`, checks
`no_exceptions`. F3 looks as without balm (balm draws nothing by itself). No user steps: nothing in the packs consumes these hooks.
