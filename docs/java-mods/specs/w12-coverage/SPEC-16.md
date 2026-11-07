# SPEC-16: vanilla menus on the server's Java side (the `menu` receiver family)

Design: `DESIGN.md` §2.5, §2.6 row 16. Needs SPEC-2 (player groups, the player block in replies), SPEC-3 (`Recv::Player` projected to `containerMenu`). Size: ~450 changed lines.

Paths as SPEC-2. Read first: `B/Menus.java` (mod menus: how the real server menu is made and driven today), `B/BlockEntities.java` (vanilla block entities already live in Java: `get` fetches Horizon's record, `refresh` reads a newer one in place, `changed` writes through), `G/java_mods.rs` `java_push_stale`; native menus: `crates/horizon-client/src/menu.rs` (`AbstractContainerMenu.clicked`'s port), `G/../game.rs` `fn menu_ops`, `fn click_inventory`, `G/stations.rs`; vanilla `AbstractContainerMenu.java`, `MenuProvider`, `BlockBehaviour.getMenuProvider`, `ChestMenu`, `AnvilMenu`, `CraftingMenu`, `ItemCombinerMenu`.

Comments in code: one line at most.

## Why
`AbstractContainerMenu#doClick` (Refined Storage HEAD), `AnvilMenu#createResultInternal` (AE2), `ItemStack#overrideOtherStackedOnMe` (Travelers Backpack, called from `doClick` with the menu's `Slot` and `SlotAccess`) all run on the server's menu. For a vanilla menu that native opened (a chest, an anvil, a crafting table) the Java player view has no menu: `player.containerMenu` is its inventory menu. These routes need the real vanilla menu object in Java, holding what native's menu holds.

## 1. When a menu view exists
At native's open of a vanilla menu for a player (where native sends `OpenScreen`: find the server-side open in `G/` for containers, block menus and stations), when `horizon_java_host::routes()` has an `on` route whose owner is in the menu's class chain (its menu class up to `AbstractContainerMenu`) or `ItemStack#overrideOtherStackedOnMe`, call op `menu_open` (new):
```
SPEC-2 player fill, int container_id, text menu_type, byte source (0 block entity at pos, 1 block at pos without entity, 2 none), long pos
```
Java (`B/MenuViews.java`, new):
- source 0: `BlockEntities.get(level, pos)` is the vanilla block entity view; `menu = ((MenuProvider) be).createMenu(id, player.getInventory(), player)`;
- source 1: `state.getMenuProvider(level, pos).createMenu(id, inventory, player)` (anvil, crafting table, grindstone, stonecutter, loom, smithing table, cartography table, enchanting table);
- source 2 (menus native opens without a block, if any in the list): report and skip that type in this spec.
- `player.containerMenu = menu` raw (no `openMenu`: native already sent `OpenScreen`; no `ContainerOpenEvent` either, since native's open posts none today: that's W13's, note it), `menu.addSlotListener`/`setSynchronizer` left unset (native syncs the client).
- Mark the menu backed (`Backed`, or an identity set in `MenuViews`).
At native's close (`ContainerClose` or a forced close): op `menu_close` → `player.containerMenu = player.inventoryMenu` raw; `menu.removed(player)` only if vanilla's close path would call it for this menu (it does: `doCloseContainer`) — then items in crafting grids go back as vanilla does, but natively: so **don't** call `removed` in Java; native already returns them. Write that down in a one-line comment.

## 2. The menu group (fill and reply)
Menus own state besides the player's inventory and the block entity: the carried stack, menu-owned containers (crafting grid, anvil inputs and result, grindstone, stonecutter input and result…), and data slots (anvil cost, enchanting seeds).
- SPEC-2's fill gets bit `MENU` (256): `int container_id`, `bytes carried`, `int n`, n × (`int menu_slot`, `bytes stack`) for menu slots whose container is **not** the player inventory and **not** a block entity (those come through SPEC-2 and `BlockEntities`), `int m`, m × (`int data_slot`, `int value`). Delta as SPEC-2: Rust keeps what it last sent per player and menu id; FULL on `menu_open`.
- Java applies raw: `menu.setCarried`, `slot.container.setItem(slot.getContainerSlot(), stack)` on the menu-owned containers **through their raw content list** (an accessor on `SimpleContainer.items`, `TransientCraftingContainer.items`, `ResultContainer.itemStacks`), so no `slotsChanged` → `createResult` cascade runs that native didn't run; `DataSlot.set`.
- Reply: after SPEC-2's player block, a menu block in the same layout with what Java changed (diff against what it was last filled with). Rust writes it to native's menu (carried, menu slots, data slots) through the paths native's own click code uses.
- Block-entity slots changed by Java go through `BlockEntities.changed` (write-through, exists).

## 3. Native's menu as the body
For edge routes on menu methods (SPEC-17), the native body is native's click or update code on native's menu; after it, the menu group is re-sent before Java's RETURN handlers run (the body bridge does a SPEC-2 `refresh` including `MENU`).

## 4. Tests
- Rust: `menu_group_delta` (carried changes only → `MENU` with `n = 0`, `m = 0` and the carried bytes); `menu_open_is_full`.
- Test mod (no mixin needed): a server-side `PlayerTickEvent` listener that, when `player.containerMenu instanceof AnvilMenu`, records `getCost()`-free state: slot 0's item and the carried stack.
- `ChecksRouting`: `anvil_view_matches`: the session opens an anvil (`/setblock` an anvil, `interact` it), puts a sword in slot 0 (a native click), and the listener sees `AnvilMenu` with the sword in slot 0 and an empty carried stack; `chest_view_is_the_block_entity`: opening a chest gives a `ChestMenu` whose container is the `BlockEntities` view of that chest (identity); `menu_view_closed`: after closing, `containerMenu == inventoryMenu`.
- The menu views exist only with a route: a boot of the play install without one runs no `menu_open` (profile).
- `lan.sh chest` (unchanged), `python3 tools/parity.py --changed-files <files> --list`.
- `cargo test` of the touched modules; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run (the test mod adds a no-op HEAD mixin on `AbstractContainerMenu#doClick` so the views are made).

## Done bar
Checks pass; `menu_open`/`menu_close` appear in the profile only when a menu route is on; no change to native menus' behaviour (the parity and LAN checks above).

## For the lead
Showcase install (RS's `doClick` route is on): open and use a chest, a furnace, an anvil, a crafting table, a shulker box; shift-click and drag stacks. Nothing differs from before; no `IllegalStateException` in the log. No user steps until SPEC-17.
