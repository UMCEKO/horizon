# SPEC-10 (W11): enchanting table and furnace take, with mod code

Read first: `DESIGN.md` G7, D8; `SPEC-1.md`, `SPEC-4.md` (borrowed menus), `SPEC-9.md`; vanilla `EnchantmentMenu`
(`slotsChanged`, `clickMenuButton`, `getEnchantmentList`, `enchantmentSeed`, `costs`, `enchantClue`, `levelClue`),
`EnchantmentHelper.selectEnchantment`, `Player.onEnchantmentPerformed`, `FurnaceResultSlot`; javap (patched)
`EnchantmentMenu` lambdas (`EventHooks.onEnchantmentLevelSet` per row in `slotsChanged`, `CommonHooks.onPlayerEnchantItem`
in `clickMenuButton` after the enchantments are applied) and `FurnaceResultSlot.checkTakeAchievements` (DESIGN G7); Rust
`G/lan_stations.rs` `lan_enchant_offers` (`native_prepare_item_enchant`), `lan_enchant` (`xp_seed`,
`native_enchant_item`), `G/lan_crafting.rs` `lan_machine_click` (`native_furnace_extract`).

Depends on: SPEC-1, SPEC-4.

Comments in code: one line at most, only for a non-obvious why.

## Goal

The enchanting table's offers and its enchanting run Java's real `EnchantmentMenu` when a mod listens to
`EnchantmentLevelSetEvent` or `PlayerEnchantItemEvent`, or the input item overrides `supportsEnchantment`,
`isPrimaryItemFor` or `applyEnchantments` (SPEC-1 bits 4, 5, 11). A furnace take fires `ItemSmeltedEvent` when a mod
listens, at vanilla's point (after the recipes' experience).

## When

- Offers and enchanting in Java: `heard(EnchantmentLevelSet)` or `heard(PlayerEnchantItem)` or the item in slot 0 has
  bit 4, 5 or 11.
- Smelted event: `heard(ItemSmelted)`.

## Wire

- Op `enchant_offers`: `java_use_request()`, `long pos`, the item and lapis stacks, `int seed` (the guest's `xp_seed`).
  Reply: `int costs[3]`, `int enchantClue[3]`, `int levelClue[3]`.
- `craft_click` (SPEC-4) gains menu 5 (enchanting table) with `int seed`; the click is `clickMenuButton(player, id)`, so the
  tail carries `int button id` instead of slot/input (add a `byte op` 0 = click, 1 = button to the tail). Reply as SPEC-4,
  plus `int newSeed` (vanilla refreshes `enchantmentSeed` after enchanting).
- Op `smelted`: `java_use_request()`, the taken stack, `int count`. Reply: effects only.

## Java (`B/Crafting.java`)

- Borrowed `EnchantmentMenu(BORROWED, inventory, ContainerLevelAccess.create(level, pos))`: set the view's
  `enchantmentSeed` (accessor on `Player`) to the request's seed, put item and lapis in `enchantSlots` (accessor) without
  events, then `menu.slotsChanged(enchantSlots)` (vanilla computes the bookshelf power through bridged world reads: the
  game is active) and read `costs`, `enchantClue`, `levelClue`.
- Button: `menu.clickMenuButton(player, id)` inside `borrowing`; read slots, the view's seed, xp (use reply).
- `smelted`: `EventHooks.firePlayerSmeltedEvent(player, stack, count)`.

## Rust

- `G/lan_stations.rs` `lan_enchant_offers`: when the rule holds, the three rows come from `enchant_offers` and go into
  `gs.menu.data` as native fills them; then `native_prepare_item_enchant` as today.
- `lan_enchant`: when the rule holds, `java_craft_click` (menu 5, button) inside `as_guest`; adopt slots, xp, and set the
  guest's `xp_seed` to the reply's seed instead of drawing a new one natively; then `native_enchant_item` as today with the
  enchanted stack.
- `G/lan_crafting.rs` `lan_machine_click`: after `native_furnace_extract` for a furnace result take with `removeCount > 0`,
  when `heard(ItemSmelted)`: `java_request("smelted", ..)` inside `as_guest`, then `java_apply_effects`.

## Tests

- **Test mod**: an `EnchantmentLevelSetEvent` listener that sets row 2's level to 30 for a `minecraft:book` input; a
  `PlayerEnchantItemEvent` listener that counts; an `ItemSmeltedEvent` listener that counts per item. Checks: offers for a
  book with 15 bookshelves placed around a table in the check's area: row 2 cost is 30 (the listener's value), rows 0 and 1
  equal vanilla's `EnchantmentHelper.getEnchantmentCost(new LegacyRandomSource... seeded as the menu seeds it)` computed in
  the check with vanilla's method; enchanting row 0 of a diamond sword at 30 levels: the count is 1, the sword holds the
  enchantments of the first clue, levels drop by 1, one lapis is used. Smelted: `Crafting.smelted` call with an iron ingot
  x3 counts 3.
- **Rust**: the rules as pure functions; `lan_enchant` adopts the reply's seed (a unit test of the adoption helper).
- `java_mods.py build`, `check`, `bridges`; `cargo test ... --lib java_crafting`; clippy both ways; the enchanting-table
  parity harness through `verify/parity.sh` (unchanged without mods).

## Done bar

- [ ] Checks print ok; a pack where no rule holds keeps native enchanting (no `enchant_offers` request in the profile).
- [ ] BG2 (install-rs): a Building Gadget in an enchanting table shows the offers a Forge client shows (its
      `isPrimaryItemFor`); M3 note in the report with a Forge screenshot pair.
- [ ] M1: with a listener-mod pack, `EnchantmentLevelSetEvent` (3 per offer computation), `PlayerEnchantItemEvent` and
      `ItemSmeltedEvent` counts match Forge's for the same session.

## Live steps for the lead

1. install-rs, survival, 30 levels, 15 bookshelves around an enchanting table: put a Building Gadget in; the offers (and
   whether there are any) match a Forge client on the same seed and shelves.
2. Smelt 8 raw iron in a furnace and take the ingots: 8 ingots and the usual experience; no exception in the log.
