# SPEC-6 (W5): what mods do through a shell reaches native (clicks, packets, input state, drags) (revision 2)

Read first: as SPEC-4, plus `SPEC-4.md`, vanilla's `AbstractContainerScreen` input fields and handlers (`mouseClicked`,
`mouseDragged`, `mouseReleased`, `slotClicked`), `G/game.rs` `menu_click`, `menu_ops`, `remote_menu_ops`,
`crates/horizon-client/src/container_screen.rs` (`Clicks`). Paths as SPEC-2; `TM/` = `mods/horizon/src/java-mods-host/testmod/`.

Depends on: SPEC-4 (SPEC-5 not needed).

## Goal

Mods act on vanilla screens by calling vanilla: Mouse Tweaks cancels `MouseButtonPressed.Pre`/`MouseDragged.Pre` and calls
`slotClicked` itself, and moves items on `MouseScrolled.Post`; it reads and writes the screen's quick-craft fields through
accessors; Inventory Profiles Next calls `handleContainerInput` from its widgets and its tick. On a shell those calls reach
the Java client's `MultiPlayerGameMode`, which would send them to the **Java** server, which does not own the menu. This spec
sends them to native instead, which performs them as its own (its prediction, its packet), so the server, native's screen and
the Java client agree. It also opens a mod's own screen class registered for a vanilla menu type (SPEC-4 J2's third branch).

Accept: Mouse Tweaks (showcase pack) works on the inventory, creative's inventory tab and every SPEC-4/SPEC-5 screen.

## 1. The outbox: Java's menu packets go to native

### Java (`B/ClientBridge.java` `send`, `B/Shells.java`)

