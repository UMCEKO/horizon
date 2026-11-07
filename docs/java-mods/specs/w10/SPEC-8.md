# SPEC-8: S-biome, the BIOMES stage in Java on native climate

Size: ~450 lines (Rust ~250, Java ~200). Depends on SPEC-5, SPEC-6 and SPEC-7. Read DESIGN.md sections 2.4, 3.1, 3.3 (`wg_biomes`) and 4 (the TerraBlender rows).

## Goal

When `WorldgenRoutes` routes `Seam::Biomes` for a level, a new chunk's biomes come from Java's real `ChunkGenerator.createBiomes` (vanilla's `doCreateBiomes`, with every mixin on it and on the resolver). The six climate density volumes it samples come from native, which already computes the vanilla router exactly.

Biome queries outside chunk generation run the vanilla method Java has for that query, never native's resolver:
- `/locate biome`;
- structure biome checks;
- `noise_biomes` for maps;
- the spawn search.

## Read first

- Vanilla 26.3:
  - `ChunkGenerator.doCreateBiomes`;
  - `MultiNoiseBiomeSource.createResolverForChunk`, which samples six `DensitySampler` volumes over a `DensityVolume` of the chunk's quarts (4x4x4 cells) and then calls `getNoiseBiome(TargetPoint)` per quart;
  - `Climate.Sampler`, `DensitySampler.sampleVolume`, `DensityBuffer`, `DensityVolume.indexUnchecked`;
  - `BiomeSource.findClosestBiome3d`;
  - `Structure.isValidBiome` (how structure checks read biomes).
- Native: `generator.rs` `biome_chunk`, `noise_biomes`, `find_closest_biome`; `biome_source.rs` `ResolvedBiomeSource::resolver`; `climate.rs`.
- SPEC-6's `WorldgenLevels`, SPEC-7's biome names order.

## Do

1. **Native volumes.** In `climate.rs` or `generator.rs`, add `climate_volumes(pos) -> [Vec<f64>; 6]`. It returns temperature, humidity (vegetation), continentalness, erosion, depth and weirdness for the chunk's `DensityVolume`. That is the same volume `createResolverForChunk` builds: 4 x (height/4) x 4 quarts from the chunk's min block, in `DensityVolume`'s index order. The values are the doubles vanilla's buffers hold, before `Climate.target` quantizes them.
2. **Java `wg_biomes`** (`Crossing.free`). In: level id, chunk x and z, the six volumes. It runs:
   - Make a `MirrorProtoChunk` (SPEC-7) with empty sections for the level's height.
   - Build a `Climate.Sampler` whose six `DensitySampler`s are a `NativeVolumes` sampler: `sampleVolume(buffer, volume)` copies native's array when `volume` equals the chunk's volume. Otherwise, and for `sampleValue`, it delegates to the real sampler from `randomState.createClimateSampler(...)` and counts the call in `Profile` as `wg_biomes off-volume`.
   - Call `generator.doCreateBiomes(Blender.empty(), randomState, chunk)` through an accessor mixin (`B/mixin/ChunkGeneratorAccess.java`: `@Invoker`). Swap the sampler by intercepting `RandomState.createClimateSampler` for this thread only (a `ThreadLocal` set around the call, read by a mixin on `RandomState.createClimateSampler`). The generator's own code, and every mod's wrap of it, then runs unchanged.
   - Out: per section, the biome palette as indexes into the session biome names (SPEC-7).
3. **Rust.** In `Pipeline::generate_with_context`, when `routes.routes(&Seam::Biomes)`, replace the native biome resolve with the `wg_biomes` call. Everything downstream (surface, features, saving) reads the chunk's biomes as usual.

   The call goes through a `WorldgenJava` handle that `VanillaTerrain` holds: a trait object `horizon-world` defines and the game implements with the host, so `horizon-world` stays free of JNI.
4. **Other biome queries** when Biomes is routed:
   - `wg_biomes_at(level, quarts)`: Java `biomeSource.getNoiseBiome(qx, qy, qz, randomState.sampler())` per quart. That is vanilla's call for structure biome checks (`Structure.isValidBiome`) and for maps. Native's `noise_biomes`, and the structure start checks that read biomes, call it when routed.
   - `wg_locate_biome(level, from, radius, horizontal step, vertical step, predicate as a list of biome names)`: Java `findClosestBiome3d`. Native's `/locate biome` and `find_closest_biome` call it when routed.
   - World spawn: if native's spawn finder reads biomes, route its biome reads through `wg_biomes_at`.
5. **Profiling:** `HORIZON_PROFILE` spans `wg_biomes` (Java time) and `wg_biomes crossing` (Rust: encode plus decode).

## Tests you run

- Rust unit test: `climate_volumes` for one vanilla chunk equals what native's own resolver used. Check that the biome palette it gives through a test `WorldgenJava` (which resolves locally) equals today's `biome_chunk`.
- Java `Checks` in the test mod, with no worldgen mod: `wg_biomes` for chunk 0,0 with volumes computed by the real sampler gives the same palette as a direct `createBiomes`. `off-volume` stays 0.
- `python3 tools/parity.py biome_generation biome_chunk` (native unchanged when not routed).

## Acceptance (the parent)

On the W10 pack (after D4, with P2 done so the pack's content loads), run `worldgen_compare.py --seed 1` and a second seed. Biomes must match 100% at the three Y.
- The `Profile` shows `wg_biomes off-volume` at 0, or explains each case.
- `/locate biome biomesoplenty:bayou` from 151 75 155 gives [2839, 75, -1509], as real NeoForge did (DESIGN.md section 1.3).
- Report the `/pregen` chunks per second, against the same pack on NeoForge (Chunky) and against vanilla Horizon.

## Not in this spec

The nether and end use the same path. If their routes are on (TerraBlender's `TheEndBiomeSource` patch), test one point each. Their full acceptance comes with the pack's nether and end comparisons, run with SPEC-10 and SPEC-11.
