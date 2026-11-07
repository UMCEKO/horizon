# SPEC-1 (W11): CraftFacts, what the mods change in crafting, loot and stations

Read first: `DESIGN.md` §1–3, `docs/java-mods.md`, `docs/java-mods-map.md` §2–3, `B/NativeSounds.java` (`listened`: the
bus's listener list), `B/ItemCode.java` (how an item's own code is detected), `B/GameBridge.java` `register`, `prepare`,
`G/java_mods.rs` `java_world_phase`, `G/java_content.rs` `entity_types`. Paths as DESIGN.

Depends on: nothing. Every other W11 spec but SPEC-2 and SPEC-5 reads these facts.

Comments in code: one line at most, only for a non-obvious why.

## Goal

At the first server start, Java answers once what crafting, loot and station code the loaded mods have; Rust keeps the answer
for the session and every W11 seam asks it (one bit test) before it crosses. Also: the test mod gains the W11 content every
later spec's checks use.

## Java: `B/CraftFacts.java` (new), op `craft_facts`

`static byte[] bin(MinecraftServer server)`; registered in `GameBridge.register` as
`Crossing.game("craft_facts", input -> CraftFacts.bin(server()))`. Computed from the server view's live state, never from
names:

1. **Recipe types with mod code.** For each `RecipeHolder` in `server.getRecipeManager().getRecipes()`: the recipe has mod
   code when `BootFacts.owns(BuiltInRegistries.RECIPE_SERIALIZER, key)` holds for its serializer, or any of its ingredients
   (`placementInfo().ingredients()`) `isCustom()`. Collect the set of `RecipeType` ids with such a recipe. Write a bit per
   vanilla station type: `CRAFTING` (bit 0), `SMITHING` (1), `SMELTING`, `BLASTING`, `SMOKING`, `CAMPFIRE_COOKING` (2–5),
   `STONECUTTING` (6). A type outside these is ignored (a mod's own machine type). Log one line per set bit:
   `Horizon: <n> recipes of mods' code in minecraft:<type>`.
2. **Heard events.** For each class below, `listened(NeoForge.EVENT_BUS, cls)` (generalise `NativeSounds.listened` into
   `static boolean listened(IEventBus bus, Class<?> event)` in a shared place, e.g. `B/Listeners.java`, and make
   `NativeSounds` call it): `ItemCraftedEvent`, `ItemSmeltedEvent`, `ItemSmithedEvent`, `AnvilUpdateEvent`,
   `AnvilCraftEvent.Pre`, `AnvilCraftEvent.Post`, `GrindstoneEvent.OnPlaceItem`, `GrindstoneEvent.OnTakeItem`,
   `EnchantmentLevelSetEvent`, `PlayerEnchantItemEvent`, `GetEnchantmentLevelEvent`. One bit each, in this order (bits 0–10).
3. **Items with code.** For each owned item (`BootFacts.ownsItem`), a bit set of the methods its class declares below
   `Item`/`IItemExtension` (walk superclasses up to `Item`; a default method counts only when a mod class overrides it):
   bit 0 `onCraftedBy(ItemStack, Player)`, 1 `getCraftingRemainder(ItemInstance)`, 2 `getEnchantmentLevel`, 3
   `getAllEnchantments`, 4 `supportsEnchantment`, 5 `isPrimaryItemFor`, 6 `isCombineRepairable`, 7 `getXpRepairRatio`,
   8 `canGrindstoneRepair`, 9 `getMaxDamage`, 10 `isDamageable`, 11 `applyEnchantments`. Write `(item id, bits)` for items
   with any bit.
4. **Loot modifiers.** From the `LootModifierManager` (`ServerLifecycleHooks.getCurrentServer().getServerResources()
   .managers().getListener(...)` as `CommonHooks.modifyLoot` reaches it; javap `CommonHooks` for the key) take
   `getSortedModifiers()`; for each: its id (`getId`), and the table ids it can apply to: if it is a `LootModifier` whose
   condition is a `LootTableIdCondition`, or an `all_of` (`CompositeLootItemCondition.terms`) with one, the set of those
   `targetLootTableId`s; otherwise "any". Fields are private: add accessor mixins in `B/mixin/` (`LootModifierAccess`
   `@Accessor("condition")`, `LootTableIdAccess` `@Accessor("targetLootTableId")`, `CompositeConditionAccess`
   `@Accessor("terms")`). Also collect each modifier's condition types other than `neoforge:loot_table_id`,
   `minecraft:random_chance`, `minecraft:all_of`, `minecraft:any_of`, `minecraft:inverted` and log them once
   (`Horizon: loot modifier <id> tests <types>`): DESIGN §8 risk 2.
5. **Owned loot kinds.** Ids of owned entries of `LOOT_FUNCTION_TYPE` and `LOOT_CONDITION_TYPE` (use
   `BuiltInRegistries.LOOT_FUNCTION_TYPE`/`LOOT_CONDITION_TYPE` and `BootFacts.owns`).

Wire (big-endian, `DataOutputStream`): `int recipeTypeBits`, `int eventBits`, `int nItems` then `(text id, int bits)*`,
`int nModifiers` then `(text id, bool any, int nTables, text table*)*`, `int nFunctions` + `text*`, `int nConditions` + `text*`.

## Rust: `G/java_crafting.rs` (new, `#[cfg(feature = "java-mods")]` module in `game.rs` beside the other `java_*`)

```rust
pub(super) struct CraftFacts { recipe_types: u32, events: u32, items: HashMap<Arc<str>, u32>, modifiers: Vec<Modifier>, functions: HashSet<String>, conditions: HashSet<String> }
pub(super) struct Modifier { pub id: String, pub tables: Option<HashSet<String>> }  // None: any table
static FACTS: OnceLock<CraftFacts> = OnceLock::new();
```
- `fn parse(bytes: &[u8]) -> Option<CraftFacts>` with `In`.
- Accessors: `pub(super) fn craft_facts() -> Option<&'static CraftFacts>`; `has_recipe_code(Station)`, `heard(Event)`,
  `item_bits(&str) -> u32`, `modifiers_for(table: &str) -> bool` (any modifier with `None` or a set holding the id),
  `owns_function(&str)`, `owns_condition(&str)`. `Station` and `Event` are small enums whose discriminants are the bits
  above (one table, with a test that the order matches the Java constants' names).
- Loaded in `java_world_phase(open = true)` right after `entity_types`: `with_active(self, || java_crafting::load())`, which
  requests `craft_facts` once per process (`FACTS.set`); a second server start in the same process keeps the first answer
  (the jars and data can't change in a process).
- Without the `java-mods` feature nothing exists; `java_hooks.rs` stubs added by later specs answer "no code".

## Test mod (`TM/`)

Register in `TestMod` (new class `TM/.../Crafting.java` with `static void register(IEventBus bus, DeferredRegister.Items items)`):
- **Serializer** `horizon_java_test:named_copy`: a `ShapedRecipe` subclass whose `assemble` copies the first ingredient's
  `custom_name` onto the result and whose `getRemainingItems` leaves one `minecraft:stick` where a `minecraft:paper` was.
  Data: `TM/res/data/horizon_java_test/recipe/named_copy.json` = shaped `"P"` over `"#"`: `P` = `minecraft:paper`,
  `#` = `minecraft:name_tag`, result `minecraft:name_tag` x1 (serializer registered through `DeferredRegister` for
  `Registries.RECIPE_SERIALIZER`). Java data lives under `TM/res/` (`java_mods.py build` packs `res/` into the test jar);
  `TM/content/` is the Horizon content mod the runs pass as `--dev-mod`.
- **Item** `horizon_java_test:charm`: overrides `onCraftedBy(ItemStack, Player)` to set `custom_data {crafted_by: <name>}`,
  `getEnchantmentLevel` to answer 3 for `minecraft:fortune` and 1 for `minecraft:silk_touch` (else super), and
  `getAllEnchantments` to match (super's plus those two). Recipe `TM/res/data/horizon_java_test/recipe/charm.json`:
  shapeless `minecraft:gold_nugget` + `minecraft:string` → `horizon_java_test:charm`.
- **Listener** of `ItemCraftedEvent` (server side only): counts per result item id into `Crafting.CRAFTED` and records the
  craft matrix's item count at the event (`event.getInventory()` non-empty slots) into `Crafting.MATRIX`.
- **Loot modifier** type `horizon_java_test:add_item` (`LootModifier` with a `item` field: adds one of it) and data
  `TM/res/data/horizon_java_test/loot_modifiers/sand.json`: condition `neoforge:loot_table_id` =
  `minecraft:blocks/sand`, item `minecraft:gold_nugget`.
- **Loot function** type `horizon_java_test:stamp` (sets `custom_data {stamped: 1}`) and **condition**
  `horizon_java_test:always` (true); data: `loot_table/blocks/dropper.json` for the test mod's `dropper` block (one pool,
  condition `horizon_java_test:always`, entry `horizon_java_test:dropper` with function `horizon_java_test:stamp`), written
  identically under `TM/res/data/horizon_java_test/` (Java's) and `TM/content/data/horizon_java_test/` (the table native
  rolls). Write it in 26.3's loot form (an entry's `modifier` object with `type`, a pool's `condition`), as
  `.local/horizon-client/26.3/data/minecraft/loot_table/blocks/shulker_box.json` does; jars of older ports still say
  `functions`/`function`, which carry upgrades. The loot modifier file is NeoForge's format (`type`, `condition`, fields),
  as in travelersbackpack's `data/travelersbackpack/loot_modifiers/*.json`.

## Tests

- **Java unit** `U/CraftFactsCheck.java` (in `java_mods.py unit`): with vanilla bootstrapped as the other unit checks do,
  `CraftFacts.recipeHasCode` is false for a vanilla `ShapedRecipe` built from `Items.STICK` and true for one whose
  ingredient is a NeoForge custom ingredient (`DataComponentIngredient`); `listened` is false for an event class with no
  listener on a fresh `BusBuilder` bus and true after `addListener`. Independent values: the checks build the inputs.
- **Test mod check** `ChecksServer` gains `craft_facts`: the facts the server answers hold bit `CRAFTING` (the test mod's
  serializer), event `ItemCraftedEvent`, item `horizon_java_test:charm` with bits 0, 2, 3, a modifier with tables
  `{minecraft:blocks/sand}`, function `horizon_java_test:stamp`, condition `horizon_java_test:always`.
- **Rust** `java_crafting::tests`: `parse` of a hand-written byte buffer (built with `Out` in the test) gives the expected
  struct; `modifiers_for("minecraft:blocks/sand")` true, `("minecraft:blocks/dirt")` false, true for any id once an "any"
  modifier is present; the `Event`/`Station` discriminant table matches the Java order (a `const` list of names in both).
- `java_mods.py build`, `check`, `unit`, `bridges`; `cargo test -p horizon-client --features java-mods --lib java_crafting`;
  clippy with and without the feature.

## Done bar

- [ ] On a java-mods build, `install-rs` world start logs `Horizon: <n> recipes of mods' code in minecraft:crafting` with
      n > 0 and no `craft_facts` error; on the showcase install, a `loot modifier ... tests` line names travelersbackpack's
      `hay_backpack_modifier` only if its condition has a non-trivial type (it has none: expect no line for it).
- [ ] `bridges` prints `CHECK craft_facts ok`.
- [ ] Unit and Rust tests above pass; default build (no feature) compiles.

## Live steps for the lead

1. `java_mods.py launch --install /home/umceko/hz-poc/agents/w12/install --world W11a` (scratch homes as RULES), wait for the
   world, `python3 tools/logsum.py <log> --grep "recipes of mods' code|loot modifier"`.
   Expect `crafting` with a few hundred recipes, `smithing` with at least 1 (Sophisticated Backpacks); no exception.
