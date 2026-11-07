# SPEC-12: S-spawn, and mod spawn data from the export

Size: ~250 lines. Depends on SPEC-11. Read DESIGN.md sections 2.2 (biome getters) and 3.1 (the SPAWN row), and section 4 (`spawnOriginalMobs`).

## Goal

1. A new chunk's original mobs (`ChunkStatus.SPAWN`, vanilla's `spawnOriginalMobs`) come from Java when `Seam::Spawn` is routed. lithostitched wraps a call inside it.
2. Natural spawning reads mod biomes' and structures' spawn settings from the export, with NeoForge's biome and structure modifiers applied. That includes MekanismAdditions' `structure_modifier`s.

## Read first

- Vanilla 26.3:
  - `NoiseBasedChunkGenerator.spawnOriginalMobs` (line 474);
  - `NaturalSpawner.spawnMobsForChunkGeneration`;
  - `ChunkStatusTasks.generateSpawn` (skipped when the chunk is upgrading).
- Native: `game/natural_spawning.rs:604` (the ported `spawnOriginalMobs`); `biome.rs` `BiomeInfo::from_json` (spawns from `attributes`); structure spawn overrides in native (`spawn_overrides`).
- SPEC-11's session machinery: reuse its region mirror form.

## Do

1. **Java `wg_spawn(level, center, 3x3 mirrors)`** (`Crossing.free`): builds the `WorldGenRegion` as SPEC-11 does, at the SPAWN step, and calls `generator.spawnOriginalMobs(region)`. Out: the entities added, as NBT (the `MirrorProtoChunk.addEntity` log).
2. **Rust:** where native runs `spawnOriginalMobs` for a chunk, call `wg_spawn` instead when routed, and add the returned entities as native adds its own.
3. **Structure spawn overrides from the export.**
   - SPEC-2 adds, per structure, its spawn overrides from `structure.modifiableStructureInfo().get().spawnOverrides()` (NeoForge's modified view), when they differ from the codec's.
   - Native's structure spawn table reads them from the export's `worldgen/structure` entry.
4. Check that mod biome spawns come from the exported biomes (SPEC-2 writes them from `getMobSettings()`). Add a unit test on an exported biome with a modifier-added spawn.

## Tests you run

- Java `Checks` with no worldgen mod: `wg_spawn` on a plains chunk with a fixed seed gives the same entity types and positions as native's `spawnOriginalMobs`.
- A Rust unit test of reading the structure spawn overrides.

## Acceptance (the parent)

- On the W10 pack, `worldgen_compare.py --chunks 5`: the entity lists (type, rounded position) of the center chunks are equal.
- In the game: `/locate` a structure that MekanismAdditions adds spawns to. Its mobs appear there.

## Not in this spec

Mod entity types (W9).