- `Shells.nativeMenu(int id)`: true when `id` is the forwarded menu's id (SPEC-4) or `id == 0` (the inventory menu native owns).
- In `send`, after `sentByWindow`, if the packet is one of these and names a native menu, append it to `Shells.outbox`
  (encoded with the serverbound play codec bound to the client's registries and `ConnectionType.OTHER`, id first), and do
  not send it: `ServerboundContainerClickPacket`, `ServerboundContainerClosePacket`, `ServerboundContainerButtonClickPacket`,
  `ServerboundContainerSlotStateChangedPacket`, `ServerboundSelectBundleItemPacket`, `ServerboundSetCreativeModeSlotPacket`
  (always: it is the inventory), `ServerboundRenameItemPacket`, `ServerboundSelectTradePacket`, `ServerboundSetBeaconPacket`,
  `ServerboundPlaceRecipePacket`. Mod menus (opened by the Java server) are untouched: their packets go where they go today.
- `Shells.echo` (SPEC-4) still drops native's own clicks first.
- The outbox travels: at the end of `ShellReply` (`int n` x `bytes`), at the end of the status (same), and is cleared when written.

### Rust (`G/java_screens.rs`, new `G/java_menu_actions.rs`)

`fn java_perform(&mut self, packets: Vec<Vec<u8>>)`: decode each with `horizon_protocol`'s serverbound play decoder and act
as native's own UI would, in order:

| Packet | Native does |
|---|---|
| `ContainerClick` | `menu_click(menu_slot(open_menu_name(id), slot), button, input)` (outside: `None`); with a flag set so R2 of SPEC-4 does not replay it to Java (Java already predicted it) |
| `SelectBundleItem` | native's bundle selection for that slot (`bundles.rs`), which sends its packet |
| `ContainerClose` | native closes its screen as on Escape (`remote_close_local`, which sends the close) |
| `SetCreativeModeSlot` | set the inventory slot (or throw for -1, `remote_creative_throw`) and send, as `creative_display_click` does |
| `RenameItem` | set native's anvil name box text, then send |
| `ContainerButtonClick`, `SelectTrade`, `SetBeacon`, `PlaceRecipe`, `ContainerSlotStateChanged` | `self.send(packet)`; and set the matching native selection where native keeps one (selected trade, beacon powers, crafter slot state): read each screen's code |

A packet for a menu id native no longer has open is dropped with a debug line. Call `java_perform` right after applying a
`ShellReply` (before acting on the owner) and after applying a status.

## 2. The screen's input state, both ways

Mouse Tweaks reads `isQuickCrafting`, `quickCraftingButton` and calls `setIsQuickCrafting`, `setSkipNextRelease` through
accessors; vanilla's handlers keep the same fields. Native keeps them in `inv_clicks` (`Clicks`) and `inv_drag`.

- **Only when detected (D5/D6):** a loaded mod has an accessor or invoker mixin on the shell's class chain (read at boot from
  Mixin's applied mixins on `AbstractContainerScreen` and every vanilla screen class above it, `ClassInfo.forName(...)
  .getAppliedMixins()`, keeping the ones whose config is a mod's; W12's `Routing.confirm` reads the same list, share the
  reader), or a handler body of that class is routed (SPEC-7). Otherwise none of this section runs. Status bit
  `bool shellStateWanted` (per open shell) tells Rust.
- New `Input::ShellState` (kind 21): every field of vanilla's `AbstractContainerScreen` input state that `Clicks`/`inv_drag`
  model: `isQuickCrafting`, `quickCraftingType`, `quickCraftingButton`, `quickCraftSlots` (menu indices), `skipNextRelease`,
  `doubleclick`, `lastClickButton`, `lastClickSlot` (menu index), `lastClickTime`, `clickedSlot`, `isSplittingStack`,
  `draggingItem`... Read the 26.3 field list and `Clicks`; mirror each one native models, list the rest in the report.
  Sent before each shell input and frame when it changed (as the layout).
- Java writes them with an accessor mixin (`B/mixin/ShellInputAccess.java`) in `apply`, and after an input compares them with
  what it wrote: if a mod changed any, `ShellReply` carries `bool changed` + the same block, and native writes it back into
  `Clicks` before acting (e.g. `skipNextRelease` makes native ignore the next release, as vanilla would).
- Test: native `Clicks` → bytes → Java-side layout of the same order (a Rust round trip of the block), and a Rust test that a
  reply with `isQuickCrafting = false` clears native's drag.

## 3. Drags are input

Vanilla posts `MouseDragged.Pre`/`Post` from `MouseHandler.handleAccumulatedMovement` while a button is down. Today the shell's
moves ride `client_shell_frame`. Only when a mod listens to `ScreenEvent.MouseDragged.Pre` or `.Post` (read with `wanted()`'s
reader at open; status bit `bool dragsHeard`), drags become inputs:
- Rust, `window_event` `CursorMoved`: while `java_shell_open()` and a mouse button is held, send
  `java_shell_input(&[Input::Move(x, y)], false, native_drag_handles)` before native's drag step, where `native_drag_handles`
  mirrors vanilla's `AbstractContainerScreen.mouseDragged` return for this move (read it: a quick craft or a held stack over a
  slot). If the owner is not `Native`, skip native's drag step for this move.
- Java `shellFrame`: do not replay movement (`onMove`, `handleAccumulatedMovement`) while a button is down (the input already
  did); keep it otherwise.

## 4. A mod's own screen class for a vanilla menu type

SPEC-4 J2's third branch (`Shells.isMods(screen.getClass())`): let `setScreen` continue, no shell. The status says a Java
screen is open; native shows `Screen::Java` and keeps its menu open without close side effects (as for JEI's recipe view,
SPEC-1 R1). Forwarded packets keep flowing (same container id), the screen's clicks reach native through the outbox, and its
close (`ServerboundContainerClosePacket` in the outbox) closes native's menu. Remove SPEC-4's log line.

## 5. Native's own Mouse Tweaks extra

`G/tweaks.rs` is off by default. When a shell input's owner is `Event` or `Java`(`Widget`), native's extras do nothing for that
input (they hang off native's handling, which does not run). For the wheel, Mouse Tweaks acts in `Post`, after native answered
`None`: if the player also turned on Horizon's extra, both move items. Decision D9 in DESIGN; implement what the user picks
(default here: nothing, the extra is the player's choice).

## Tests

- Rust: outbox decode → the right native action for a `ContainerClick` on a chest menu (a unit test on `java_perform` with a
  fake open menu, as `native_menus.rs` tests do); a `ContainerClick` from the outbox is not replayed to Java.
- Test mod check (`TM/`, family `ChecksClient` or `ClientKeys.java`): `shell_mod_click_reaches_native`: when the system property
  `horizon.test.shellclick` is set and an `InventoryScreen` shell gets `ScreenEvent.Init.Post`, call
  `minecraft.gameMode.handleContainerInput(0, 36, 0, ContainerInput.QUICK_MOVE, player)` once. `java_mods.py bridges` sets the
  property, gives a stack in hotbar slot 0, presses the inventory key, closes it, and checks the stack moved to the main
  inventory (`data get entity @s Inventory`). Add the name to the `wrong.extend(...)` list.
- `java_mods.py check`, `unit`, `bridges`, `cargo test ... --bins java_`, clippy, default build.

## Scenario

`scn-6.json` on the showcase install (SPEC-10 builds it), `HORIZON_JAVA_DEBUG=1`: SPEC-2's floor, survival,
`/give @s minecraft:cobblestone 64`, `/setblock 2 100 0 minecraft:chest`, open it (`lookat`, `use`), `wait 20`, check
`Java shell 3 opened` and `screen events heard by yalter.mousetweaks`, no exceptions, close. (Drags and wheel need real
input: user steps.)

## User steps (showcase install: Mouse Tweaks loaded; Horizon's own Mouse Tweaks extra off)

1. Chest: pick up 64 cobblestone, right-drag across 10 empty chest slots: one in each (Mouse Tweaks' RMB tweak).
2. Left-drag with cobblestone held over slots holding cobblestone: they are gathered into the hand.
3. Shift + left-drag over inventory slots: each stack moves into the chest.
4. Wheel down over an inventory stack: one item moves into the chest per notch; wheel up over a chest stack: one comes back.
5. Each result stays after closing and reopening the chest (the server agrees). Do 1–4 in the survival inventory with the
   crafting grid, in a furnace (wheel puts coal in the fuel slot) and in creative's inventory tab.
6. A plain click, shift-click, double click and vanilla drag-split behave exactly as without the mod where Mouse Tweaks does
   not act (left-drag with an empty hand, middle click in creative).
