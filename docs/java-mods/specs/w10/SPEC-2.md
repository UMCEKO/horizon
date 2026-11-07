# SPEC-2: Java exports the final worldgen registries

Size: ~350 lines (Java ~250, Rust reader ~100). Depends on nothing. Read DESIGN.md section 2.

## Goal

When asked, Java writes one file holding every worldgen registry entry the loaded mods add or change, as the real loader and the modifiers left it. Each entry carries the list of mod-owned types inside it. Rust reads that file into a `WorldgenOverrides` plus an owners table.

## Read first

- `B/GameBridge.java` `prepare`. It runs the real `WorldLoader.load` with NeoForge's mod packs. `registries` is the composite access.
- `B/BootFacts.java` `owns`, `seal`: registry ownership. Never a namespace.
- `crates/horizon-world/src/worldgen/overrides.rs`: `KINDS`, `TAG_KINDS`, `WorldgenOverrides::set`, `add_tag`, the bundle names.
- Vanilla: `RegistryDataLoader.WORLDGEN_REGISTRIES` (the 26.3 list and codecs), `RegistryOps.create(JsonOps.INSTANCE, registries)`.

## Do

### Java

Add the op `worldgen_export` (a `Crossing.game` op: it runs at world start under the lock). Input: an output file path. Body:

1. For each registry of `RegistryDataLoader.WORLDGEN_REGISTRIES` that has a bundle in `overrides.rs` `KINDS` (by folder name), and for `dimension` (from `LEVEL_STEM`), do steps 2 to 6 for every entry.
   - The 26.3 folder names come from the registry key's path: `worldgen/biome`, `worldgen/feature` (configured), `worldgen/placed_feature`, `worldgen/carver`, `worldgen/density_function`, `worldgen/noise`, `worldgen/noise_settings`, `worldgen/structure`, `worldgen/structure_set`, `worldgen/template_pool`, `worldgen/processor_list`, `worldgen/multi_noise_biome_source_parameter_list`, `worldgen/world_preset`, `dimension_type`, `worldgen/material_rule`, `worldgen/material_condition`, `worldgen/block_state_provider`.
   - Every registry the loader has, even one native ignores today, goes in the file. Native decides what it reads.
2. Encode the entry with the registry's element codec through `RegistryOps.create(JsonOps.INSTANCE, registries)`, to JSON text. An encode error is logged once per entry, naming the entry and its class, and the entry is written as `null`.
3. Compare it with vanilla's built-in entry: the same id in a registry access loaded from the vanilla pack alone, built once by a second `WorldLoader.load` with `ServerPacksSource.createVanillaTrustedRepository()` only. Write the entry only when it is new or its JSON differs.
4. **Biomes:** after step 2, replace the JSON's `features` with the lists `biome.getGenerationSettings().features()` gives, written as placed feature ids per step. Replace `carvers` and the spawn settings from `getGenerationSettings().getCarvers()` and `getMobSettings()` the same way. NeoForge's modifiers act through these getters.

   If the codec's output and the getters differ, log it once per biome: `worldgen export: <id> modified by biome modifiers`.
5. **Noise settings:** replace `material_rule` (26.3's name for the surface rule) with the encoding of `settings.materialRule()`, the getter.
6. **Owners:** walk the live object graph of the entry, not the JSON.

   For each object that implements a type-dispatched interface, find its type key by reverse lookup in that interface's type registry: the object's `type()` (or `codec()`) as the registry value. Record `"<type registry>:<key>"` when `BootFacts.owns(typeRegistry, key)` is true. Record `"unregistered:<class name>"` when the lookup finds nothing: a codec or wrapper a mixin installed.

   | Interface | Type registry |
   |---|---|
   | `Feature` (inside `ConfiguredFeature`) | `FEATURE` |
   | `PlacementModifier` | `PLACEMENT_MODIFIER_TYPE` |
   | `BlockStateProvider` | `BLOCKSTATE_PROVIDER_TYPE` |
   | `TrunkPlacer`, `FoliagePlacer`, `RootPlacer`, `TreeDecorator` | their placer and decorator type registries |
   | `BlockPredicate` | `BLOCK_PREDICATE_TYPE` |
   | `DensityFunction` | `DENSITY_FUNCTION_TYPE` (by codec) |
   | `MaterialRule`, `MaterialCondition` | their 26.3 type registries |
   | `BiomeSource` | `BIOME_SOURCE` |
   | `ChunkGenerator` | `CHUNK_GENERATOR` |
   | `Structure` | `STRUCTURE_TYPE` |
   | `StructurePoolElement` | `STRUCTURE_POOL_ELEMENT` |
   | `StructureProcessor` | `STRUCTURE_PROCESSOR` |
   | `PoolAliasBinding` | `POOL_ALIAS_BINDING` |
   | `StructurePlacement` | `STRUCTURE_PLACEMENT` |
   | `WorldCarver` (inside `ConfiguredWorldCarver`) | `CARVER` |
   | `IntProvider`, `FloatProvider`, `HeightProvider` | their type registries |
   | `RuleTest`, `PosRuleTest`, `RuleBlockEntityModifier` | their type registries |

   Walk with reflection over record components and fields, bounded to depth 64. Keep a visited set, and don't descend into `Holder`s of another registry: those are their own entries.
7. Tags: `Registries.BIOME`'s tags whose values differ from vanilla's, as `{"replace": false, "values": [...]}` with the resolved ids.

File format (JSON, written atomically through a temp file and rename):

```json
{"format": 1, "entries": {"<folder>": {"<id>": <json or null>}}, "tags": {"tags/worldgen/biome": {"<tag id>": {"values": [...]}}},
 "owners": {"<folder>": {"<id>": ["<type registry>:<type id>", ...]}}, "warnings": ["..."]}
```

Use `safely("worldgen export", ...)` so one bad entry doesn't stop the rest.

### Rust

In `crates/horizon-world/src/worldgen/overrides.rs`, add `WorldgenOverrides::from_java_export(text: &str) -> Result<(WorldgenOverrides, ExportOwners), String>`:

- It maps each folder to its bundle through `KINDS` and `TAG_KINDS`. Folders native doesn't read are kept in `ExportOwners::unread` for the log.
- `ExportOwners` holds `BTreeMap<(bundle, id), Vec<String>>`, with `fn mod_types(&self, bundle, id) -> &[String]`.
- Add a `java_worldgen_export(path)` wrapper in `G/java_mods.rs` that calls the op. It isn't called from game code yet; SPEC-3 calls it.

## Tests you run

- **Java check:** in the test mod (`mods/horizon/src/java-mods-host/test`), add a data biome `horizon_test:export_biome`, a NeoForge `biome_modifier` adding `minecraft:ore_diamond` to it, and a configured feature of a test `Feature` subclass.

  `Checks` runs `worldgen_export` and asserts four things: the biome is in `entries`; its `features` list holds the diamond ore; the test feature's owners list `minecraft:worldgen/feature:horizon_test:...`; and `minecraft:plains` is absent (unchanged).
- **Rust:** a unit test of `from_java_export` on a small literal file, covering bundle mapping, null entries, tags and owners.
- `java_mods.py check` and `bridges` as usual.

## Acceptance (the parent)

- With the W10 pack, at world start (SPEC-3 wires it): the file lists the 69 `biomesoplenty:*` and the Terralith biomes.
- The `minecraft:overworld` dimension entry exists and differs from vanilla.
- The Mekanism ore placed features appear in vanilla biomes' `features`.
- The owners hold `biomesoplenty:*` feature types and `mekanism:*` feature and placement types.

## Not in this spec

When the export runs (SPEC-3), and native using it (SPEC-4).
