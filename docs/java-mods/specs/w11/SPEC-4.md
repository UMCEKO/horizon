# SPEC-4 (W11): result-slot clicks run on Java's real menu

Read first: `DESIGN.md` G3, D3, §5 (W5, W12); `SPEC-1.md`, `SPEC-3.md`; vanilla `AbstractContainerMenu.doClick` (PICKUP, QUICK_MOVE,
THROW, SWAP on a result slot), `CraftingMenu.quickMoveStack`, `InventoryMenu.quickMoveStack`, `ResultSlot` (`onTake`,
`onQuickCraft`, `checkTakeAchievements`, `getRemainingItems`), `ItemCombinerMenu`/`SmithingMenu.onTake`,
`RecipeCraftingHolder.awardUsedRecipes`, `ServerPlayer.triggerRecipeCrafted`; javap of the patched `ResultSlot` (DESIGN G3:
`ItemCraftedEvent` inside `removeCount > 0`, after `onCraftedBy`, before `awardUsedRecipes`; `setCraftingPlayer` around
`getRemainingItems`). Rust: `G/lan_crafting.rs` `guest_menu_op` (how `crafted` and `item_crafted` are awarded),
`lan_inventory_click`, `lan_crafting_click`; `G/lan_stations.rs` `lan_station_op`; `G/java_mods.rs` `java_use_request`,
`java_apply_use`; `B/GameBridge.java` `fill`, `use` (its reply tail).

Depends on: SPEC-1, SPEC-3.

Comments in code: one line at most, only for a non-obvious why.

## Goal

A click that takes from a crafting result (inventory 2x2, crafting table) or the smithing result runs vanilla's whole click
on Java's real menu when mod code is involved, so `Item.onCraftedBy`, `ItemStack.onCraftedBy`, `ItemCraftedEvent`/
`ItemSmithedEvent`, `getRemainingItems` with the crafting player, and shift-click's repeated crafts with Java's lookups all
happen in Forge's order. Native adopts the grid, result, carried stack and inventory the click leaves and awards the
recipes as it does today.

## When

Per click, on the server: the clicked slot is the result (0 for crafting menus, 3 for smithing) and the input is `PICKUP`,
`QUICK_MOVE`, `THROW` or `SWAP`, and one of: `has_recipe_code(station)`; `heard(ItemCrafted)` (crafting) or
`heard(ItemSmithed)` (smithing); the result's item has bit 0 (`onCraftedBy`); any grid item has bit 1
(`getCraftingRemainder`). Otherwise the click stays native.

## Wire: op `craft_click`

Request: `java_use_request()` (the acting player), then `byte menu` (0 inventory, 1 crafting table, 2 smithing table),
`long pos` (tables; 0 for the inventory), `int slot`, `int button`, `int input` (`ContainerInput` ordinal; map
`horizon_client::menu::Input` by name, in `ContainerInput`'s declared order, with a test), `int n` + `n` stacks (the grid
row-major, or the three smithing inputs), the carried stack. Each stack `int len` + NBT, 0 = empty.
Reply: `GameBridge.use`'s reply (factor its tail into `static void writeUseReply(Call call, Crossing.Out out, boolean claimed)`
and call it from both), then the carried stack, `int n` + the inputs after, the result after, `int awards` +
`(text recipe id, int k, k stacks)*` (each `triggerRecipeCrafted`, in order).

## Java

### `B/Crafting.java`
`static void click(Crossing.In in, Crossing.Out out)` (`Crossing.game("craft_click", Crafting::click)`):
1. `var call = GameBridge.fill(in, false)`; read the tail.
2. The menu: 0 → `player.inventoryMenu`; 1 → `new CraftingMenu(BORROWED, player.getInventory(), ContainerLevelAccess.create(level, pos))`;
   2 → `new SmithingMenu(BORROWED, player.getInventory(), ContainerLevelAccess.create(level, pos))`. `BORROWED` is a constant
   container id no client has open (e.g. `Integer.MAX_VALUE`; one-line comment why).
