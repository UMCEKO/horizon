# SPEC-3 (W7): fluids as content: declarations, per-state fluids, the carry, the root's fluid table

Read first: `DESIGN.md` §2.1, D2; `crates/horizon-mods/src/content.rs` (`Content`, `Block`, `StateOverride`, `read_declaring`, how `menu_types` files are found and parsed near line 995); `crates/horizon-port-pack/src/carry.rs` (the facts branch near line 1140, `write_decl`), `oracle.rs` (`choice`, the `per_state` loop near line 602); `crates/horizon-client/src/mod_content.rs` (`menu_types`, the `registries` push near line 1870, how `overrides`/`ResolvedOverride` reach the gameplay rows); `docs/modpacks.md` (content format), `docs/native-mods.md` (declarations).

Depends on: SPEC-1, SPEC-2. Comments in code: one line max, only a non-obvious why. Upstream-bound prose (doc text, commit message) must not name NeoForge (DESIGN D2).

## Goal

A mod fluid is a content declaration, and a block state's fluid is a per-state override, so the root every loader reads carries them. Native still ignores them (SPEC-4 reads them).

## Content format (`horizon-mods`)

### `data/<ns>/horizon/fluid/<path>.json` → `Content.fluids: Vec<Fluid>`

```json
{
  "flowing": "<ns>:flowing_<path>",        // its flowing partner, declared in the same content; equal to the id for a still-only fluid
  "bucket": "<item id>" | null,
  "states": {"amount=8,falling=false": {"amount": 8, "falling": false, "source": true, "random": false, "block": "<block id>[level=0]"}, ...},
  "replaceable": "0101...", "replaceable_fluids": ["minecraft:empty", "minecraft:flowing_water", ...],
  "model": {"layer": "TRANSLUCENT", "still": "<sprite>", "flowing": "<sprite>", "overlay": null, "tint": {"kind": "constant", "states": {...}}},
  "overlay_texture": "<texture id>" | null,
  "fog_code": false,
  "fluid_type": {"id": "...", "motion_scale": 0.014, "can_push_entity": true, "can_swim": true, "can_drown": true, "fall_distance_modifier": 0.5, "can_extinguish": false, "can_hydrate": false, "supports_boating": false, "light_level": 0, "density": 1000, "temperature": 300, "viscosity": 1000, "sounds": {"bucket_fill": "..."}}
}
```

- A declaration file declares the still fluid `<ns>:<path>`; its flowing partner is declared by the same file (its `states` include the flowing ones; `source` tells them apart). Pairs keep vanilla's naming freedom: `flowing` is whatever id SPEC-1's facts say.
- `replaceable_fluids` is the list SPEC-1's `fluid_order` gave (all of Java's fluids, in Java's order), copied as names so native can index by name.
- `model` absent: a server-only carry (dedicated server, no client facts). `fluid_type` absent: a fluid without a NeoForge type (native content mods); defaults are vanilla water's numbers from SPEC-1's facts of `minecraft:water`'s type, written into `horizon-mods` as constants with a test that compares them to those facts (`#[ignore]`d, `HORIZON_FACTS`).
- Checks in `Content::check`: ids in the mod's namespaces; `flowing` and `block` ids declared by this content or vanilla; every `states` key parses; `replaceable.len() == states.len() * replaceable_fluids.len() * 12`; at most 64 fluids per mod (`MAX_FLUIDS`).

### Per-state fluid

