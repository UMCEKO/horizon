# SPEC-8 (W8): text recorded glyph by glyph while a mod patches `PreparedTextBuilder`

Read first: `CLAUDE.md`, RULES.md, DESIGN §1.2 G4 and D4, SPEC-2 (per-font glyph sources), `B/Recorder.java` (`addText`, the
entry kinds, `pose`, `scissor`), `B/ClientLoader.java` (`glyph`, `font`), `B/Routing.java` (`route`), `G/java_screens.rs`
`paint_frame` (the TEXT branch and how it draws one glyph with the font and atlas). Vanilla: `Font.PreparedTextBuilder`
(`accept`, `getShadowColor` lines ~350–358, `visit`), `GuiTextRenderState.ensurePrepared`, `BakedGlyph.createGlyph`,
`EffectGlyph.createEffect`, `TextRenderable`, `Font.GlyphVisitor`.
Jade's patch (javap constant pool, `snownee.jade.mixin.PreparedTextBuilderMixin`): `@Inject accept(ILStyle;BakedGlyph;)Z`
(cancels a glyph when `JadeFont.isFilteredGlyph`), `@WrapOperation getShadowColor → ARGB.scaleRGB(IF)I` (theme alpha, light
scheme); both act only when the builder's font is a `JadeFont`.

