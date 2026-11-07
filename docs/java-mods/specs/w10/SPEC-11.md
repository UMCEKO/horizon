# SPEC-11: The FEATURES session (S-feature runs and S-place)

Size: ~500 lines (Rust ~280, Java ~220). Depends on SPEC-7 and SPEC-9. Read DESIGN.md sections 3.1, 3.3 (`wg_decorate`, "Mirrors") and 6.

## Goal

Native keeps running a chunk's decoration in vanilla's order. Within each step that means structure placement first, then placed features by index. Each item routed to Java (`Feature(id)` or `Place(structure)`) runs in Java, on a real `WorldGenRegion` over a mirror of the same 3x3 region, with the random state vanilla would have for that item.
- Consecutive Java items form one call.
- Each side sends only its writes since the last switch.
- A chunk with no routed item makes no Java call.

## Read first

- Vanilla 26.3 `ChunkGenerator.applyBiomeDecoration`:
  - `setDecorationSeed`;
  - per step: the structures of the step (`random.setFeatureSeed(decorationSeed, index, step)`, `startsForStructure`, `placeInChunk` with `getWritableArea`);
  - then the features of the step from `featuresPerStep` (`FeatureSorter`) filtered by the possible biomes, each with `setFeatureSeed` and `placeWithBiomeCheck`;
  - `PlacedFeature.placeWithBiomeCheck`.
- Native: `generator_decoration.rs` `decorate` (the two closures passed to `self.plan.decorate`, one for structures and one for features), `decoration_order.rs`, `feature_region.rs` (the region `World`), `write_zone.rs`.
- SPEC-7: mirror, `WriteLog`, `MirrorProtoChunk`, and native write recording. SPEC-9: `JavaStarts`.

## Do

1. **Java ops,** `Crossing.free`, per worker thread:
   - `wg_decorate_open(level, center, header, 9 chunk mirrors)`: builds nine `MirrorProtoChunk`s and a `WorldGenRegion(level, cache, ChunkPyramid.GENERATION_PYRAMID.getStepTo(ChunkStatus.FEATURES), center)`. Build the `StaticCache2D<GenerationChunkHolder>` over holders that return the mirrors; see `WorldGenRegion`'s 26.3 constructor. Returns a session id, kept in a per-thread map.
   - `wg_decorate_run(session, native write log, items)`: applies native's log to the mirrors, without logging it back. Then, for each item in order:
     - a feature `(step, index, placed id)`: `random.setFeatureSeed(decorationSeed, index, step)`, then `placedFeature.placeWithBiomeCheck(region, generator, random, origin)`;
     - a placement `(step, index, structure id)`: `random.setFeatureSeed(...)`, then for each start of that structure referencing the center: `start.placeInChunk(region, structureManager, generator, random, writableArea, chunkPos)`.

     `random` is a `WorldgenRandom` of the level's kind: `XoroshiroRandomSource`, or legacy when the noise settings say `legacy_random_source`. `decorationSeed` is `random.setDecorationSeed(seed, minBlockX, minBlockZ)`, as vanilla computes it once per chunk.
     - Out: Java's `WriteLog` since the run began.
   - `wg_decorate_close(session)`: frees it. Native always calls it, also after an error.
2. **Rust,** in `generator_decoration.rs` `decorate`:
   - When the chunk's plan holds any routed item: open the session lazily, at the first routed item, with the 9 protos encoded (SPEC-7). Start native's write recording.
   - Collect consecutive routed items, across a step boundary too, as long as no native item comes between. Then call `wg_decorate_run` with native's log since the last call.
   - Apply Java's log to the region through native's own writer paths, with the same effects as native's writes: blocks, block entities, entities, ticks, post-processing.
   - Close at the end.
   - Items keep their native step and index from the plan. Routed items whose feature native couldn't decode are still in the plan (SPEC-4).
3. **The biome filter.** Vanilla filters a step's features by the biomes possible in the chunk. Native's plan already does that. Java's `placeWithBiomeCheck` checks the biome again at the origin, which is vanilla's own check.
4. **Starts for S-place:** Java gets them from `JavaStarts` (SPEC-9). Native-generated starts are loaded from native's NBT through `NativeChunkScan`, the one path.
5. **Profiling:** `wg_decorate` runs per chunk, Java time, and log bytes in and out.

## Tests you run

- Java `Checks` with no worldgen mod: open a session on a native 3x3 region at a known chunk. Run `minecraft:ore_diamond_medium` (or any vanilla ore) through `wg_decorate_run` at its real step and index. The Java log equals the blocks native's own placement of that feature writes there: the same seed, so the same blocks.
- Rust unit test: run grouping. A plan `[N, J, J, N, J]` gives two Java calls with items `[1, 2]` and `[4]`, and the native logs between them.
- `python3 tools/parity.py worldgen chunk_generation` (native unchanged when nothing is routed).

## Acceptance (the parent)

- On the W10 pack, `worldgen_compare.py --seed 1 --chunks 5` and a second seed: the center chunk's section hashes and block entities are 100% equal. Neighbours may differ only at features crossing into chunks generated in another order; report which.
- Profile: Java runs per chunk, and decoration ms per chunk against NeoForge (`/pregen` against Chunky).
- A user playtest (visual): fly over a Biomes O' Plenty forest, a Waystones village and a Mekanism ore seam at the coordinates `/locate` gives. They should look as on NeoForge. Put a side-by-side through `tools/mod_screens.py` with the same seed in the report.

## Not in this spec

Original mob spawning (SPEC-12), and mod dimensions.
