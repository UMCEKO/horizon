# SPEC-3 (W11): crafting and smithing results from Java's RecipeManager

Read first: `DESIGN.md` G2, G8, D1, D2; `SPEC-1.md`; vanilla `CraftingMenu.slotChangedCraftingGrid`, `CraftingMenu.slotsChanged`,
`InventoryMenu.slotsChanged`, `AbstractCraftingMenu`/`RecipeBookMenu` `finishPlacingRecipe` (the `recipeHint`),
`SmithingMenu.createResult`, `slotsChanged`, `mayPlace` (`RecipePropertySet`), `RecipeCraftingHolder.setRecipeUsed`;
`L/menu.rs` `Env`, `recompute`, `smith_result`, `smith_error`, `may_place` (`Role::SmithInput`); `L/stations.rs`
`smithing_accepts`; `G/lan_crafting.rs` `menu_env`, `lan_inventory_click`, `lan_crafting_click`, `lan_place_recipe`;
`G/lan_stations.rs` `lan_station_op` (the `recomputed` check, `native_prepare_smithing`); `B/GameBridge.java` `level`,
`decode`, `writeTag`.

Depends on: SPEC-1 (`has_recipe_code(Station::Crafting | Station::Smithing)`). SPEC-4 completes it (the result take).

Comments in code: one line at most, only for a non-obvious why.

## Goal

When SPEC-1 says a station's recipe type holds mod code, that station's result is computed by Java's real
`RecipeManager.getRecipeFor(type, input, level, hint)` and `assemble` (NeoForge's order and priorities, mod recipe classes,
custom ingredients), right after the click that changed the inputs, where native mods' result hooks already run. Native
doesn't match at all in that case (one owner). Without mod code, nothing changes.

## Wire: op `craft_result`

