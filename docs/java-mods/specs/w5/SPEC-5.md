# SPEC-5 (W5): workstation, brewing, beacon and merchant screens as shells (revision 2)

Read first: as SPEC-4, plus `SPEC-4.md`, and vanilla's screen classes listed below. Paths as SPEC-2.

Depends on: SPEC-4 (forwarding, kind 3, click replay).

## Goal

Shell kind 3 also covers the menus native draws through `draw_station`, `draw_beacon` and `draw_merchant`: anvil, smithing
table, grindstone, stonecutter, loom, cartography table, enchanting table, brewing stand, beacon, villager and wandering
trader. JEI's list, click areas and R/U work on each; every screen looks and works as before.

## The screens (26.3): their steps and what native draws

No vanilla code of these screens runs in Java except `init()` (D5). What this spec needs from each class is the vanilla
method behind each SPEC-2 step, so a routed method (SPEC-7) is called at the right step, and its native counterpart. Read each
class's overrides of `extractRenderState`, `extractContents`, `extractBackground`, `extractLabels`, `extractTooltip`, and write
the table into `B/ShellLeaves.java` as `static String[] methods(Class<?> screen, int leaf)` (route keys with descriptors, as
`routes.bin` writes them; most classes inherit the generic ones):

| Class (menu type) | Steps with a class-specific method | Vanilla widgets made by `init` (native's) | Native |
|---|---|---|---|
| `AnvilScreen`, `SmithingScreen` via `ItemCombinerScreen` | `BACKGROUND`, `LABELS` (anvil cost); `EXTRA`: what `ItemCombinerScreen.extractRenderState`/`AnvilScreen` draw outside the common steps (the name box is a widget) | anvil name `EditBox` | `draw_station` anvil, smithing |
| `GrindstoneScreen`, `CartographyTableScreen`, `LoomScreen`, `StonecutterScreen` | `BACKGROUND` (patterns, recipes, map), `TOOLTIP` (their `extractTooltip` overrides) | list them | `draw_station` branches |
| `EnchantmentScreen` | `BACKGROUND` (book, options), `EXTRA`: the clue tooltips set in `extractRenderState` (a `TOOLTIP`-like step: put them in `TOOLTIP`) | none | `draw_station` enchanting |
| `BrewingStandScreen` | `BACKGROUND` | none | `draw_station` brewing |
| `BeaconScreen` | `BACKGROUND`, `LABELS` | power and confirm buttons | `draw_beacon` |
| `MerchantScreen` | `EXTRA`: `extractContents`' own part after `super` (offers, arrows, scroller, progress), offer tooltips in `TOOLTIP` | trade buttons | `draw_merchant` |

Add each class declaring handler bodies (`mouseClicked`, `keyPressed`, `charTyped`, `mouseScrolled`, `mouseDragged`,
`containerTick`) to `ShellMixin`'s `value` list; `containerTick` of a shell stays cancelled (native owns it).
Add the menu types to SPEC-4's `java_mirrors` table (menu type → screen class, menu class).

## Rust

### R1. Kind 3 for these screens

`java_shell_kind`: 3 also when `self.station` is one of the kinds above, `brewing_open()`, or the merchant, as long as a server
menu is open (`remote.menu` id != 0). Layout: `inventory_origin`, the station's width and height (vanilla's `imageWidth`,
`imageHeight` for that screen: the merchant is 276 x 166, the others 176 x 166 unless the source says otherwise; take them from
native's existing constants), hovered = `menu_slot_index(open_menu_name(id), slot)` (`menu_slot` already maps stations).

### R2. Forwarding

Add `MerchantOffers` for the mirrored menu id to SPEC-4's forwarded kinds (vanilla's `handleMerchantOffers` fills the client
menu). `ContainerSetData` already carries the beacon's, enchanting table's, brewing stand's, furnace's and loom's values.

### R3. Drawing in leaf order

`draw_station`, `draw_beacon`, `draw_merchant`: `java_shell_frame()`, `shell_start`, then `shell_leaf` around `BACKGROUND`
(fill, panel, progress, book, preview), `WIDGETS` (name box, beacon buttons, trade buttons), `LABELS`, `SLOTS`,
`HIGHLIGHT_FRONT`, `EXTRA` (merchant), `CARRIED`, `TOOLTIP` (the `tooltip`/`tooltip_stack` drawing at the end), `DEFERRED`.
Move code only.

### R4. Input

As SPEC-1 R4 and SPEC-3 R3, for these screens' input branches in `window_event`:
- `native_widget`: the cursor is over the anvil name box, a beacon button, a trade button or the merchant scroller (factor
  each existing hit test into `fn station_widget_hit(&self) -> bool`).
- `native_handles` for buttons: true. For the wheel: the merchant list or stonecutter/loom list scrolls, or a bundle is under
  the cursor. For keys: `fn native_station_key(&self, code) -> bool`, mirroring each class's `keyPressed` (the anvil's name box
  takes every key while focused; Escape; inventory key; hotbar, drop and pick over a slot). For chars: the anvil name box is focused.

## Tests

- Rust: `java_shell_kind` is 3 for each station kind with a server menu and 0 without; layout widths per kind; a
  `MerchantOffers` round trip through `encode`/`decode` with two offers.
- Java unit `U/ShellLeavesCheck.java`: `methods(MerchantScreen.class, EXTRA)` names `MerchantScreen#extractContents`;
  `methods(FurnaceScreen.class, BACKGROUND)` resolves to the class that declares it.
- `java_mods.py check`, `unit`, `cargo test ... --bins java_`, clippy, default build.

## Scenario (`scn-5.json`, `HORIZON_JAVA_DEBUG=1`)

As SPEC-4's loop, for `anvil`, `smithing_table`, `grindstone`, `stonecutter`, `loom`, `cartography_table`,
`enchanting_table`, `brewing_stand`, `beacon` (`lookat`, `use`, check `Java shell 3 opened`, no exceptions, screenshot,
close). For the merchant:
`{"command": "/summon minecraft:villager 2 100 0 {VillagerData:{profession:\"minecraft:librarian\",level:2},NoAI:1b}"}`,
`{"lookat": [2.5, 101, 0.5]}`, `{"use": 1}`, then the same checks.

## User steps (`install-rs`, survival, JEI loaded)

1. Open each screen above. Each looks exactly as before (anvil cost text, enchanting book and runes, beacon buttons, the
   villager's offer list and its tooltips) with JEI's list on the right.
2. Anvil: click the name box and type `stone`: the name changes, JEI's list does not filter, the anvil stays open.
3. Villager: scroll the offer list with the wheel; click an offer; trade. Hover an offer's result and press R: its recipes.
4. Stonecutter: put stone in, scroll and pick a recipe. Enchanting table: hover the three options (clue tooltips).
5. JEI's click areas: the brewing stand's bubbles and the anvil's arrow open their JEI categories.
