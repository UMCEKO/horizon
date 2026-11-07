# SPEC-4 (W5): every vanilla container screen native opens, generically by menu type (revision 2)

Read first: as SPEC-2, plus `SPEC-2.md`, `SPEC-3.md`, DESIGN 2.2, and vanilla's `MenuScreens.java`,
`ClientPacketListener.handleOpenScreen`/`handleContainerContent`/`handleContainerSetSlot`/`handleContainerSetData`/
`handleContainerClose`, `MultiPlayerGameMode.handleContainerInput`, `LocalPlayer.clientSideCloseContainer`. Paths as SPEC-2;
`P/` = `crates/horizon-protocol/src/`.

Depends on: SPEC-2. Independent of SPEC-3.

## Goal

When native opens a vanilla container screen from the server's `OpenScreen` (chests and every `generic_9xN`, dispenser and
dropper, crafter, hopper, shulker box, crafting table, furnace, blast furnace, smoker), the Java client builds the same menu
and screen **from the same server packets**, exactly as vanilla's client does, and holds the screen as a shell when a mod
listens. JEI's list, its click areas (the furnace arrow, the crafting arrow) and its `+` transfer buttons appear on all of
them; R/U work over their slots.

The Java client mirrors a menu **only when a loaded mod is detected to need it** (user, D6): a mod listens to screen events
(a shell will open), or a mod patches a method of that menu type's vanilla classes (`routes.bin`, W12). Otherwise nothing
is forwarded and the Java client shows `NativeScreen` as today. Clicks stay native's; while a menu is mirrored, every
click native makes is replayed through the Java client's real `handleContainerInput` with its packet dropped, so the Java
menu stays right (the server does not resend slots a client predicted correctly) and a patch of that method sees every click.

## When a menu is mirrored (`G/java_screens.rs`)

