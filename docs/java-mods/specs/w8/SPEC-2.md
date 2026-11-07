# SPEC-2 (W8): Java measures each font with native's advances for that font

Read first: `CLAUDE.md`, RULES.md, DESIGN §1.2 G3, `B/ClientLoader.java` (`advances`, `glyph`, `font`, `language`),
`B/ClientBridge.java` `font` (op `client_font`), `G/java_screens.rs` `java_client_font`, `trimmed_advances`,
`G/java_components.rs` `prepare_font`, `R/font.rs` (`Font::load`, `advances`, `provider_advance`, `glyph_of`, `epoch`, the
`uniform` and `japanese` options, `missing`). Vanilla: `FontManager.CachedFontProvider.glyphs` (lines ~341–347: `Resource` →
its font set, `AtlasSprite`/`PlayerSprite` → sprite glyphs, else the missing font set), `AtlasGlyphProvider`/`PlayerGlyphProvider`
(`GlyphInfo.simple(8.0F)`), `Minecraft.selectMainFont`.

Depends on: none.

Code comments: one line at most.

## Goal

Every `Font` in the Java client measures text in the font its style names, with the advances native draws that font with:
`minecraft:default`, `minecraft:uniform`, `minecraft:alt`, `minecraft:illageralt`, `minecraft:include/*` and any pack's or
mod's own fonts. A font native does not have measures as vanilla's missing font set (every glyph the missing glyph). Sprite
fonts measure 8 per glyph (vanilla). The tables are boot data and are sent again only when native's fonts change (a resource
reload, Force Unicode Font, the Japanese glyph variants option, the language).

Accept: GuideME's text styled `minecraft:uniform` (AE2's guide, install-rs) wraps where native draws it; Waystones' warp
plate name in `minecraft:alt` fits its box.

## Wire: `client_font` (replaces today's format)

`text language`, `float missing` (native's missing glyph advance), `int fonts`, then per font: `text id`, `int n`, `n` x
`float` (advance by code point, trimmed as `trimmed_advances` does: the table ends at the last advance that differs from
`missing`). `minecraft:default` first. Reply `byte 1`.

## Rust

- `R/font.rs`:
  - `pub fn ids(&self) -> Vec<String>`: the loaded font sets' ids (the `sets` of the cache), default first.
  - `pub fn advances_in(&self, id: &str) -> Vec<f32>`: `advances()` generalised to any set (`advances()` becomes
    `advances_in(DEFAULT)`); an unknown id is empty.
  - `pub fn missing_advance(&self) -> f32` (`self.missing.advance`).
  - `pub fn measure_key(&self) -> u64`: changes when advances may change and not on a glyph-cache reset: a `load_id` field set
    once in `Font::load` (the `next_generation()` value at construction, kept), combined with `uniform` and `japanese` as
    `epoch` does.
- `G/java_screens.rs` `java_client_font`:
  - Writes the new format for `font.ids()` (each through `trimmed_advances`, which now takes the missing advance).
  - `CLIENT.font: bool` becomes `font_sent: Option<(u64, String)>` (measure key, language); send when it differs from
    `(measure_key(), settings.language)`. The first send stays where it is (`prepare_font`, before the Java client starts);
    a later one is sent from `java_client_tick_send`, before the tick request, through `host.request("client_font", …)`
    directly (the client is running).
- Log once per send with `HORIZON_JAVA_DEBUG`: `Horizon: fonts for Java: <n> fonts, <bytes> bytes`.

## Java (`B/ClientLoader.java`, `B/ClientBridge.java`)

- `ClientLoader`: `advances` becomes `volatile Map<Identifier, float[]> fonts` plus `float missing`; `GLYPHS` becomes a map
  keyed by `(Identifier, codepoint)` (or one `ConcurrentHashMap<Integer, BakedGlyph>` per font id), cleared when tables
  arrive.
- `font()`'s `Font.Provider.glyphs(FontDescription description)`:
  - `FontDescription.Resource r` → a `GlyphSource` over `fonts.get(r.id())`; an id with no table → the missing source;
  - `FontDescription.AtlasSprite` / `PlayerSprite` → a source whose every glyph advances `8.0F` (vanilla's `GLYPH_INFO`);
  - anything else → the missing source (every glyph `missing`).
  - A code point past a table's end advances `missing`.
- `getRandomGlyph(random, width)` (obfuscated text) returns the glyph of a code point of the same font whose advance equals
  the requested width, chosen with the given `random` among those code points in ascending order (vanilla `FontSet` picks
  among glyphs of that width); with none, the same glyph as now. Keep the code-point lists per font lazily built.
- `ClientBridge.font(in)` reads the new format; `ClientLoader.language(text)` as today.

## Tests

- Java unit `U/FontTablesCheck.java` (no GPU, as `HudAnimationCheck`): feed tables `default: 'a' = 6`, `minecraft:uniform:
  'a' = 4`, `missing = 6`; then with `ClientLoader.font()` (expose it package-private for the check):
  - `width(Component.literal("aa"))` = 12;
  - the same text with `Style.EMPTY.withFont(new FontDescription.Resource(Identifier.withDefaultNamespace("uniform")))` = 8;
  - with `Identifier.parse("nomod:nofont")` = 12 (missing 6 each);
  - with `new FontDescription.AtlasSprite(...)` on one `￼` = 8;
  - mixed runs ("a" default + "a" uniform) = 10.
- Rust: `advances_in("minecraft:alt")` on the test font root of `R/font.rs` (its existing `glyph_in("alt", 'b').advance == 3.0`
  fixture) has `[b] == 3.0`; `measure_key` changes after setting `uniform` and does not change after `reset_cache`; the wire
  bytes for two fonts parse back (write a reader in the test).
- `java_mods.py build` (private `HORIZON_JAVA_MODS_DIR`), `unit`, `check`; `cargo test -p horizon-client --features java-mods --lib java_screens`,
  `cargo test -p horizon-client-render font`; clippy with and without `java-mods`.

## Done bar
Tests pass; `java_mods.py check` reports no new problem; one `hz_scenario.py` run of the scenario below shows the log line
with `fonts` ≥ 4 and no exceptions.

## Live test (lead; install-rs)
Scenario `tools/scenarios/w8_fonts.json`: creative, floor, `{"wait": 100}`, checks `log` `Horizon: fonts for Java: \d+ fonts`,
`no_exceptions`. Run with `HORIZON_JAVA_DEBUG=1`. Then toggle Force Unicode Font (`horizon-ctl options font` and the
toggle, or the options file key `forceUnicodeFont:true` and a restart): the log line appears again.
User steps (install-rs): open AE2's guide (GuideME: hold the guide key over an AE2 item, or `/ae2guide` if listed by the
mod's docs); pages with code or small text (GuideME's `uniform` style) wrap inside the page and nothing overlaps.
Showcase: place a Waystones warp plate, look at its name in the alt font: it stays inside its label.
