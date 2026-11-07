# SPEC-9 (W11): anvil and grindstone with mod code

Read first: `DESIGN.md` G7, D3, D8, §5 (W12, AE2's `AnvilMenu#createResultInternal`); `SPEC-1.md` (events and item bits),
`SPEC-4.md` (the borrowed click: this spec adds two menu kinds to it); vanilla `AnvilMenu` (`createResult`,
`createResultInternal`, `setItemName`, `onTake`, `mayPickup`, fields `cost`, `repairItemCountCost`, `itemName`,
`onlyRenaming`), `GrindstoneMenu` (`createResult`, the result slot's `onTake`, `getExperienceAmount`); javap of the patched
classes (DESIGN G7 and the grindstone flow: `onTake` returns early when `CommonHooks.onGrindstoneTake` answers true);
Rust `G/lan_stations.rs` `lan_station_op` (the `recomputed` blocks with `native_prepare_anvil`/`native_prepare_grindstone`,
the `anvil_hook` deferral, `lan_station_combined`), `lan_station_rename`, `L/menu.rs` `anvil_result`, `Anvil`.

Depends on: SPEC-1, SPEC-4 (`craft_click` and its `borrowing` packet rule).

Comments in code: one line at most, only for a non-obvious why.

## Goal

When a mod listens to the anvil's or grindstone's events, an input item's class overrides what these menus ask
(`supportsEnchantment`, `isCombineRepairable`, `getMaxDamage`, `isDamageable`, `canGrindstoneRepair`: SPEC-1 bits 4, 6, 8,
9, 10), or a W12 route patches `AnvilMenu`/`GrindstoneMenu`, the result is Java's real `createResult` (so
`AnvilUpdateEvent`, `GrindstoneEvent.OnPlaceItem` and the overrides apply), and a take from the result slot is the real
click (so `AnvilCraftEvent.Pre/Post`, `GrindstoneEvent.OnTakeItem`, the level cost, the anvil's wear).

## When

- Anvil result in Java: `heard(AnvilUpdate)`, or the left or right input's item has bit 4, 6, 9 or 10, or
  `horizon_java_host::routes()` has a route on `net/minecraft/world/inventory/AnvilMenu`.
- Grindstone result in Java: `heard(GrindstoneOnPlace)`, or an input's item has bit 6, 8, 9 or 10, or a route on
  `GrindstoneMenu`.
- Take in Java: the result rule, or `heard(AnvilCraftPre | AnvilCraftPost)` (anvil) / `heard(GrindstoneOnTake)` (grindstone).

## Wire

- Op `station_result`: `java_use_request()`, `byte menu` (3 anvil, 4 grindstone), `long pos`, the two inputs (stacks as in
  SPEC-4), `text name` (anvil rename box; "" = none, `\0` = null as vanilla's `itemName` can be). Reply: result stack,
  `int cost`, `int repairItemCountCost`, `bool onlyRenaming` (anvil; zeros for the grindstone).
- `craft_click` (SPEC-4) gains menus 3 and 4; for 3 the tail adds `text name`.

## Java (`B/Crafting.java`)

- Borrowed `AnvilMenu(BORROWED, inventory, ContainerLevelAccess.create(level, pos))` / `GrindstoneMenu(...)`; inputs set
  through `ItemCombinerMenu.inputSlots` (anvil, SPEC-4's accessor) or `GrindstoneMenu.repairSlots` (new accessor), the
  anvil's `itemName` through an accessor (never `setItemName`, which recomputes and validates), then the menu's own
  `createResult()` (anvil: public; grindstone: private → `@Invoker("createResult")`).
- `stationResult(in, out)` reads the result slot and the anvil's fields (accessors for `repairItemCountCost`,
  `onlyRenaming`; `cost` is a `DataSlot`).
- `craft_click` for 3/4: the same setup, then the click as SPEC-4 step 4; experience levels and orbs go through the view
  (`giveExperienceLevels` writes through, `ExperienceOrb.award` spawns through `EntityViews.spawn`), the anvil's damage
  through the bridged `setBlock`/`levelEvent`.

## Rust

- `G/java_crafting.rs`: `java_station_result(&mut self, menu: u8, pos, left, right, name) -> Option<StationResult>`
  (`result`, `cost`, `repair_count`, `only_renaming`); `java_station_takes(...) -> bool` per "When".
- `G/lan_stations.rs` `lan_station_op`:
  - in the `recomputed` anvil block, before `native_prepare_anvil`: when the rule holds, set slot 2, `data[0]`,
    `menu.anvil.repair_count`, `menu.anvil.only_renaming` from Java (make those fields `pub(crate)` if they aren't); then
    native mods' hook as today. Same for the grindstone block before `native_prepare_grindstone`.
  - before running a click on the result slot (2) with `PICKUP`, `QUICK_MOVE`, `THROW` or `SWAP`, when
    `java_station_takes`: run SPEC-4's `java_craft_click` (menu 3 or 4) inside `as_guest` instead of the native click and
    adopt its state as SPEC-4 does (slots, carried, inventory, xp through `java_apply_use`). The native `anvil_hook`
    deferral and `grindstone_taken` don't run for that click (Java did both). If native mods are also present, call
    `native_anvil_craft_post` after (their `Pre` can't cancel a click Java already made: log once
    `Horizon: native mods' AnvilCraftEvent.Pre is skipped for a click Java ran`).
  - `lan_station_rename` (anvil): its recompute goes through the same Java result when the rule holds.

## Tests

- **Test mod**: a listener of `AnvilUpdateEvent` that, for left = `minecraft:stick` named "w11" and empty right, sets output
  `minecraft:blaze_rod` and cost 7; a listener of `GrindstoneEvent.OnPlaceItem` that turns a lone `minecraft:paper` into
  `minecraft:map`; and an `AnvilCraftEvent.Pre` listener that cancels when the output is `minecraft:barrier`. Checks
  (`ChecksServer`, calling `Crafting.stationResult` and `borrowedClick` directly):
  - anvil stick named "w11": result `blaze_rod`, cost 7 (the listener's values);
  - anvil stick named "w11", PICKUP on slot 2 with a player at 10 levels: carried `blaze_rod`, levels 3, inputs consumed as
    vanilla's `onTake` does with `repairItemCountCost` 0 and not `onlyRenaming` (the right slot cleared);
  - grindstone paper: result `minecraft:map`;
  - a cancelled `Pre` (a listener-made `barrier` output): carried empty, inputs untouched, levels unchanged.
- **Rust**: the "When" rules as pure functions over the facts (each bit and event alone).
- `java_mods.py build`, `check`, `bridges`; `cargo test ... --lib java_crafting`; clippy both ways; the anvil and
  grindstone parity harnesses through `verify/parity.sh` (unchanged without mods).

## Done bar

- [ ] The checks above print ok.
- [ ] install-rs: an anvil refuses to put an enchanted book on a Meka-Tool exactly when Forge refuses (Mekanism's
      `supportsEnchantment`): compare with a Forge client on the same jar (M3 note in the report).
- [ ] showcase: AE2's `AnvilMenu#createResultInternal` mixin runs (W12 route counter or a `HORIZON_JAVA_DEBUG` line from the
      route) when an anvil result is computed; `deferred.tsv` gets `special: W11 SPEC-9` for that key; lint green.
- [ ] A vanilla anvil repair (two damaged iron pickaxes) in a pack where no rule holds is native, unchanged.

## Live steps for the lead

1. install-rs, survival, 30 levels: Meka-Tool on the left, an enchanted book (Sharpness) on the right: the result is what a
   Forge client shows for the same inputs (expect none if Mekanism refuses books).
2. Rename a diamond in an anvil: costs 1 level, as without mods.