`fn java_mirrors(&self, menu_type: &str) -> bool`, decided when `OpenScreen` arrives:
- the latest status says a mod listens to screen or tooltip events (`bool screensHeard`, new, after `shellAnimating`:
  `Shells.wanted()` evaluated at the tick; JEI's listeners appear only after join, so it is per tick, not boot); or
- `horizon_java_host::routes()` has an `on` route whose class is one of that menu type's classes: its vanilla screen class
  (`MenuScreens` registrations, e.g. `generic_9x3` → `ContainerScreen`, `crafting` → `CraftingScreen`) and its superclasses up
  to `AbstractContainerScreen`, its menu class and superclasses up to `AbstractContainerMenu`, or the client handlers
  `ClientPacketListener#handleOpenScreen|handleContainerContent|handleContainerSetSlot|handleContainerSetData|handleContainerClose`
  and `MultiPlayerGameMode#handleContainerInput`. Write the menu type → classes table as a `const` with a test that every
  type of this spec's list has a row.
The survival inventory (container 0) is not forwarded (the Java server's view already sends it); its clicks are replayed only
while a shell is open or `MultiPlayerGameMode#handleContainerInput` is routed.

Out of scope: workstations, brewing, beacon, merchant (SPEC-5), mounts (their screen needs the entity in the Java client's
level: W2), the lectern (not a container screen), clicks or packets that mods make (SPEC-6), a mod's own screen class
registered for a vanilla menu type (SPEC-6), recipe transfer (W9/W12, DESIGN D4).

## Wire (inputs in the tick batch and in shell requests; `Input` kinds)

| Kind | Name | Fields | Java does |
|---|---|---|---|
| 18 | `Packet` | `bytes` = packet id (VarInt) + body, as vanilla's play protocol writes it | decodes with vanilla's play codec and runs it on the client's `ClientPacketListener` |
| 19 | `MenuClick` | `int containerId`, `int slot` (menu index, -999 outside), `int button`, `int input` (`ContainerInput` ordinal) | replays native's click (J3) |
| 20 | `ShellBook` | `bool visible` | the open recipe-book screen's `RecipeBookComponent.visible` (kinds 1 and 3) |

Shell kind **3** = the vanilla menu screen of the menu native has open.

## Rust

### R1. Forwarding the menu packets (`G/remote.rs`, `G/java_screens.rs`)

At the top of `handle_packet`, before the `match`, call `self.java_menu_packet(&packet)` (a `java_hook_*`-style pair in
`G/java_hooks.rs`, stub without the feature). It forwards, in arrival order, as `Input::Packet(packet.encode())`
(`P/packets/mod.rs` macro's `encode`; check it writes the id first, else prepend it):
- `OpenScreen` when `java_mirrors(type)` (the Java client then builds what vanilla builds);
- `ContainerSetContent`, `ContainerSetSlot`, `ContainerSetData`, `ContainerClose` whose container id is the mirrored menu's
  (the last forwarded `OpenScreen`'s id, kept in `CLIENT`; cleared by a forwarded close or native's own close);
- `SetCursorItem` while a forwarded menu is open.
Never container id 0: the Java server's view of the player already sends the inventory menu (`Menus.serverTick`).
Test in `P/` (or beside the hook): for each forwarded kind, `decode(encode(p)) == p`, including a slot holding a stack with
a foreign (mod) component (`horizon_protocol::item::foreign`).

### R2. Native's clicks (`G/game.rs` `remote_menu_ops` or the function that sends `S::ContainerClick`)

After native predicts and sends a click on a mirrored menu (or on container 0 under the rule above), queue
`Input::MenuClick(container, menu_slot, button, input)` with the same values it put in the packet (one per packet, in order;
a quick-craft drag is several). Map `horizon_client::menu::Input` to the
`ContainerInput` ordinal by name (read the enum: `PICKUP`, `QUICK_MOVE`, `SWAP`, `CLONE`, `THROW`, `QUICK_CRAFT`,
`PICKUP_ALL`, in its declared order) with a test. The replay runs the real merged method: a mod's patch on it (IPN in the top-50 pack) sees native's clicks.

### R3. Shell kind 3 and its layout

- `java_shell_kind`: 3 when `self.screen == Screen::Inventory`, a server menu is open (`remote.menu` id != 0), its type is
  one of this spec's list, and no station, brewing stand, mount or native mod screen is shown.
- `java_shell_layout` for kind 3: `inventory_origin` (floor), native's width and height for that screen
  (`native_screen_width().unwrap_or(176.0)`, `screen_height()`), hovered = `menu_slot_index(open_menu_name(id), slot)`.
- `Input::ShellBook(visible)` from native's recipe-book state (`recipe_ui`), sent like the layout when it changes, for
  kinds 1 and 3 (crafting table, furnaces, the survival inventory).
- `java_shell_input` and `java_shell_frame` first move every queued input (`c.queue`, `c.queued`) into their own request, in
  order, so Java's menu has native's last click before a mod reads it; the tick then sends what is left.

### R4. Drawing

`draw_inventory` already runs in leaf order (SPEC-2) for these screens. Check each container's own pieces sit in the right
leaf: the furnace's flame and arrow and the hopper's and dispenser's panels in `BACKGROUND`, their titles in `LABELS`.

## Java

### J1. `B/ClientBridge.java`: `case PACKET`

Decode with `GameProtocols.CLIENTBOUND_TEMPLATE` bound to the Java client's registries and **`ConnectionType.OTHER`**
(native's bytes are vanilla's; `Wire.clientbound` uses `NEOFORGE`, so add a second cached `ProtocolInfo` beside it). Then
`Shells.forwarding = true; packet.handle(minecraft.getConnection()); Shells.forwarding = false` on the Render thread
(`ensureRunningOnSameThread` passes there). Wrap in `GameBridge.safely`-style logging: `Horizon: a forwarded <Packet> failed`.

### J2. Adopting the screen vanilla builds (`B/mixin/ShellAdoptMixin.java`, on `Gui`)

`@Inject(method = "setScreen", at = @At("HEAD"), cancellable = true)`: when `Shells.forwarding` and the screen is an
`AbstractContainerScreen` whose menu is `player.containerMenu` (vanilla's `ScreenConstructor.fromPacket` just set it):
- its class is vanilla's (`!Shells.isMods(screen.getClass())`) and `Shells.wanted()` → `Shells.adopt(screen, 3)`
  (`current = screen`, reset `last`, `animating`, `kind = 3`), let `setScreen` continue (`Opening`, `init` and nothing else of
  the screen's code, D5);
- vanilla's class and not wanted (mirrored only for a route) → cancel and `setScreen(new NativeScreen(false))`;
  `player.containerMenu` stays the real menu;
- a mod's class → log once `Horizon: <class> stands for the vanilla screen of <menu type>; SPEC-6 opens it` and handle as
  "not wanted" (SPEC-6 replaces this branch).
`nativeScreen(open, pauses, 3)` then finds the shell or `NativeScreen` already there and opens nothing.

### J3. Replaying native's clicks: `case MENU_CLICK`

If `player.containerMenu.containerId == containerId`: `Shells.echo = true`, then
`minecraft.gameMode.handleContainerInput(containerId, slot, button, ContainerInput.values()[input], player)`, `Shells.echo = false`
(in `finally`). In `ClientBridge.send`, drop a `ServerboundContainerClickPacket` while `Shells.echo` (native already sent it;
add it to `sentByWindow`'s reasoning, one-line comment). A mismatched id is vanilla's own warning; leave it.

### J4. `case SHELL_BOOK` and closing

- `SHELL_BOOK`: if the shell is an `AbstractRecipeBookScreen`, set its component's `visible` through an accessor
  (`B/mixin/RecipeBookAccess.java`: `@Accessor("recipeBookComponent")` on the screen, `@Accessor("visible")` setter on
  `RecipeBookComponent`); never call `setVisible` or `toggleVisibility` (they send the book settings to the server).
  Apply it in `apply(screen)` with the layout.
- `Shells.close` for kind 3 (native closed its screen, it already sent `ContainerClose`): `player.clientSideCloseContainer()`
  (vanilla: inventory menu back, screen null), `current = null`. `NativeScreen` standing for a forwarded menu: the same call.
  A server close arrives as a forwarded `ContainerClose` first and vanilla's handler does it; the following
  `Input::Screen(false)` then finds nothing open.
- Status `int shell` writes `Shells.kind` when the shell is the screen.

## Tests

- Rust: the round trip in R1; the `ContainerInput` mapping; `java_mirrors` with no listeners and no routes is false, with
  `screensHeard` true, and with a fake `routes()` holding `ContainerScreen#extractLabels` true for `generic_9x3` only; `Input::Packet`, `MenuClick`, `ShellBook` write what `events`
  reads; `java_shell_kind` is 3 for a chest menu and 0 for an anvil (station) and a mount.
- Java unit `U/MenuForwardCheck.java`: encode a `ClientboundContainerSetSlotPacket` with vanilla's codec and
  `ConnectionType.OTHER`, decode it with the new `ProtocolInfo`, same fields (registries: the unit's `RegistryAccess` as
  other checks build it; skip with a printed reason if a check cannot bootstrap registries, and say so in the report).
- `java_mods.py check`, `java_mods.py unit`, `cargo test ... --bins java_`, clippy, default build.

## Scenario (`scn-4.json`, `HORIZON_JAVA_DEBUG=1`)

SPEC-2's floor, survival, `/give @s minecraft:oak_log 8`, `{"wait": 200}`, then for each block B in
`chest`, `furnace`, `crafting_table`, `hopper`, `dispenser`, `shulker_box`, `blast_furnace`:
```json
{"command": "/setblock 2 100 0 minecraft:B"}, {"slot": 8}, {"lookat": [2.5, 100.5, 0.5]}, {"use": 1}, {"wait": 20},
{"check": {"log": "Java shell 3 opened", "present": true, "within": 5}},
{"check": {"no_exceptions": true}}, {"screenshot": "B"}, {"close": true}, {"wait": 10},
{"check": {"log": "Java shell 3 closed", "present": true, "within": 5}}
```
(slot 8 must be empty so the click opens the block). Add `{"check": {"log": "a forwarded .* failed", "present": false, "since": "start"}}` at the end. (Every existing install has a screen
listener, Jade's or Mekanism's, so "not mirrored" is covered by the Rust unit test only; add the debug line
`Horizon: menu <type> not mirrored` for the parent's own checks.)

## User steps (`install-rs`, survival, JEI loaded)

1. Open a chest, a furnace, a crafting table, a hopper, a dispenser and a shulker box. Each looks exactly as before and has
   JEI's list on the right from the first frame.
2. Furnace: click the progress arrow: JEI opens the smelting recipes; Esc returns to the furnace with its contents.
3. Crafting table: JEI's `+` buttons show in recipe views opened from it (pressing them is DESIGN D4, not yet).
4. Hover a chest slot holding oak logs, press R: oak log recipes. Move logs around (click, shift-click, drag-split, double
   click); then hover the moved stack's new slot and press R again: still oak logs (the Java menu followed the clicks).
5. Open the recipe book in the crafting table: JEI's bookmarks (left) move aside for it.
6. Another player (or `/item replace block 2 100 0 container.0 with minecraft:diamond`) changes the chest while it is open:
   the slot updates and R over it shows diamond recipes.
