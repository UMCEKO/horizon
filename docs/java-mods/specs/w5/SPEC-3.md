# SPEC-3 (W5): the creative inventory as a shell (revision 2)

Read first: as SPEC-2, plus `SPEC-2.md` and vanilla's `CreativeModeInventoryScreen.java`
(`.local/minecraft/26.3/sources/client/net/minecraft/client/gui/screens/inventory/`). Paths as SPEC-2.

Depends on: SPEC-2.

## Goal

With a mod listening to screen events (JEI), native's creative screen gets a Java shell: a real
`CreativeModeInventoryScreen`, `init()` run, nothing else of its code (D5/D7). JEI's list shows beside every tab and works:
wheel, clicks, R/U over its list **and over creative slots**, its search box. Creative looks and works as before.

Out of scope: tooltip changes (SPEC-8), routed creative methods (SPEC-7).

## What the shell holds (only what detected mods read)

Mods with screen listeners read the screen's geometry, the selected tab's type and the hovered slot's stack (JEI's R/U,
`getHoveredSlot()`). Vanilla's `selectTab`, `scrollTo` and search refresh do **not** run; the shell's `ItemPickerMenu` is not
filled. Instead native sends:
- `ShellLayout` (kind 16, as SPEC-1): `leftPos`, `topPos` (`creative_origin`, floor), `195`, `136`, hovered = `-1` (unused
  for creative; see below).
- **`Input::ShellCreative` (kind 17):** `text tab` (the selected tab's registry id, `Tab.id`; check it is the id
  `CreativeTabs.java` exports, else export it), `int hovered` (0 none, 1 a grid slot, 2 a hotbar slot, 3 an inventory-tab
  slot, 4 the destroy slot), `int index` (grid 0..44 / hotbar 0..8 / the inventory menu's slot index), `bytes stack` (the
  hovered stack as native shows it, its saved form as `java_item_text` keys it; empty when none). Sent with the layout when
  it differs, and at the head of every `client_shell_input`/`client_shell_frame` request when it changed.

Java (`Shells.creative(...)`, applied in `apply` before the layout):
- `selectedTab` (static field, `@Accessor` in `B/mixin/CreativeShellAccess.java`) = the tab with that id from
  `BuiltInRegistries.CREATIVE_MODE_TAB` (unknown id: log once, leave it). A field write, no `selectTab`.
- Hovered: grid → put the stack into the picker menu's container at `index` (the only slot written; clear the one written
  last time) and `hoveredSlot` = that grid slot; hotbar → the picker menu's hotbar slot `45 + index` (the Java inventory
  already holds the real stack); inventory tab → `player.inventoryMenu.getSlot(index)` (real stacks); none → null.
  Read `CreativeModeInventoryScreen`'s constructor/`init` to confirm the picker menu's slot order (45 grid, then 9 hotbar).
- That is a one-slot mirror, only while a shell is open (D6).

## Java

- `Shells.open(2)`: construct the screen as vanilla does where it opens creative (copy the `new CreativeModeInventoryScreen(`
  arguments: player, enabled features, operator tab option and permission), mark `current`, `setScreen`. If `init()` switches
  to `InventoryScreen` (the Java client has no infinite items), refuse the shell: log once
  `Horizon: the Java client is not in creative; no creative shell` and check `Menus.gameModeChanged` sends the client its mode.
- A shell of the other kind still open (E from survival into creative): `close` it first.
- `ShellMixin`: add `CreativeModeInventoryScreen.class` to `value` (its handler overrides and `containerTick`, `removed`).
- Status `int shell` = the open kind.

## Rust

- `java_shell_kind`: 2 when `self.screen == Screen::Creative`. `Shell` gets `kind` (from the status);
  `java_shell_open()` = `kind != 0 && c.shell.kind == self.java_shell_kind()`; the status's screen switch (SPEC-1 R1) uses the kind.
- `fn java_creative_state(&self) -> (String, i32, i32, Vec<u8>)` from native's slot ids (`Creative::tab_slots`,
  `Creative::inventory_slots`, `Creative::DISPLAY`, the trash slot) and `creative_screen.shown(...)` for grid stacks. Unit test per tab type.
- `Input::ShellCreative`, kind 17; `Sent.creative`.
- `draw_creative` in step order (SPEC-2): `java_shell_frame()`, `shell_start`, `BACKGROUND` (fill, unselected tabs, panel,
  scroller, selected tab), `EFFECTS`, `WIDGETS` (search box, page buttons), `LABELS` (tab title), `HIGHLIGHT_BACK`, `SLOTS`,
  `HIGHLIGHT_FRONT`, `CARRIED`, `TOOLTIP` (slot, tab and trash tooltips), `DEFERRED`. Move code only.
- Input, as SPEC-1 R4, in the `Screen::Creative` branches of `window_event`:

| Input | `native_widget` | `native_handles` |
|---|---|---|
| button | over the search box or a page button (`fn creative_widget_hit`, factored from `creative_search_click`) | true |
| wheel | false | the tab scrolls, or a bundle is under the cursor |
| key | false | `fn native_creative_key(code)`: Escape; any key while the search box is focused; the inventory key otherwise; hotbar, drop, pick over a slot; saved-hotbar keys. Mirror `CreativeModeInventoryScreen.keyPressed` |
| char | false | native's search box would take it (focused, or a search tab where vanilla's `charTyped` focuses it) |

## Tests
Rust: `Input::ShellCreative` writes what `events` reads; `java_creative_state` per tab type. `java_mods.py check`, `unit`,
`cargo test ... --bins java_`, clippy, default build.

## Scenario (`scn-3.json`, `HORIZON_JAVA_DEBUG=1`)
SPEC-2's floor, then `/gamemode creative`, `{"wait": 200}`, `key.inventory`, `{"wait": 40}`, checks `Java shell 2 opened`,
`not in creative; no creative shell` absent, no exceptions, screenshot, close; `/gamemode survival`, `key.inventory`,
`Java shell 1 opened`, close.

## User steps (`install-rs`, creative)
1. E: JEI's list right of creative on the first frame; creative unchanged.
2. Click through tabs (a Mekanism tab, the second page): JEI stays.
3. Building Blocks: scroll a few rows, hover a grid item, press R: that exact item's recipes. Esc: back on the same tab and scroll.
4. Search tab: type `piston`, hover the first result, U: uses of a piston.
5. Inventory tab: hover a worn or held item, R.
6. JEI's search box takes typing without closing creative; creative's own search filters as before.
7. Grid, trash slot and middle click work as before.