3. Put the inputs in without events: crafting through an accessor to `TransientCraftingContainer`'s `items` list
   (`B/mixin/CraftingItemsAccess`), smithing through `ItemCombinerMenu.inputSlots` (`B/mixin/CombinerInputsAccess`) and its
   container's `setItem` on a menu not yet listening; then one `menu.slotsChanged(container)`, vanilla's own result
   computation (the same lookup SPEC-3 serves). `menu.setCarried(carried)`.
4. `borrowing = true`; `menu.clicked(slot, button, ContainerInput.values()[input], player)` in `try`; `borrowing = false` in
   `finally`. Exceptions follow `GameBridge.safely`'s logging: `Horizon: a Java mod threw in a crafting click`, and the
   reply then holds the state as it was (native keeps its own).
5. While `borrowing`:
   - `B/mixin/RecipeCraftedMixin` on `ServerPlayer.triggerRecipeCrafted(RecipeHolder, List)` HEAD records
     `(id, copies of the list)` into `Crafting.AWARDS` (cleared at step 1).
   - The view's packet path drops `ClientboundContainerSetSlotPacket` and `ClientboundContainerSetContentPacket` (find where
     the server view's connection sends: `ConnectionMixin`/`Clients`; one check of `Crafting.borrowing`): native tells the
     clients what changed; the Java client's mirror follows native's packets (W5 SPEC-4).
6. Read the state: carried, inputs, result. For menu 0, then empty the view's 2x2 grid and result through the same
   accessors without `slotsChanged` (the view's inventory menu never holds native's grid between calls, so its
   `broadcastChanges` sends nothing new).
7. `writeUseReply`, then the tail above. Debug line: `Horizon: craft_click <menu> <input> -> <awards> crafts`.

