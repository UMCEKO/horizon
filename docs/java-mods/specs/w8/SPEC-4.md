# SPEC-4 (W8): mods' F3 entries on native's debug screen

Read first: `CLAUDE.md`, RULES.md, DESIGN §1.2 G6 and D3, `L/debug_entries.rs` (`ENTRIES`, `Entry`, `Category`, `Profile::statuses`,
`DebugEntries::{status, enabled, any_enabled, toggle_status}`, `search`), `L/debug_overlay.rs` (`Displayer`), `G/debug_lines.rs`
(`debug_entry_lines`), `G/debug_options.rs`, `B/ClientLoader.java` (where it builds `debugEntries`), `B/ClientBridge.java`
(`tick`, `events`, `status`). Vanilla: `DebugScreenEntries` (`allEntries`, `getEntry`, `PROFILES`), `DebugScreenEntry`
(`display`, `isAllowed`, `category`), `DebugEntryCategory` (`label`, `sortKey`), `DebugScreenOverlay.extractRenderState`
(lines ~95–150: the displayer, `getLevel`, `getClientChunk`, `getServerChunk`, `lastPos`, `clearChunkCache`),
`DebugScreenEntryList.rebuildCurrentList` (sorted by `Identifier`'s natural order: path, then namespace). NeoForge:
`RegisterDebugEntriesEvent` (javap: `register`, `includeInProfile`, `validateProfiles`).

Depends on: none.

Code comments: one line at most.

## Goal

Mods register `DebugScreenEntry`s in `RegisterDebugEntriesEvent` (already posted in Horizon's Java client). Make them
behave as on a real client: they are listed (and toggled, and saved in `debug-profile.json`) on native's debug options
screen, their profile defaults apply, and while enabled their `display` lines appear on native's F3 in vanilla's order.

Showcase entries: AppleSkin `FoodStatsDebugEntry`, Nature's Compass, lithostitched, Puzzles Lib (development only).
Accept: AppleSkin's food lines (hunger, saturation, exhaustion) show on F3 and update as the player sprints.

## Wire

- Op `client_debug_entries` (window → Java, once, after the Java client's boot reload has finished, at the same moment the
  window takes the boot's other client results): reply `int n` x (`text id`, `text categoryLabel` (the category's `label` as
  JSON, `ComponentSerialization.CODEC`), `float sortKey`, `bool reducedAllowed` (`isAllowed(true)`), `byte default`,
  `byte performance`) for every id of `DebugScreenEntries.allEntries()` whose namespace is not `minecraft`, in `Identifier`
  order. Status bytes: 0 absent from the profile, 1 `ALWAYS_ON`, 2 `IN_OVERLAY`, 3 `NEVER`, read from
  `DebugScreenEntries.PROFILES` after NeoForge's registration (it merges `includeInProfile` there; check with javap that
  `PROFILES` is the map NeoForge updates, else read the event's `validateProfiles()` result kept by a listener at
  `EventPriority.LOWEST`).
- `Input` kind **42** `DebugLines`: `bool reduced`, `int n` x `text id` (the mod entries native has enabled now, in
  `Identifier` order). Sent each client tick while `n > 0`, and once with `n = 0` when it drops to 0.
- Tick status, appended block `debugLines`: `int n` x (`text id`, `int p` x `text` priority lines, `int l` x `text` lines,
  `int g` x (`text group id`, `int k` x `text`)), for the ids of the last `DebugLines` input, in that order.

## Java (`B/DebugLines.java`, new; `B/ClientBridge.java`)

- `DebugLines.export()` builds the op's reply.
- `DebugLines.wanted(in)` keeps `reduced` and the ids (the `DEBUG_LINES` case of `events`).
- `DebugLines.record()` runs in the tick job right after `minecraft.tick()` (as the HUD recording), only while ids are wanted:
  - the overlay's `lastPos` follows the camera entity's chunk as `extractRenderState` does it (an `@Accessor` mixin
    `mixin/DebugOverlayAccess.java` on `DebugScreenOverlay` for `lastPos`; call `clearChunkCache()` when it changes);
  - `level`, `clientChunk`, `serverChunk` from the overlay's own private getters through `@Invoker`s on the same accessor
    (vanilla's answers on this client: the single-player server's level when the window plays single player);
  - for each id: `DebugScreenEntries.getEntry(id)`; skip when null or `!entry.isAllowed(reduced)`; else
    `GameBridge.safely("DebugScreenEntry " + id, () -> entry.display(displayer, level, clientChunk, serverChunk))` with a
    displayer that records this entry's `addPriorityLine`, `addLine`, `addToGroup` calls in order (group ids as
    `Identifier.toString()`);
  - if `getServerChunk()` throws, or returns null while the window has that chunk, that is a gap of the Java server view
    (`ChunkViews`): report it, do not substitute another chunk.
