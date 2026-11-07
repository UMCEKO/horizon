# W11: recipes, loot, enchantments and crafting with mod code

Status: design only, nothing implemented. Binding: `agents/RULES.md`, COVERAGE-SPEC §1 (rule 0: Java runs only what a loaded
mod is detected to change), W12 `DESIGN.md` (gates, `deferred.tsv` reasons), entities SPEC-0/1d (events posted only when the
bus has a listener: "admits").

Paths: `H/` `crates/horizon-java-host/src/`, `G/` `crates/horizon-client/src/game/`, `L/` `crates/horizon-client/src/`,
`W/` `crates/horizon-world/src/`, `I/` `crates/horizon-items/src/`, `P/` `crates/horizon-port-pack/src/`,
`B/` `mods/horizon/src/java-mods-host/boot/dev/horizonlab/javamods/boot/`, `TM/` `mods/horizon/src/java-mods-host/testmod/`,
`U/` `mods/horizon/src/java-mods-host/unit/dev/horizonlab/javamods/boot/`, `T/` `tools/`.
Installs: play pack `/home/umceko/hz-poc/play/install-rs` (Mekanism x4, AE2, RS, BG2, Jade, JEI, guideme), showcase
`/home/umceko/hz-poc/agents/w12/install` (43 jars). Patched vanilla: `install-rs/libraries/net/neoforged/minecraft-server-patched/26.3.0.101/`.

## 1. The gap, with evidence

Every claim below is from the jars' data (zip listing), javap of the NeoForge-patched classes, the `replaces` reports in
`agents/finish/rep-{rs,showcase}/replaces.json`, or the code at java-mods `cc1b066b6`.