Depends on: SPEC-2 (Java's fonts know every font id; this spec reuses its glyph sources).

Code comments: one line at most.

## Goal

Text drawn by Java (mod screens, HUD layers, Jade's overlay, tooltips) reaches native as TEXT runs that native lays out
itself. While a loaded mod patches `Font$PreparedTextBuilder` (W12 `routes.bin` has `…Font$PreparedTextBuilder#accept…` or
`#getShadowColor…` on), Java instead runs vanilla's real `prepareText` for each text and records its output glyph by glyph:
position, colour and shadow colour per glyph as the (patched) builder computed them, plus underline, strikethrough and
background effects. Native draws exactly those glyphs. Without such a patch nothing changes (D4).

Also: TEXT runs carry the style's own shadow colour (`Style.getShadowColor()`), which they drop today.

Accept: Jade's overlay text shadow matches Forge in Jade's default and light themes and while the overlay fades.

## Wire (inside a GUI recording)

- TEXT run (existing): after `text font`, add `bool hasShadowColor` + `int shadowColor` (ARGB). Native uses it for that run's
  shadow with vanilla's rule (`getShadowColor` above: the text alpha times the shadow's alpha when the text is translucent).
- New entry kind **5** `GLYPHS`: `pose` (as TEXT), `int n` x glyph (`int codepoint`, `text font`, `float x`, `float y`,
  `int color`, `int shadowColor` (0: no shadow), `byte flags` (1 bold, 2 italic), `float boldOffset`, `float shadowOffset`),
  `int m` x effect (`float x0`, `float y0`, `float x1`, `float y1`, `int color`, `int shadowColor`, `float shadowOffset`),
  then scissor. Coordinates are the builder's (text space, before `pose`).

## Java

- `B/Glyphs.java` (new):
  - `static boolean routed`: decided once the routes load: any `Routing.route(key) >= 0` for the `Font$PreparedTextBuilder`
    keys (copy them from `java_routing.py report`). Log `Horizon: text recorded as glyphs (<keys>)` when true.
  - `record Glyph(int codepoint, Identifier font, float x, float y, int color, int shadowColor, Style style, float boldOffset, float shadowOffset) implements TextRenderable.Styled`
    (`left/top/right/bottom` from `x`, `y` and the glyph's advance; `render`, `renderType`, `textureView`, `guiPipeline` throw:
    nothing in Java draws them).
  - `record Effect(float x0, float y0, float x1, float y1, int color, int shadowColor, float shadowOffset) implements TextRenderable`
    (same).
- `B/ClientLoader.java`: the glyph sources of SPEC-2 return a `BakedGlyph` whose `createGlyph(x, y, color, shadow, style, bold, offset)`
  returns a `Glyphs.Glyph` with its own code point and font id; the provider's `effect()` returns an `EffectGlyph` making
  `Glyphs.Effect`. (Today `createGlyph` returns null and `effect()` null; nothing in Java calls them outside this spec, so
  measuring is unchanged.)
- `B/Recorder.java` `addText`: when `Glyphs.routed`, write `GLYPHS` instead of TEXT: `state.ensurePrepared().visit(visitor)`
  with a `Font.GlyphVisitor` that collects `acceptGlyph` (a `Glyphs.Glyph`) and `acceptEffect` (a `Glyphs.Effect`; the
  background effect comes first, as `visit` emits it). Otherwise TEXT as today plus the run's shadow colour.

## Rust (`G/java_screens.rs`)

- TEXT: read the two new fields per run; a run with its own shadow colour draws its shadow in that colour (with the alpha rule).
- `GLYPHS`: for each glyph, with the pose applied: if `shadowColor`'s alpha is not 0, draw the glyph at `(x + shadowOffset,
  y + shadowOffset)` in `shadowColor`, and its bold copy at `+ boldOffset`; then the glyph at `(x, y)` in `color` and its bold
  copy. Italic uses the same shear as native's TEXT path. Glyphs come from `font.glyph_in(font, char)` as TEXT's do. Effects:
  their shadow rectangle (offset by `shadowOffset`) then the rectangle, in vanilla's order (`visit`: background, glyphs,
  effects). Reuse the TEXT path's per-glyph quad code; do not duplicate it (extract a function both call).
- Mark the GLYPHS painter with `gui_route!` for both `PreparedTextBuilder` keys; remove them from `deferred.tsv`.

## Tests
- Java unit `U/GlyphRecordCheck.java` (tables as `FontTablesCheck`: `'A' = 6`, `'b' = 5`, missing 6), with `Glyphs.routed` forced:
  record `GuiTextRenderState` for "Ab", colour `0xFFFF0000`, shadow on: two glyphs at x = 0 and 6 (the advances), shadow colour
  of each = `ARGB.scaleRGB(0xFFFF0000, 0.25F)` (vanilla's own function as the oracle); bold: second glyph at x = 7
  (`getAdvance(true)` = advance + bold offset 1, vanilla `GlyphInfo`); underlined: one effect per glyph from `x0` (`x - 1` for the first glyph, else `x`) to `x + advance`, at
  `y + 8 .. y + 9` (vanilla `accept`).
- Test mod mixin (`TM/mixin/ShadowRouteMixin.java`): `@WrapOperation` on `Font$PreparedTextBuilder#getShadowColor` →
  `ARGB.scaleRGB` returning `0xFF00FF00` when the builder's font is the test mod's `Font` subclass. A test screen draws text with
  that font; `bridges` check `glyph_shadow_from_patch`: the recording's GLYPHS entry has shadow colour `0xFF00FF00` for it and
  vanilla's for text in the plain font.
- Rust: GLYPHS and the extended TEXT parse; a glyph with shadow 0 draws one quad (two when bold), with a shadow two (four).
- `java_mods.py build` (private dir), `unit`, `check`, `bridges`; `cargo test -p horizon-client --features java-mods --lib java_screens`;
  clippy with and without the feature; `python3 tools/java_routing.py lint`.

## Done bar
Tests pass; without the routes (install-rs has Jade: it is routed there too; check with a pack without Jade, e.g. the test mod
alone) TEXT is still used and the W5 screen checks are unchanged.

## Live test (lead; install-rs, `HORIZON_JAVA_DEBUG=1`)
Scenario `tools/scenarios/w8_glyphs.json`: floor, creative, `/setblock 0 100 3 minecraft:furnace`, `{"lookat": [0.5, 100.5, 3.5]}`,
`{"wait": 40}`, `{"screenshot": "jade"}`; checks `log` `Horizon: text recorded as glyphs`, `no_exceptions`.
User steps: look at blocks with Jade's overlay in its default theme, then switch Jade to a light theme (Jade's config screen):
the overlay text and its shadow look as on Forge (no dark halo on light backgrounds); look away: the text fades out with the
box, shadow included.