- `status` appends the block (empty `n = 0` when nothing is wanted).
- Debug log (`HORIZON_JAVA_DEBUG`): `Horizon: F3 entries from mods: <ids>` at export.

## Rust

- `L/debug_entries.rs`:
  - `ModEntry { id: String, category: CategoryKey, reduced: bool, default: Option<Status>, performance: Option<Status> }` and a
    process-wide `set_mod_entries(Vec<ModEntry>, Vec<ModCategory>)` (`OnceLock`/`RwLock`, set once per Java client boot).
    `ModCategory { label_json: String, sort_key: f32 }`. `CategoryKey` orders vanilla `Text` (1.0), `Renderer` (2.0) and mod
    categories by `sort_key` as vanilla does (equal keys: vanilla's first, then label).
  - `Profile::statuses` callers also take the mod entries' defaults (`reset_to_profile` adds `(id, status)` of each mod entry
    with a status for that profile).
  - `enabled(id, overlay, reduced)` and `any_enabled` work on full identifiers for mod entries (`reduced` false or
    `reduced_allowed` true; for mod entries Java re-checks `isAllowed`, so native passes `reduced` along and only filters
    by status).
  - `search` returns vanilla and mod entries (a small enum or an owned view), sorted as today (category, then id).
- `G/debug_options.rs`: lists mod rows with their category headers (the label JSON through `horizon_client::component::to_text`
  or the crate's existing JSON → `Text`), toggles and saves them as vanilla rows.
- `L/debug_overlay.rs` `Displayer::group` takes `impl Into<String>`; a key without `:` means `minecraft:` (vanilla call sites
  stay as they are).
- `G/debug_lines.rs` `debug_entry_lines`: build the list of enabled ids (vanilla paths as `minecraft:<path>` and mod ids),
  sort by `(path, namespace)`, iterate: vanilla ids run today's `match`, mod ids replay the latest recorded calls into `d`
  (priority, line, group) in their order. Lines from Java are plain strings with `§` codes, as vanilla's.
- `G/java_screens.rs`: `Input::DebugLines(bool, &[String])` kind 42; `java_client_tick_send` sends it from the current
  enabled mod ids; the status parser stores the block in `CLIENT.debug_lines: Vec<RecordedEntry>`; the `client_debug_entries`
  request after the boot reload calls `set_mod_entries`.

## Tests
- Java test mod (`TM/ClientDebug.java`, registered from `TestMod` client side): `RegisterDebugEntriesEvent` registers
  `horizon_java_test:probe` (category `SCREEN_TEXT`, in the default profile as `IN_OVERLAY`) whose `display` calls
  `addLine("probe")` and `addToGroup(horizon_java_test:g, "g1")`. The checks in
  `java_mods.py bridges` read the log line printed by a `Checks.check("debug_entry_exported", …)` in `DebugLines.export`'s test
  hook, and `debug_entry_lines_recorded` after one tick with the id wanted.
- Rust (`L/debug_entries.rs`): with a mod entry `appleskin:food_stats` and vanilla `minecraft:fps`, the sorted order is by path:
  `fps` before `food_stats` (vanilla `Identifier.compareTo`: path first); a mod entry with default `IN_OVERLAY` is enabled after
  `load_profile(Default)` with the overlay shown and not without it; `reset_to_profile` writes its status into the saved file
  (round-trip through `parse`).
- Rust (`G/debug_lines.rs`): replaying a recorded entry with one priority line, two lines and one group puts them where
  vanilla's displayer would (priority to the shorter column, lines split after, group kept together): compare with the column
  rules in vanilla `extractRenderState` lines ~150–190, written as expected arrays in the test.
- `java_mods.py build` (private dir), `unit`, `check`, `bridges`; `cargo test -p horizon-client --lib debug_`,
  `cargo test -p horizon-client --features java-mods --lib java_screens`; clippy with and without the feature;
  `python3 tools/parity.py hud` through `verify/parity.sh` (F3 text unchanged without mods).

## Done bar
Tests pass; with no mods the F3 screen and the debug options screen are byte-identical in the `hud` harness; one scenario
run below without exceptions.

## Live test (lead; showcase install)
Scenario `tools/scenarios/w8_f3.json` with `HORIZON_JAVA_DEBUG=1`: floor, survival, `{"key": "f3"}`, `{"wait": 40}`,
`{"screenshot": "f3"}`; checks `log` `Horizon: F3 entries from mods: .*appleskin`, `no_exceptions`. Expected: AppleSkin's
food lines on the left column of the screenshot.
User steps: F3 → the AppleSkin lines are there; sprint: exhaustion rises; the debug options screen (`key.debug.debugOptions`, F3+F6 by default): AppleSkin's, Nature's
Compass's and lithostitched's rows are listed under their categories; turn AppleSkin's to "Never": its lines go; restart: the
choice was kept.