| # | Gap | Evidence | What a player sees |
|---|---|---|---|
| G1 | Recipes whose `neoforge:conditions` use a mod's condition type never load natively | `W/conditions.rs` `known()` accepts only `neoforge:mod_loaded/true/not/and/or`; an unknown type drops the file. Jars: `sophisticatedcore:item_enabled` 75 entries (Sophisticated Backpacks), `mekanism:mod_version_loaded` 118, `terralith:config` 6. Java loads them through NeoForge's real `ICondition`s (`GameBridge.prepare`, real `WorldLoader`) | No Sophisticated Backpack, upgrade or tier is craftable |
| G2 | Recipes of a mod's serializer never match in a native crafting table, 2x2 grid or smithing table | `L/crafting.rs` `Recipes::from_sources` (l. 1525) files any non-`minecraft:` type under `custom`; `find_with` (l. 1762) scans only `crafting`; `menu.rs` `recompute` (l. 1751) uses it. Jar types: `mekanism:mek_data` 86 (energy cubes, bins, tanks, digital miner, jetpack, atomic disassembler...), `mekanismtools:paxel` 12, `ae2:storage_cell_upgrade` 40 + `add/remove_item_upgrade`, `facade`, `storage_cell_disassembly` 20, `refinedstorage:recoloring` 320 + `storage_container_upgrade` 12 + `upgrade_with_enchanted_book` 4, `sophisticatedcore:upgrade_next_tier` 22 + 4 backpack types + `smithing_backpack_upgrade`, `travelersbackpack:backpack_shaped` 43 + `backpack_upgrade` 44. `replaces`: `Recipe#matches/assemble` implemented by ae2, mekanism, sophisticated*, travelersbackpack; `ShapedRecipe#matches/assemble` by mekanism, refinedstorage | Most of Mekanism's tiers and machines, all paxels, AE2 cell upgrades, every Traveler's Backpack but the standard one: the result slot stays empty |
| G3 | The crafting take runs no mod code | Patched `ResultSlot.checkTakeAchievements` (javap): `if removeCount > 0 { ItemStack.onCraftedBy; EventHooks.firePlayerCraftingEvent(player, carried, craftSlots) }` then `awardUsedRecipes`; `onTake` wraps `getRemainingItems` in `CommonHooks.setCraftingPlayer`. Listeners of `ItemCraftedEvent`: mekanism, sophisticatedbackpacks, sophisticatedcore, architectury. `Item#onCraftedBy` overridden by SB `BackpackItem`, SC `EnderLinkerItem`; `CraftingRecipe#getRemainingItems` by ae2, mekanism, refinedstorage, sophisticatedcore. The host posts none of it (`grep firePlayerCraftingEvent\|ItemCraftedEvent mods/horizon/src/java-mods-host` is empty) | A crafted backpack skips its own setup; AE2/RS recipes keep native remainders; Mekanism's listener never hears a craft |
| G4 | Global loot modifiers never apply | Patched `LootTable.getRandomItems(LootContext)` calls `CommonHooks.modifyLoot(getLootTableId(), list, ctx)`; every public `getRandomItems` and `fill` goes through it. `LootModifierManager` loads in Java, but every roll in Horizon is native: `W/loot.rs` `roll`, `roll_entity`, `roll_split`, `fill_with`; `I/items.rs` `evaluate`'s top-level callers; mod blocks' drops too (`BlockBehaviour.getDrops` and `Block.dropResources` are bridged to native: `G/java_bridges/effects.rs`). Jars: travelersbackpack 24 (one, `hay_backpack_modifier`, has an empty `all_of`: it runs on every roll), sophisticatedbackpacks 9 `inject_loot`, balm 1 | No backpacks or upgrades in structure chests; the hay backpack's crop bonus never happens |
| G5 | Mod loot functions and conditions in tables Horizon rolls do nothing | `W/loot.rs` `native_loot_function`/`native_loot_condition` (l. 70/84) reach native mods only (`L/entry.rs` l. 65). RS block tables: `refinedstorage:energy` 16, `refinedstorage:storage_block` 10, `refinedstorage:portable_grid` 2 | An RS storage block, controller or portable grid drops without its storage id or energy, so the stored items are lost |
| G6 | Enchantments that come from item code don't reach native loot | Mekanism overrides `IItemExtension#getEnchantmentLevel`, `getAllEnchantments`, `supportsEnchantment`; AE2 mixes into `EnchantmentHelper#getItemEnchantmentLevel` (W12 `deferred.tsv`: `seam: crates/horizon-items/src/item_data.rs:274`). Native loot reads the `enchantments` component only (`I/items.rs` l. 1601 `can_silk_touch`, `W/loot.rs` `item_level`) | Meka-Tool modules that act as enchantments (which ones: SPEC-8 step 1 measures) don't change the drops of what it breaks |
| G7 | Anvil, grindstone, enchanting table and furnace hooks never run | Patched sites (javap): `AnvilMenu.createResult` → `CommonHooks.onAnvilUpdate`; `AnvilMenu.onTake` → `fireAnvilCraftPre`/`Post`; `GrindstoneMenu.createResult` → `onGrindstoneChange`; `onTake` → `onGrindstoneTake`; `EnchantmentMenu` lambdas → `EventHooks.onEnchantmentLevelSet`, `CommonHooks.onPlayerEnchantItem`; `FurnaceResultSlot.checkTakeAchievements` → `firePlayerSmeltedEvent` (after `awardUsedRecipesAndPopExperience`, when `removeCount != 0`); `SmithingMenu.onTake` → `firePlayerSmithedEvent`. Item overrides that change these menus: Mekanism `supportsEnchantment`, BG2 `isPrimaryItemFor`, `getMaxDamage`. No listener of these events in either install except `ItemSmeltedEvent` (architectury, which no installed mod uses) | An anvil puts enchantments on a Meka-Tool that Forge refuses; M1 misses these events for any mod that listens |
| G8 | Smithing slots refuse a mod recipe's inputs | Vanilla `SmithingMenu.mayPlace` tests the recipe property sets (`RecipeManager.propertySet(SMITHING_BASE...)`, built from every recipe's `placementInfo`); native `L/stations.rs` `smithing_accepts` (l. 646) knows native smithing recipes only | The Sophisticated Backpacks netherite upgrade can't be placed |