Request: `text dim`, `long time`, `byte station` (0 crafting, 1 smithing), `int width`, `int height`, then `width*height`
stacks (crafting, row-major) or 3 (smithing: template, base, addition), each `int len` + NBT (`len` 0 = empty), then
`text hint` ("" = none). Reply: `bool found`; if found `text recipe id`, `bool special` (`Recipe.isSpecial`), `int len` + NBT
of the result (0 when `assemble` gave empty or the item isn't enabled: `isItemEnabled(level.enabledFeatures())`).

## Java: `B/Crafting.java` (new)

`static void result(Crossing.In in, Crossing.Out out)`, registered as `Crossing.game("craft_result", Crafting::result)`.
`var level = GameBridge.level(dim)` (as `java_level_call`'s ops do); build `CraftingInput.of(w, h, stacks)` or
`new SmithingRecipeInput(t, b, a)`; `hint` → `ResourceKey.create(Registries.RECIPE, Identifier.parse(hint))`;
`level.getServer().getRecipeManager().getRecipeFor(type, input, level, hintKey)` (the `ResourceKey` overload); `assemble`;
write the reply. With `HORIZON_JAVA_DEBUG`: `Horizon: craft_result <station> -> <id|none>`.

Also op `recipe_sets` (no input): the item ids of `RecipeManager.propertySet(RecipePropertySet.SMITHING_TEMPLATE)`,
`SMITHING_BASE`, `SMITHING_ADDITION`, each as `int n` + `text*` (a property set's `test` is by item: list the items of
`BuiltInRegistries.ITEM` whose default instance it accepts).

## Rust

### `G/java_crafting.rs`
- `pub(super) fn java_craft_result(&mut self, station: Station, width: usize, inputs: &[Option<ItemStack>],
  hint: Option<&str>) -> Option<(String, bool, Option<ItemStack>)>`: writes the request (stacks with
  `java_mods::write_tag(horizon_client::save::item_to_nbt(s))`), `self.java_request("craft_result", ..)`, reads the reply.
- `java_smithing_sets()`: requests `recipe_sets` once, stores `[HashSet<Arc<str>>; 3]` in a `OnceLock` (the data can't
  change in the process); called from `java_crafting::load` (SPEC-1's load point).
- The limited-crafting rule, as `recompute` applies it: when `limited_crafting` and not `special` and the player's book
  doesn't know the id, the result is empty.

### `java_hooks.rs` stubs (compile without the feature)
`java_hook_mod_crafting(&self, Station) -> bool` (false without the feature), `java_hook_craft_result(...)` (None),
`java_hook_smithing_sets(&self) -> Option<&'static [HashSet<Arc<str>>; 3]>` (None).

### `L/menu.rs`
- `Env` gains `pub mod_crafting: bool`, `pub mod_smithing: bool` and
  `pub smithing_sets: Option<&'a [HashSet<Arc<str>>; 3]>` (default false/false/None in every `Env` literal; `menu_env`
  fills them from the hooks).
- `recompute`: with `env.mod_crafting`, set the result to `None` and `self.grid_changed = true` (new pub field), without
  matching. `smith_result`: same with `mod_smithing`. A native quick-move on the result then stops after one craft instead
  of reusing a stale result (SPEC-4 replaces that click).
- Smithing slot acceptance (`Role::SmithInput(i)` in `may_place`, and the quick-move check at l. ~2514): with
  `smithing_sets` set, accept when `sets[i]` holds the stack's item; else today's `smithing_accepts`.
- Make `smith_error` `pub(crate)`.

### `G/lan_crafting.rs`, `G/lan_stations.rs`
- `lan_inventory_click` and `lan_crafting_click`: after the click, if `mod_crafting` and `menu.grid_changed` (clear it), call
  `java_craft_result` with the grid (2x2 or 3x3, `menu.slot(1..)`), apply the limited-crafting rule, set slot 0, then
  `native_prepare_item_craft` as today. Done inside `as_guest` for the guest (the Java call needs the guest's level).
- `lan_place_recipe`: same after the placement, with `hint = Some(&id)`.
- `lan_station_op` for `StationKind::Smithing`: when `recomputed` and `mod_smithing`, call `java_craft_result(Smithing, ..)`
  with the three inputs, set slot 3, then `menu.smith_error()`, then `native_prepare_smithing` as today.

## Scenario support (`T/hz_scenario.py`)

- Step `{"ctl": [ARGS...]}`: one raw horizon-ctl command (for `open-inventory`, `click-slot N`, `interact X Y Z`, `close`).
- Check `{"screen_slot": N, "item": ID, "count": C?}`: reads `horizon-ctl screen` (verify its slot fields in
  `docs/horizon-ctl-commands.md` and the reply) and compares like the `slot` check. Unit tests in `tests/` for both (the
  command list a step makes; a check against a canned `screen` reply).

## Tests

- **Rust, `L/menu.rs`**: with `mod_crafting`, a grid of one oak log leaves slot 0 `None` and `grid_changed` true (native
  would have made planks: assert it does without the flag); `smithing_sets` with `{"mymod:pack"}` in the base set accepts
  `mymod:pack` in slot 1 and refuses `minecraft:diamond_sword` there.
- **Rust, `G/java_crafting.rs`**: the request bytes for a 2x2 grid with one stack match a hand-built `Out`; the reply parser
  reads a hand-built reply.
- **Test mod check** `ChecksServer` `craft_result`: call `Crafting.result` directly with a 2x2 input of a named `name_tag`
  above `paper` (the test mod's `named_copy`) and expect found, id `horizon_java_test:named_copy`, a result named as the
  input (vanilla `ShapedRecipe` would not copy the name: the copy proves Java's class ran); with `minecraft:oak_log` alone
  expect `minecraft:oak_planks` x4 (vanilla's recipe through the same path).
- **Scenario** `T/scenarios/w11-crafting.json` (survival, install-rs): `/give @s ae2:item_storage_cell_1k`,
  `/give @s ae2:cell_component_16k`, `{"ctl": ["open-inventory"]}`, click the two stacks into grid slots 100 and 101
  (`click-slot`), then `{"check": {"screen_slot": 200, "item": "ae2:item_storage_cell_16k"}}`; then clear the grid, put an
  oak log in slot 100: `{"check": {"screen_slot": 200, "item": "minecraft:oak_planks", "count": 4}}`;
  `{"check": {"no_exceptions": true}}`. Run with `HORIZON_JAVA_DEBUG=1` and also check
  `{"log": "Horizon: craft_result crafting -> ae2:"}`.
- `cargo test -p horizon-client --features java-mods --lib java_crafting`, `cargo test -p horizon-client --lib menu::`,
  `python3 -m unittest discover -s tests -p "test_hz_scenario*"`, `java_mods.py build`, `check`, `bridges`; clippy with and
  without the feature; `python3 tools/parity.py` is not needed (no vanilla behaviour changes without mods), but run the
  crafting harnesses that cover `menu.rs` (`tools/parity.py --changed-files` lists them) through `verify/parity.sh`, at most 3.

## Done bar

- [ ] The scenario passes on install-rs: the AE2 cell upgrade shows `item_storage_cell_16k`, planks still work.
- [ ] In a crafting table, Mekanism's Basic Energy Cube (mek_data) shows its result.
- [ ] A pack without mod crafting code (`java_mods.py run --jars` a folder holding only Jade: no mod serializer, no custom
      ingredient) still matches natively: `craft_result` is never logged and the SPEC-1 line for `crafting` is absent.
- [ ] W12: `T/java_routing/deferred.tsv` gains, for any showcase key on `CraftingMenu#slotChangedCraftingGrid` or
      `SmithingMenu#createResult`, `special: W11 SPEC-3` (none today; the lint stays green).

## Live steps for the lead

1. install-rs, survival, crafting table. `/give @s mekanism:energy_tablet`, `/give @s mekanism:steel_casing`,
   `/give @s minecraft:redstone 4`, `/give @s mekanism:ingot_osmium 2` (JEI's recipe for the Basic Energy Cube gives the
   exact layout; use R on the cube). Lay it out: the cube appears in the result slot.
2. Survival inventory: AE2 1k item cell + 16k component in the 2x2 grid: the 16k cell appears.
3. Showcase install, smithing table: a Sophisticated Backpacks diamond backpack in the base slot and a netherite ingot plus
   the upgrade template per its recipe (JEI): the items go into the slots and the netherite backpack appears.
