# SPEC-6: Java's generation context per level

Size: ~350 lines (Java ~250, Rust ~100). Depends on SPEC-3. Read DESIGN.md section 3.3 ("Java's generation context") and section 3.4.

## Goal

Each Java level view (overworld, nether, end) gets vanilla's generation objects at world start, built the way vanilla builds them, so mods' patches of that construction run:
- the level stem's `ChunkGenerator`;
- the `RandomState`;
- the `ChunkGeneratorStructureState`;
- `StructureManager` with `StructureCheck`;
- the server's `StructureTemplateManager`.

`level.getChunkSource().getGenerator()` and `randomState()` return them. That serves the `wg_*` ops and mods that query the generator themselves (Nature's Compass).

## Read first

- `B/ServerView.java` `attach` (the view's `ServerChunkCache` and `ChunkMap` are built by serialization constructors) and `openWorld`.
- `B/GameBridge.java` `level(id)`, `lifecycle` (after SPEC-3: `server_about_to_start`).
- Vanilla: the `ChunkMap` constructor (lines 154 to 200: `RandomState.create`, `createState`); `ServerChunkCache` constructor; `ServerLevel` constructor (`structureCheck`, `structureManager`); `StructureCheck`; `ChunkScanAccess`.
- `docs/java-mods-map.md` sections 3 (views) and 3 "Bridged natives".

## Do

1. **A real `ChunkMap` per level.** In `server_about_to_start`, after `openWorld`, build one per level through the real constructor, so `ChunkMap.<init>`'s mixins run (lithostitched wraps `RandomState.create` there). The arguments:
   - **`levelStorage`:** a `LevelStorageSource.LevelStorageAccess` on a scratch save named `horizon_java_gen`, under the world folder's `horizon_java/` directory. The constructor only derives paths from it. Nothing is ever saved through this map: native saves.
   - **`dataFixer`:** the server's.
   - **`structureManager`:** the server's `StructureTemplateManager`. If the view has none, build it from the server's resource manager as `MinecraftServer` does.
   - **Executors:** `Util.backgroundExecutor()`. The main thread executor is the server view's.
   - **`chunkGetter`:** the view's chunk source.
   - **`generator`:** the `LevelStem`'s generator from `server.registryAccess()` (after TerraBlender's init in `ServerAboutToStart`).
   - **Listener:** a no-op status listener. Ticket storage: a new one. View distance: the server's. `syncWrites`: false.

   Then copy the generation fields into the view's own `ChunkMap`: `generator`, `randomState`, `chunkGeneratorState`, `worldGenContext`. Keep the view's `ChunkMap` instance, because `Tracking` and `EntityViews` use it. Keep the real one in a `WorldgenLevels` map (new class `B/WorldgenLevels.java`) for the `wg_*` ops.

   If the constructor throws, log `Horizon: Java worldgen context for <level> failed: <e>` and fail the world open. Don't fall back.
2. **`StructureCheck` and `StructureManager`.** Build `new StructureCheck(scan, registryAccess, templateManager, dimension, generator, randomState, level, biomeSource, seed, dataFixer)`, with the 26.3 arguments. Set the view `ServerLevel`'s `structureCheck` and `structureManager` (`new StructureManager(level, worldOptions, structureCheck)`).

   `scan` is `NativeChunkScan implements ChunkScanAccess`. Its `scanChunk(pos, visitor)` visits the chunk's `structures` compound (`starts` and `References`) that native has for that chunk: saved, or in memory if generated.

   Add a bridged native `NativeChunkScan.structures(String level, long chunk) -> byte[]` (NBT, empty when none) in a new family `G/java_bridges/worldgen.rs`. It reads native's chunk structure data for the level by dimension id, through the server's levels (`G/levels.rs`).
3. **`wg_probe`** (a `Crossing.free` op): input level id and a list of x, z points. It returns:
   - the generator class;
   - the biome source class;
   - `getBaseHeight(x, z, WORLD_SURFACE_WG)` per point;
   - `getNoiseBiome(qx, qy, qz, randomState.sampler())` at y 64;
   - the `RandomState` router's `finalDensity` class.

   In Rust: `horizon-ctl worldgen probe <level> x,z ...` (extend SPEC-1's arm). It's a debugging tool, and SPEC-8 to SPEC-10 use it in their reports.
4. **Threads.** The objects are vanilla's own, shared between threads as vanilla shares them. Add no lock.

## Tests you run

- The test mod's `Checks`: after world start, the overworld view has a `NoiseBasedChunkGenerator`, `level.getChunkSource().randomState()` is not null, and `structureManager()` answers `hasAnyStructureAt` without throwing.
- `java_mods.py check`, `bridges` (the new bridge).
- A Rust unit test of the structures bridge's NBT selection on a hand-made column.

## Acceptance (the parent)

- On a vanilla-mods world: `horizon-ctl worldgen probe minecraft:overworld 0,0 151,155 1000,-1000` gives heights equal to native's `surface_sample` and to the Forge dump (SPEC-1).
- On the W10 pack: the probe shows TerraBlender's effect. `getNoiseBiome` at the quart of 2839, 75, -1509 gives `biomesoplenty:bayou`, the biome real NeoForge's `/locate` found there (DESIGN.md section 1.3). Let the probe take an optional y per point.
- No file is written under the world folder except `horizon_java/` (`find` after the run).

## Not in this spec

Any generation through these objects (SPEC-8 on).