### Test mod
`ChecksServer` gains `craft_take` (calls `Crafting.borrowedClick(player, menu, pos, slot, button, input, inputs, carried)`,
the body of steps 2–6 split out so a check can call it with the wand's player):
- `named_copy`, inventory menu, PICKUP on slot 0, grid `[name_tag named "N", empty, paper, empty]` (2x2): carried is a
  `name_tag` named "N"; the grid holds `[empty, empty, stick, empty]` (the test serializer's remainder in paper's place);
  `Crafting.CRAFTED` for `minecraft:name_tag` is 1 and `Crafting.MATRIX` saw 2 non-empty slots (the event fired before the
  inputs were used: vanilla's order).
- `charm`, crafting table, QUICK_MOVE on slot 0, grid holding 3 gold nuggets and 3 strings: the player's inventory gains 3
  charms in one stack whose `custom_data` has `crafted_by` = the player's name (`Item.onCraftedBy` edits the moved stack
  before `moveItemStackTo`); `CRAFTED` for the charm is 3 (one event per `onQuickCraft`, none from `onTake`, whose
  `removeCount` is 0); the grid is empty. These counts come from reading vanilla's and the patched bytecode, not from the
  code under test.

## Rust

### `G/java_crafting.rs`
- `pub(super) struct CraftClick { carried: Option<ItemStack>, inputs: Vec<Option<ItemStack>>, result: Option<ItemStack>, awards: Vec<(String, Vec<Option<ItemStack>>)> }`.
- `pub(super) fn java_craft_click(&mut self, menu: u8, pos: [i32; 3], slot: i32, button: i32, input: Input, inputs: &[Option<ItemStack>], carried: Option<&ItemStack>) -> Option<CraftClick>`:
  request as above, `java_request`, `java_apply_use` on the reply (it writes the acting player's inventory and xp; `as_guest`
  maps that to the guest), then parse the tail; `java_drain_block_events()`.
- `fn java_takes(&self, station: Station, slot: usize, input: Input, result: Option<&ItemStack>, inputs: &[Option<ItemStack>]) -> bool`:
  the "When" rule.

### `G/lan_crafting.rs` (`lan_inventory_click`, `lan_crafting_click`), `G/lan_stations.rs` (`lan_station_op` for smithing)
Before `guest_menu_op`, when `java_takes`: store the guest's menu state as `guest_menu_op` does, run
`self.as_guest(host, conn, |g| g.java_craft_click(..))` with the menu's inputs and the guest's carried stack, then:
- set the menu's input slots and result from the reply (`menu.set_slot` with `menu_env()`), the guest's `carried`, and
  collect every slot whose content changed for the resync (`inventory_clicked` / the table's and station's send paths, as
  after a native click);
- for each award: `trigger_recipe_crafted(id, inputs)`, the recipe book unlock as `award_crafted` does (skipping ids the
  book index doesn't know: mods' recipes aren't in the native book, DESIGN §7), and `native_item_crafted(player, id,
  result-before-the-click)` for native mods;
- `None` (no Java, or it threw) leaves the click to native as today.
Smithing: then `menu.smith_error()` and `native_prepare_smithing` as after a native click.

## Tests

- **Rust**: the `Input` → `ContainerInput` ordinal table (names in Java's order: `PICKUP`, `QUICK_MOVE`, `SWAP`, `CLONE`,
  `THROW`, `QUICK_CRAFT`, `PICKUP_ALL`); `java_takes` is false for a click on slot 1, for `QUICK_CRAFT`, and with all facts
  off, and true with only `heard(ItemCrafted)`; adopting a hand-built `CraftClick` sets the menu's slots and carried and
  lists exactly the changed slots.
- **Java**: the two checks above (`bridges`).
- **Scenario** `T/scenarios/w11-crafting.json` (from SPEC-3) gains: after the AE2 result shows, `{"ctl": ["click-slot", "200"]}`,
  a check that the grid holds what a Forge client's grid holds after the same take (record it once on Forge with this jar:
  the recipe's `result_component` is expected back as AE2's `getRemainingItems` override; use that observation as the
  expected value), `{"ctl": ["close"]}`, then a `slot` check for `ae2:item_storage_cell_16k` in the inventory; and 4 oak
  logs in slot 100 with `{"ctl": ["click-slot", "200", "shift"]}` → 16 oak planks in the inventory (vanilla's 1 log → 4
  planks, 4 times). Run with `HORIZON_PROFILE=OUT/profile` and report the
  `craft_click` mean from `profile_report.py`.
- `cargo test ... --lib java_crafting`, `java_mods.py build`, `check`, `unit`, `bridges`; clippy with and without the feature.

## Done bar

- [ ] Scenario passes; `craft_click` mean under 2 ms for a 64-craft shift-click.
- [ ] A Sophisticated Backpack crafted in survival is a working backpack (open it: its own storage), as on Forge.
- [ ] A shift-click on a Traveler's Backpack recipe makes exactly as many backpacks as the inputs allow; no item is
      duplicated or lost (count the inputs before and after).
- [ ] M1 (`agents/bootcmp` gameplay run, "craft" step): `ItemCraftedEvent` count equals Forge's for the same crafts.
- [ ] W12: `deferred.tsv` gets `special: W11 SPEC-4` for showcase keys on `ResultSlot`, `CraftingMenu`, `InventoryMenu`,
      `SmithingMenu` if any appear; lint green.

## Live steps for the lead

1. Showcase install, survival: craft a Sophisticated Backpack (JEI layout); place it and open it: it has its own inventory;
   break and pick it up, the contents stay (Forge behaviour, which needs its `onCraftedBy`/UUID setup).
2. install-rs: AE2 1k cell + 16k component in the 2x2 grid, take the result: you hold the 16k cell and the grid keeps what
   it keeps on Forge (expected: the 1k component, given back).
3. Shift-click 10 oak logs' worth of planks from a crafting table: 40 planks, same as without mods.
