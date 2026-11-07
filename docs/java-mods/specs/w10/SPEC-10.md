# SPEC-10: TERRAIN seams (S-terrain, S-surface, S-carver, S-beard)

Size: ~450 lines (Rust ~250, Java ~200). Depends on SPEC-7, SPEC-8 and SPEC-9. Read DESIGN.md sections 3.1, 3.3 and 4 (the `buildTerrain`, `Beardifier` and `materialRule` rows), and decision D4.

## Goal

The TERRAIN stage keeps vanilla's order (fill, surface, carvers) and runs each part where the routing table says.

| Route | Who runs what |
|---|---|
| `Terrain` | Java runs the whole `NoiseBasedChunkGenerator.buildTerrain` on a chunk mirror. Native runs none of the stage |
| `Surface` | native fills, then Java runs the material surface on the mirror, then native carves (unless a carver is routed) |
| `Carver(id)` | native runs the carvers before it in vanilla's order, then Java runs that configured carver, and so on. A run of consecutive routed carvers is one call |
| `Beard` | native's beardifier inputs (rigid boxes, terrain adaptation, junctions) come from Java's `Beardifier.forStructuresInChunk`. Native then evaluates the beardifier density itself |

## Read first

- Vanilla 26.3 `NoiseBasedChunkGenerator`:
  - `buildTerrain` (line 353);
  - `doFill`, `buildSurface` (212), `generateCarvers` (226), `applyCarvingMask`;
  - `createNoiseChunk`, which builds the `Beardifier`;
  - `Beardifier.forStructuresInChunk`.
- `ChunkStatusTasks.buildTerrain` primes the heightmaps after the stage. Native already does this.
- Native: `generator.rs` `generate_with_context` (fill, material surface, carvers in order), `carver.rs` `generate_carving_mask`, `carving.rs`, `structure/start.rs` `beardifier`.
- SPEC-7 mirrors, SPEC-9 `JavaStarts` (the beardifier needs the starts referenced by chunks within radius 1, as `StructureManager.startsForStructure` gives them).

## Do

1. **Java ops,** all `Crossing.free`. Use one `B/mixin/NoiseGeneratorAccess.java` with `@Invoker`s for the private `doFill`, `buildSurface`, `generateCarvers` and `createNoiseChunk`. Never copy their bodies.
   - **`wg_terrain(level, chunk mirror, referenced starts)`:** Java's `generator.buildTerrain(chunk, Blender.empty(), randomState, structureManager.forWorldGenRegion(region), biomeManager, region, possibleBiomes).join()`.
     - `region` is a `WorldGenRegion` whose chunk cache holds the center mirror and the 8 neighbours' biome-only mirrors. Vanilla's `collectPossibleBiomes(region, 1)` reads those, so native sends the 3x3 biomes.
     - Out: the chunk mirror (SPEC-7 whole-chunk form).
   - **`wg_surface(level, chunk mirror)`:** builds the `NoiseChunk` as `createNoiseChunk` does, then calls `buildSurface(chunk, context, randomState, structureManager, biomeManager, biomeRegistry, blender)` with the 26.3 arguments. Out: the changed sections.
   - **`wg_carvers(level, chunk mirror, list of (carver index in the biome's carver list, source chunk))`:** runs vanilla's carver loop for exactly those entries. Each entry is the same iteration of `generateCarvers` (source chunk ring, `setLargeFeatureSeed`), so the random draws match. Out: changed sections and the carving mask.
   - **`wg_beard(level, chunk)`:** `Beardifier.forStructuresInChunk(structureManager, chunkPos)` over Java's starts (SPEC-9's `JavaStarts`, loading native ones by NBT). Out: the piece list (box, adaptation, ground delta) and junctions, as the Beardifier fields hold them.
2. **Rust.** In `generate_with_context`, branch per route, in order:
   - `Terrain`: build the mirror after biomes and call `wg_terrain`. Apply the reply. Skip native fill, surface and carvers.
   - Otherwise: native fill (unless `Fill` is routed, which also implies `Terrain` in practice; treat a lone `Fill` as `Terrain` and log it).
   - Then surface: native or `wg_surface`.
   - Then carvers: walk vanilla's carver iteration; consecutive routed entries go in one `wg_carvers`.
   - `Beard`: before fill, ask `wg_beard` when the chunk has starts in reach (native already knows from references), and build native's `Beardifier` from the reply.
3. **Profiling spans:** `wg_terrain`, `wg_surface`, `wg_carvers`, `wg_beard`, plus their crossings.

## Tests you run

- Java `Checks` with no worldgen mod: `wg_terrain` for chunk 0,0, given native's biomes, gives sections equal to native's own terrain for that chunk. That is the strongest unit check: both are vanilla.
- The same for `wg_surface` on native's filled chunk, and for `wg_beard` near a village start.
- `python3 tools/parity.py chunk_stage structure_density` (native unchanged when not routed).

## Acceptance (the parent)

- On the W10 pack: `worldgen_compare.py --chunks 5` with the decoration step skipped on both sides must be 100% equal. Add `--stage terrain` to SPEC-1's tool: the Forge dump stops chunks at `ChunkStatus.TERRAIN` (`level.getChunk(x, z, ChunkStatus.TERRAIN, true)`, then hash the proto chunk), and the Horizon side hashes protos before decoration.
- Report ms per chunk for `wg_terrain` against native terrain (Profile), as D4 asked.

## Not in this spec

Features and structure placement (SPEC-11).
