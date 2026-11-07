# SPEC-9: S-start, structure starts in Java; native keeps their NBT; `/locate structure` for routed structures

Size: ~450 lines (Rust ~250, Java ~200). Depends on SPEC-5 and SPEC-6. Its full-pack acceptance also needs SPEC-8, because start biome checks read routed biomes. Read DESIGN.md sections 3.1, 3.3 (`wg_start`), 3.5 and 4.

## Goal

For a structure routed `Start(id)`, native's per-chunk start generation calls Java's real `ChunkGenerator.tryGenerateStructure` for that structure, with the same placement inputs vanilla uses. It stores the start's NBT as the chunk's start, untouched. Unrouted structures stay native.
- References, saving and loading work for Java starts as for native ones. Java rebuilds a start object only when a Java seam needs it.
- `/locate structure` for a routed structure runs Java's `findNearestMapStructure`.

## Read first

- Vanilla 26.3:
  - `ChunkGenerator.createStructures`: the loop over `generatorState.possibleStructureSets()`, `placement.isStructureChunk`, the single-structure and weighted-selection branches, `tryGenerateStructure`;
  - `StructureStart.createTag` and `loadStaticStart`;
  - `ChunkGenerator.findNearestMapStructure`.
- Native:
  - `generator_structures.rs` (`StructureState`, `starts_at`, `use_saved_start`, `locate`, `references`, the shard cache);
  - `structure/start.rs` (`StructureStart` and its NBT, `beardifier`);
  - `structure/start_checks.rs`;
  - `test_structure_start_nbt_parity.py`.
- SPEC-6 (`WorldgenLevels`, `StructureCheck`, `NativeChunkScan`).

## Do

1. **Per set, as vanilla does.** In `starts_at`, find the place where native handles one structure set for one chunk:
   - For a set with one structure: if that structure is routed, call `wg_start(level, chunk, structure id)` instead of native's generation.
   - For a weighted set: vanilla draws the selection with the set's random, then tries candidates in order (`tryGenerateStructure` until one succeeds). Native keeps the draw and the candidate order, and calls Java only for routed candidates. Java's answer (a start or none) decides whether the loop goes on, exactly as vanilla's boolean does.
2. **Java `wg_start`** (`Crossing.free`). In: level, chunk, structure id, the set id, and the `StructureCheck`-free inputs. It calls the real `tryGenerateStructure(structureSelectionEntry, structureManager, registryAccess, randomState, templateManager, seed, chunk, chunkPos, sectionPos, dimension)` (26.3 signature, through an `@Invoker`). The structure manager is `level.structureManager().forWorldGenRegion`-free: start generation reads no region.
   - Out: `bool generated`, then `StructureStart.createTag(StructurePieceSerializationContext.fromLevel(level), chunkPos)` as NBT.
   - The `StructureStart` object goes into `JavaStarts` (new, `B/JavaStarts.java`): a `ConcurrentHashMap<(level, chunk, structure), StructureStart>`, evicted when native unloads the chunk (`java_chunks_leaving` already exists: add the eviction there).
3. **Native storage.** A Java start is a native `StructureStart` variant `Opaque { id, chunk, bbox, pieces_bbox: Vec<BoundingBox>, nbt }`.
   - The bounding boxes are read from the NBT's `BB` and pieces' `BB`, which is all references and the beardifier gate need.
   - Saving writes the NBT as it came. Loading a saved chunk with a start whose structure is routed keeps it `Opaque`, never decoding pieces natively.
   - Unknown structure ids in saves stay opaque too, never dropped.
4. **Java needs a start it didn't make** (S-place, S-beard, locate after a reload): `JavaStarts` misses. It then reads the chunk's start through `NativeChunkScan.structures` (SPEC-6) and calls `StructureStart.loadStaticStart`. This is the only path; there's no second cache.
5. **Locate.** When a `/locate structure` target list holds a routed structure:
   - Native calls `wg_locate_structure(level, from, radius, ids)`, which runs Java's `findNearestMapStructure(level, holderSet, pos, radius, false)`.
   - When the list mixes routed and unrouted structures, Java runs the whole query, because vanilla's search interleaves them by distance.
   - The result goes back as structure id plus position.
6. **References:** native's `references` reads `Opaque` boxes as it reads native ones. No Java call.

## Tests you run

- Rust unit tests:
  - an `Opaque` start round-trips through save and load byte-identical;
  - references from an `Opaque` box equal those from the same native start;
  - in a weighted set where candidate 1 is routed and returns none, native tries candidate 2.
- Java `Checks`: `wg_start` for `minecraft:village_plains` at a chunk the vanilla oracle knows (`VanillaStructureStartOracle` gives one) returns NBT equal to what native makes there, with no worldgen mod loaded.
- `python3 tools/parity.py structure_start_nbt structure_layout` (native unchanged when not routed).

## Acceptance (the parent)

On the W10 pack, `worldgen_compare.py --seed 1 --starts 64` (and a second seed) shows starts 100% equal (id, chunk, box, pieces) to real NeoForge, including `minecraft:village_*`, `nova_structures:*`, `terralith:*` and `ae2:meteorite`.
- `/locate structure minecraft:village_plains` from 151 75 155 gives [240, ~, 128].
- `/locate structure terralith:desert_outpost` gives [9936, ~, -8976] (DESIGN.md section 1.3).

## Not in this spec

Placing the pieces (S-place, SPEC-11), and the beardifier (SPEC-10).
