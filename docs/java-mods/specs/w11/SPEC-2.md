# SPEC-2 (W11): the mods' recipes as Java loaded them

Read first: `DESIGN.md` G1, D9; `docs/java-mods.md` "Blocks and items from the boot"; `B/BootFacts.java` `export` (how
`blockExtras` encodes loot tables with the codec); `P/carry.rs` `carry` (the recipe branch: `rel.strip_prefix("recipe/")`,
`upgrade_recipe`, `report.recipes_kept`), `P/oracle.rs` `Facts`, `P/pipeline.rs` `carry_boot`, `G/java_content.rs`
(`GENERATION`), `W/conditions.rs`. Vanilla: `RecipeManager`, `Recipe.DIRECT_CODEC`.

Depends on: nothing. Lands alone.

Comments in code: one line at most, only for a non-obvious why.

## Goal

The recipes Horizon loads for a carried mod are exactly the ones Java's real data load kept, in the form Java's codec writes:
conditions (mod condition types included) are resolved by NeoForge, once, at boot. A jar recipe Java didn't keep is not
carried; a recipe Java kept whose file has conditions native can't read is carried without them.

## Java: `B/BootFacts.java`

In the anonymous `FactsWriter`'s `run()`, after `tags()`, call a new `recipes()`:
```java
void recipes() throws IOException {
    json.name("recipes").beginArray();
    for (var holder : GameBridge.server().getRecipeManager().getRecipes()) { ... }
    json.endArray();
}
```
For each holder whose id namespace isn't `minecraft`, sorted by id: an object `{"id", "type" (recipe type id),
"serializer" (serializer id), "json"}` where `json` is `Recipe.DIRECT_CODEC.encodeStart(ops, holder.value())`; on an
encode error write `"error": <message>` instead of `json`. Bump `schema` to 4.

## Rust

- `P/oracle.rs`: `Facts` gains `#[serde(default)] pub recipes: Option<Vec<FactRecipe>>`, with
  `pub struct FactRecipe { pub id: String, #[serde(rename = "type")] pub kind: String, pub serializer: String,
  #[serde(default)] pub json: Option<Value>, #[serde(default)] pub error: Option<String> }`. Oracle facts (no key) give
  `None`, so `neoforge_oracle.py` runs and old caches carry as today. Check `Facts::read` accepts schema 3 and 4.
- `P/carry.rs` `carry`: build `let java: Option<HashMap<&str, &FactRecipe>>` from `facts.and_then(|f| f.recipes.as_ref())`.
  In the recipe branch, for a file `data/<ns>/recipe/<path>.json` whose `ns` is owned:
  - `java` is `None`: today's behaviour.
  - the id `<ns>:<path>` is in `java` with `json`: write that JSON (pretty) instead of the jar's bytes, still through
    `tags_used` and `renamed_ids` (not `upgrade_recipe`: it is 26.3's form already); `report.recipes_from_java += 1`.
  - in `java` with `error`: keep today's behaviour for that file and log `"{id}: Java couldn't encode it: {error}"`.
  - not in `java`: Java's load dropped it (a condition, a parse error): write it to `outside/` and push the file to a new
    `report.recipes_unloaded`.
  After the zip loop, write every `java` recipe of an owned namespace that no jar file named (a mod's code-provided data
  pack) under `inside/data/<ns>/recipe/<path>.json`; `report.recipes_from_java += 1` each.
- `P/carry.rs` `Report`: the two new fields; the summary line in `P/pipeline.rs` (the `"{}: {} files, {} recipes upgraded"`
  format) adds `{} recipes from Java ({} unloaded)`.
- `G/java_content.rs`: `GENERATION` 3 → 4 (carry writes something else from the same jars).
- `W/conditions.rs`: add `neoforge:false` (the condition NeoForge registers next to `true`) to `hold` and `known`: a facts
  file or a pack may carry it, and an unknown kind would drop the file for the wrong reason.

## Tests

- `P/carry.rs` test `java_recipes_replace_the_jars` (pattern: the existing tests that build a jar with `zip::ZipWriter`):
  jar `mymod` with `data/mymod/recipe/a.json` =
  `{"type":"minecraft:crafting_shapeless","ingredients":["minecraft:stick"],"result":{"id":"minecraft:stone"},"neoforge:conditions":[{"type":"mymod:flag"}]}`
  and `data/mymod/recipe/b.json` (any shapeless). Facts (hand-written `Facts` with `recipes`): `mymod:a` with json
  `{"type":"minecraft:crafting_shapeless","category":"misc","ingredients":["minecraft:stick"],"result":{"id":"minecraft:stone","count":1}}`,
  and `mymod:c` (not in the jar) with a shaped recipe. Expect: `inside/data/mymod/recipe/a.json` parses equal to the facts'
  JSON; `b.json` is under `outside/` and not `inside/`; `c.json` is under `inside/`; `recipes_from_java == 2`,
  `recipes_unloaded == ["b.json"]` (or its path form; match the field's convention).
- Same jar with `facts.recipes = None`: today's output (`a.json` kept with its conditions, `b.json` inside).
- `W/conditions.rs`: `neoforge:false` fails a file; `neoforge:not` of it passes.
- Then load the produced folder with `crafting::Recipes::load` (as `L/mod_content_tests.rs` does) and check `mymod:a`
  matches a grid of one stick (`Recipes::find`), proving native reads the codec form.
- `cargo test -p horizon-port-pack`, `cargo test -p horizon-world conditions`, `java_mods.py build`, `check`.

## Done bar

- [ ] A fresh content generation for the showcase install logs `sophisticatedbackpacks: ... recipes from Java (... unloaded)`
      with a non-zero count, and the generated `.horizon-cache/java-mods/<key>/mods/sophisticatedbackpacks/data/
      sophisticatedbackpacks/recipe/upgrade_base.json` has no `neoforge:conditions` key.
- [ ] In that world (survival, a crafting table), Sophisticated Backpacks' Upgrade Base crafts (a `minecraft:crafting_shaped`
      recipe behind `sophisticatedcore:item_enabled`; the backpack itself is the mod's own type: SPEC-3).
- [ ] Tests above pass; the oracle path (`neoforge_oracle.py export` facts) carries as before.

## Live steps for the lead

1. Delete the install's `HORIZON_HOME/.horizon-cache/java-mods`, `java_mods.py launch --install .../w12/install --world W11b`
   (first launch waits on "Loading Java mods").
2. Survival. `/give @s minecraft:string 4`, `/give @s minecraft:iron_ingot 4`, `/give @s minecraft:leather 1`; in a crafting
   table lay out `SIS / ILI / SIS` (S string, I iron, L leather). Expect `sophisticatedbackpacks:upgrade_base` in the result
   slot; take it. Before this spec the slot stays empty.
