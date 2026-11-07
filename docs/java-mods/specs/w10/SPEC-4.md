# SPEC-4: Native generates from the export; undecodable entries keep their slots; the routing table (product half)

Size: ~450 lines of Rust. Depends on SPEC-3 and P1 (main's data-pack biome table, #4577 and #7528, merged into java-mods). Read DESIGN.md sections 2.3 and 3.2.

## Goal

- Native's generator for a Java-mods world reads, over the prepared bundles: Java's export, then the world's own enabled data packs.
- The exported biomes join the world's biome table and are generated, coloured, saved and synced like data-pack biomes.
- An entry whose tree holds a mod-owned type doesn't break loading. It keeps its id and its place in every order (feature indices, structure steps), and it is recorded as a Java seam in a `WorldgenRoutes` table. The SPECs from 8 on run those seams.

## Read first

- On main after P1:
  - `G/world_loading.rs` (`world_worldgen_overrides`, the biome table via `Biomes::with_packs`, line ~547);
  - `overrides.rs` `finish(known)`;
  - `datapack.rs` `worldgen_overrides`.
- `crates/horizon-world/src/worldgen/generator.rs`: `GeneratorData::load_with`, `Pipeline::new`, `OverworldGenerator::build`, `unsupported_features`.
- `generator_decoration.rs`: `features[id]` holds `Err` for roots that can't run, and `stats.unsupported`.
- `generator_structures.rs` `StructureState::new`, `feature_sort.rs`, `decoration_order.rs`.
- `ExportOwners` from SPEC-2.

## Do

1. **Stacking.** In `world_worldgen_overrides`, start from the export's `WorldgenOverrides` when the hook gives one. Then add the world's enabled packs, as today, so their files win.

   A pack biome or export biome is "known" when the world's biome table has it. Build that table from the export's biomes plus the packs' biomes, with `Biomes::with_packs`, before `finish` runs.
2. **The routing table.** Create `crates/horizon-world/src/worldgen/routes.rs` with `WorldgenRoutes`:

   ```rust
   pub enum Seam { Biomes, Terrain, Fill, Surface, Beard, Spawn, Carver(String), Feature(String), Start(String), Place(String) }
   pub struct WorldgenRoutes { java: BTreeSet<Seam>, why: BTreeMap<Seam, Vec<String>> }
   impl WorldgenRoutes { pub fn routes(&self, seam: &Seam) -> bool; pub fn summary(&self) -> String; }
   ```

   `Place(id)` keys by structure id. The export's owners fill the table at world start, under these rules:
   - Feature: a placed feature with any owner type routes that placed id.
   - Start and Place: a structure with any owner type routes both its ids. That includes its pools' elements and processors, read from the structure's start pool and the pools it reaches, and processor lists.
   - Carver: a configured carver with an owner.
   - Biomes: the level stem's biome source has an owner, or a mod-owned `multi_noise_biome_source_parameter_list` preset is referenced. Same for Fill (the noise settings' router) and Surface (the material rule).

   `why` lists the owner types, for the log.
3. **Loading doesn't fail on routed entries.** In `GeneratorData::load_with` and `Pipeline::new`, a routed entry must not abort loading:
   - **Features:** keep the `Err` slot, as today. Their index and step still count in the decoration plan. That is vanilla's `FeatureSorter` over every biome's lists, unknown or not.
   - **Structures:** a routed structure keeps its set membership and placement (set placement is vanilla-typed unless the set's placement is mod-owned, which routes every structure of the set). Native just doesn't try to generate it.
   - **Biomes, Fill, Surface:** when routed, native skips decoding that part.
   - **A routed seam whose Java side isn't built yet** (SPEC-8 to SPEC-12) fails each chunk's generation with `GeneratorError::Stage(<seam>, "needs Java worldgen (W10 SPEC-n)")`. The error is logged once per seam. The world still opens, so queries that don't need those chunks keep working (`worldgen_compare` biomes and starts).
   - Never silent vanilla output in place of a routed seam.
4. **Log, once per world:** `Horizon: Java mods' worldgen: <n> biomes, <m> placed features, <k> structures from mods; Java runs: <summary>`.
5. **Keep `WorldgenRoutes`** in `VanillaTerrain` (an `Arc`), so the later SPECs reach it from the worker.

## Tests you run

- Unit tests in `routes.rs`: owners turn into seams, and a structure with a mod processor routes Start and Place.
- A `generator.rs` test: a placed feature whose JSON has an unknown type keeps its index, and the next feature's `set_feature_seed` index is unchanged. Use the decoration plan on a two-feature biome with a hand-written bundle.
- The existing datapack and worldgen harnesses for files you touch: `python3 tools/parity.py datapack_worldgen biome_generation worldgen` (at most these).

## Acceptance (the parent)

- **D&T alone** (an install with only `dungeons-and-taverns-6.1.0.jar`, which has no dependencies and is all vanilla-typed): `worldgen_compare.py --seed 1` shows biomes 100% and starts 100%, `nova_structures:*` included.
- **The full W10 pack:**
  - The log lists the seams routed by product: Surface (TerraBlender's rule), the Biomes O' Plenty, Mekanism and Waystones features, and AE2's meteorite.
  - Chunks fail generation with the "needs Java worldgen" messages, once per seam.
  - The export's Terralith biomes are known, and `/locate biome terralith:alpine_highlands` parses.

## Not in this spec

Running any routed seam, and the method half of the routing (SPEC-5).