Checked and not a gap for the bar:
- `LootTableLoadEvent` already fires where Forge fires it: Java's real data load (`GameBridge.prepare`). Its only listener is
  architectury's forwarder, and no installed mod depends on architectury (no `neoforge.mods.toml` names it).
- Mod enchantments (Dungeons and Taverns, 18) use vanilla effect types and `run_function` only: data, native's job.
- Villager trades are data in 26.3 (`trade_set`, `villager_trade`), carried and read by `L/merchant.rs`; NeoForge 26.3 has no
  `VillagerTradesEvent`.
- No mod fuel: no `getBurnTime` override in `replaces`, no `furnace_fuels` data map in any jar. No mod cooking recipe type.
- NeoForge's 446 overrides of vanilla recipes only swap ingredients for 30 `c:` tags; no installed jar adds items to them.
- No brewing, potion or trade event listener in either install.

Live evidence: one offscreen boot (`live/start.log`) came up without Java mods: the main tree's `target/fast/horizon-client`
has no `java-mods` feature (`strings` finds no `HORIZON_JAVA_RUN`) and its log reads `mod ae2 content not loaded ... unknown
field states`. So the gaps above are proven statically, and each spec's done bar starts with a live repro on a java-mods
build.

## 2. Approach

Native keeps every station, menu and both loot engines. Java runs only the step a loaded mod changes, at vanilla's moment,
on vanilla's inputs, decided once at server start (SPEC-1). Nothing is keyed on a mod: the facts come from the registries'
owned set (`BootFacts.owns`), the classes' own code, the bus's listener lists and the loaded data.

1. **Facts (SPEC-1).** `CraftFacts` at the first server start: which vanilla recipe types hold recipes of a mod's class or
   serializer, which events have listeners, which owned items override the relevant `Item`/`IItemExtension` methods, the
   loaded loot modifiers and the table ids they can apply to, the owned loot function and condition types.
2. **The recipe set (SPEC-2).** Java's loaded recipes are the truth: the boot facts export each recipe of the mods'
   namespaces in its codec form, and carry writes those instead of the jar's files. Conditions are resolved by the real
   NeoForge, once, at boot.
3. **Crafting and smithing (SPEC-3, SPEC-4).** When `CraftFacts` says crafting (or smithing) has mod code, the grid's result
   is Java's real `RecipeManager.getRecipeFor` + `assemble` (one owner, no native matching), computed after the click like
   `native_prepare_item_craft`; a click on the result slot runs on Java's real menu (vanilla's `doClick`, `quickMoveStack`,
   `ResultSlot.onTake` with NeoForge's patches), and native adopts the menu state it leaves.
4. **Loot (SPEC-5, 6, 7).** Both engines get one hook at each top-level `getRandomItems` (after the stack splitter, before a
   chest's shuffle), the place NeoForge calls `modifyLoot`. Mod function and condition kinds already have native hook points;
   Java answers them. The roll's random state crosses both ways, so Java's draws continue native's sequence.
5. **Item enchantments (SPEC-8).** Before a native roll whose tool's item has enchantment code, the tool is replaced, for that
   roll only, by a copy carrying Java's `getAllEnchantments` answer.
6. **Stations (SPEC-9, SPEC-10).** At native's existing post-click seams (`native_prepare_anvil/grindstone`,
   `native_prepare_item_enchant`, `native_furnace_extract`), Java runs the real vanilla method when a listener or an input
   item's override is detected; result takes and the enchanting button use SPEC-4's borrowed click (menus 3–5).

## 3. Decisions

- **D1. Java's `RecipeManager` is the crafting lookup when mod code is detected, not a merge with native.** A merge needs
  NeoForge's recipe order (registry order plus `RecipeManager.setPriorityMap`) and the menu's `recipeHint`; Java has both.
  Native's matcher stays the owner in packs without mod crafting code. One crossing per grid change, never per frame.