`StateOverride` gains `pub can_place_liquid: Option<Vec<String>>` (SPEC-1's list, carried as is; same id check as `fluid`) and `pub fluid: Option<StateFluid>` (`{"id": "<fluid id>", "amount": 1..=8, "falling": bool}`), checked: `id` is `minecraft:water`, `minecraft:flowing_water`, `minecraft:lava`, `minecraft:flowing_lava` or a fluid this content declares.

## Carry (`horizon-port-pack`)

- `oracle.rs` `choice`'s per-state loop: when `s.can_place_liquid` is `Some`, insert it as `"can_place_liquid"`; when `s.fluid` is `Some`, insert `"fluid"` into `over` (always, for owned blocks: the template's fluid is derived from its name and would never match). Also for a block whose template is vanilla's but whose state has no fluid while the template's has (a mod block with a water template): insert nothing; SPEC-4's table makes "no override" mean "no fluid" for mod states.
- `carry.rs`, in the facts branch after blocks: for each `facts.fluids` entry with `facts.owns_fluid(id)` whose id is in this mod's namespace and that is a still fluid (`source == id`) or has no partner, write `data/<mod>/horizon/fluid/<path>.json` from the facts (`states` of both fluids merged; `replaceable` concatenated in still-then-flowing order with `states` keyed accordingly; the order is documented in the declaration's Rust doc comment) and `baked.fluids[id]` (model, `overlay_texture`, `fog_code`). A fluid whose partner lives in another namespace is reported in `report.fluids_unknown` and left out (the same rule as blocks).
- `fluid_overlay` (SPEC-2): add `"fluid_overlay": true` to those states' `per_state` (a new `StateOverride` field, `Option<bool>`).
- Report counts: `report.fluids` written.

## Root (`mod_content.rs`)

- `fn fluids(mods: &[ContentMod]) -> Vec<&Fluid>` beside `menu_types`, in mod then id order.
- Push `("minecraft:fluid", ids)` into `registries` (still then flowing per declaration, mods in order), as menu types are, so `registries.json` gives mod fluids protocol ids after vanilla's 0..4.
- Write `horizon/fluids.json` into the overlay root:
  ```json
  {"fluids": [{"id": ..., "source": ..., "flowing": ..., "bucket": ..., "states": ..., "replaceable": ..., "replaceable_fluids": ..., "model": ..., "overlay_texture": ..., "fog_code": ..., "fluid_type": ...}, ...],
   "states": {"<block state id>": ["<fluid id>", amount, falling], ...},
   "overlay_states": [<block state id>, ...],
   "containers": {"<block state id>": ["<fluid id>", ...], ...}}
  ```
  `states` lists every **mod** block state (ids from the rewritten registry) whose resolved override has `fluid`; `overlay_states` those with `fluid_overlay`. With no fluids and no fluid states, write nothing (the root stays the base root when no content mod exists).
- `ResolvedOverride`: carry `fluid` and `fluid_overlay` through; they don't touch the gameplay rows.

## Tests

- `horizon-mods`: a declaration round trip (fixture with a still and a flowing fluid); each refusal of `check` (bad namespace, length mismatch, unknown `flowing`, unknown per-state fluid id).
- `horizon-port-pack`: extend `a_fluid_block_with_an_item_but_a_template_without_one_declares_the_item_alone`'s fixture with SPEC-1 facts for `gem:blood`/`gem:flowing_blood` and a SPEC-2 baked fluid; assert the fluid file's fields and that `level=0` of `gem:blood` has `per_state` `fluid {id: gem:blood, amount: 8, falling: false}` and `level=3` `{gem:flowing_blood, 5, false}` (vanilla: level n<8 → amount 8-n).
- `horizon-client` `mod_content_tests.rs`: a content mod with that declaration produces `horizon/fluids.json` whose `states` key is the rewritten id of `gem:blood[level=0]`, and `registries.json` has `gem:blood` and `gem:flowing_blood` at protocol ids 5 and 6 (vanilla's last fluid is 4: check `minecraft:lava`'s id in the base report as the expected value).

## Done bar

- `cargo test -p horizon-mods content`, `-p horizon-port-pack carry`, `-p horizon-client --lib mod_content` (and the 5 `#[ignore]`d content-root tests, since content changes: `cargo test -p horizon-client-lib --lib mod_content::tests:: -- --ignored`), clippy on the three crates.
- A fresh `play/install-rs` boot's cache root (`.horizon-cache/java-mods/<fingerprint>/`) has `data/mekanism/horizon/fluid/brine.json` and the overlay root's `horizon/fluids.json` lists Mekanism's and Generators' fluids, with `states` for `mekanism:brine`'s 16 states and Mekanism's `fluid_logged=water` states. Bump the generation number in `java_content.rs` so caches rebuild.

## Live test (lead)

`jq '.fluids | length, (.states | length)' <root>/horizon/fluids.json` on both packs; no `content mods not loaded` line in the boot log (`logsum --grep 'content mods not loaded|fluid'`).