- **D2. The lookup runs after the click, not inside `Menu::click`.** `Env` borrows `Game`, and a Java call needs `&mut Game`
  for bridged natives (`H/bridge.rs` `with_active`). The result slot is set at the same point native mods' result hook
  already uses (`lan_crafting.rs` `native_prepare_item_craft`); an `Env` flag stops native's in-click `recompute`.
- **D3. A click on the result slot runs on Java's real menu.** Shift-click crafting interleaves `Item.onCraftedBy`, the move,
  `onQuickCraft`, the event, the remainders and a new lookup, many times per click; replaying that as native steps with
  Java in between would duplicate `quickMoveStack`. The anvil precedent (`anvil_hook`) defers one hook; this needs dozens.
  Native adopts the grid, result, carried stack and inventory the click leaves, then awards recipes and tells native mods.
- **D4. The loot hooks run Java isolated** (`bridge::isolated`: no game behind the call), because a roll can run inside a
  level cascade or a bridged native (`with_level`), where a second `&mut Game` is unsound. A modifier, function or condition
  that reaches a bridged native gets a Java exception, logged once per kind:
  `Horizon: <kind> read the world during a loot roll; loot code runs without world access (W11 D4)`. The alternative,
  lifting the call out of every roll site (more than 20, several inside cascades), is W12's `seam: lift` class. None of the
  installed modifiers or functions reads the world (they read the context's parameters, the stack and configs).
- **D5. The roll's random crosses as state.** `LegacyRandom` (48-bit state) and `XoroshiroRandom` (two words) both have
  exact Java twins (`LegacyRandomSource`, `XoroshiroRandomSource(long, long)`); Java's draws advance native's random, so
  a chest's shuffle after the modifiers draws what Forge draws.
- **D6. Mod loot kinds in a roll keep the existing `seed` draw** (`NativeLootContext.seed`, API 133). Changing it is an API
  change for native mods; RS's three functions draw nothing. Recorded as a known difference for tables with mod kinds.
- **D7. Item enchantments are substituted per roll, not globally.** `ItemStack::enchantments()` has dozens of native readers
  on several threads; a Java query there would cross from places that can't. The rolls are where the bar needs it
  (drops); other readers (damage, protection) are W9 phase 4's.
- **D8. Events are posted only when the bus has a listener** (SPEC-1 `heard`), as entities SPEC-1d posts only what
  `admits`. M1 counts compare listened events.
- **D9. The recipe set travels with the boot facts**, not at world open: Horizon builds its tables before a world opens, and
  a cached launch must not wait for Java (`docs/java-mods.md`, "Blocks and items from the boot").

## 4. Specs

| Spec | What | Depends on |
|---|---|---|
| [SPEC-1](SPEC-1.md) | `CraftFacts`: detection at server start; test-mod content for W11 | – |
| [SPEC-2](SPEC-2.md) | Recipes from Java's loaded set (boot facts, carry) | – |
| [SPEC-3](SPEC-3.md) | Grid and smithing results from Java's `RecipeManager`; smithing slot acceptance; scenario steps | SPEC-1 |
| [SPEC-4](SPEC-4.md) | Result-slot clicks on Java's real menu: `onCraftedBy`, `ItemCraftedEvent`, `ItemSmithedEvent`, remainders | SPEC-3 |
| [SPEC-5](SPEC-5.md) | The loot seam in both engines, random state, `bridge::isolated` (Rust only) | – |
| [SPEC-6](SPEC-6.md) | Global loot modifiers through `CommonHooks.modifyLoot` | SPEC-1, SPEC-5 |
| [SPEC-7](SPEC-7.md) | Mod loot functions and conditions in native rolls; the real block entity from Java's drops | SPEC-1, SPEC-5 |
| [SPEC-8](SPEC-8.md) | Item-code enchantments for the tool of a roll | SPEC-1 |
| [SPEC-9](SPEC-9.md) | Anvil and grindstone: Java's `createResult` and take events when detected | SPEC-1, SPEC-4 |
| [SPEC-10](SPEC-10.md) | Enchanting table and furnace events and item overrides | SPEC-1, SPEC-4 |

Order for the bar: SPEC-1, SPEC-2 (any time), SPEC-3, SPEC-4, then SPEC-5, SPEC-7 (RS data loss), SPEC-6, SPEC-8;
SPEC-9 and SPEC-10 last (no listener in either install today; M1 and the Meka-Tool anvil case).

Every spec: comments in code are one line at most, and only for a non-obvious why. Tests use independent expected values
(hand-written JSON, `java.util.Random`, vanilla's own classes in Java) or a NeoForge oracle; never values computed by the
code under test. Live steps need a `--features java-mods` build and the gate (`agents/gate.py`).

## 5. Interfaces with other workstreams

- **W12.** SPEC-4 serves the routes on `CraftingMenu`, `InventoryMenu`, `ResultSlot`, `SmithingMenu` (their bodies run in
  Java when crafting has mod code); SPEC-9 serves `AnvilMenu#createResult*` when it runs Java's `createResult`, which is how
  AE2's `AnvilMenu#createResultInternal` mixin (`undecided` today) gets to run. Each spec adds its keys to
  `T/java_routing/deferred.tsv` as `special: W11 SPEC-n` where the route is served only under SPEC-n's condition, and
  removes them when always served. `LootTable#fill` (`seam: W/loot.rs:712`) and `EnchantmentHelper#getItemEnchantmentLevel`
  stay W12's (Lootr's and AE2's patches are mixins, not hooks W11 runs).
- **W9.** SPEC-6 and SPEC-7 build `LootContext` parameters from live entity views (`EntityViews`, phase 1c, done) by entity
  id; damage sources are built from type and entity ids until W9 phase 4 defines a damage view.
- **W5.** SPEC-4's borrowed menus never send container packets (the Java client's mirror follows native's packets,
  W5 SPEC-4). W5 SPEC-4's D4 (JEI recipe transfer) is not W11.
- **W13.** Block events (crop growth, bonemeal) are W13. SPEC-10's furnace event is a take event, not a block event.

## 6. M1 events W11 makes fire

`PlayerEvent.ItemCraftedEvent`, `ItemSmeltedEvent`, `ItemSmithedEvent`, `AnvilUpdateEvent`, `AnvilCraftEvent.Pre/Post`,
`GrindstoneEvent.OnPlaceItem/OnTakeItem`, `EnchantmentLevelSetEvent`, `PlayerEnchantItemEvent`, `GetEnchantmentLevelEvent`
(inside SPEC-8's query, as on Forge). `LootTableLoadEvent` already fires (Java's load).

## 7. Out of scope (beyond the bar)

- The crafter block with mod recipes: its craft runs from a scheduled block tick inside the level cascade (W12 `seam: lift`).
- Mod recipes in the native recipe book (displays, ghost recipes); JEI covers browsing and W5 its overlay.
- World data packs in Java (`GameBridge.prepare` loads `WorldDataConfiguration.DEFAULT`): a world pack's recipes are native's.
- Tables a `LootTableLoadEvent` listener changes reaching native rolls (no listener with an effect is installed).
- Lootr's chest replacement (W12/W10); mod brewing recipes and potion events (none installed); mod cooking recipes and fuels
  (none installed); NeoForge's vanilla recipe overrides (no effect in either install).
- Item-code enchantments outside loot rolls (damage, protection, wear): W9 phase 4.

## 8. Risks

- SPEC-4 runs vanilla's whole click in Java; a mod patch on `AbstractContainerMenu.doClick` (IPN in the top-50) then runs
  for result-slot clicks only. Measure with M1 on a pack that has IPN.
- Isolation (D4) turns a world read inside loot code into a logged exception. The boot facts can list modifier condition
  types; SPEC-6 logs the ones that aren't `neoforge:loot_table_id`, vanilla random or a combinator, so a pack that needs
  world access is visible before a player finds it.
- One crossing per grid change and one per click on a result slot (a whole shift-click is one Java click). Profile with
  `HORIZON_PROFILE` in SPEC-4's scenario.
