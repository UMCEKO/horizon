# SPEC-5: `worldgen_closure.py` and the method half of the routing table

Size: ~400 lines (Java devtool ~200, Python ~60, Java boot ~60, Rust ~80). Depends on SPEC-4 and on W12's boot fact (the set of vanilla methods a loaded mod patches). Read DESIGN.md sections 3.2 and 4.

## Goal

A seam runs in Java when a loaded mod patches a vanilla method that the seam can reach. Reachability is read once, from the vanilla jar's call graph. The patched set comes from W12. Nothing here names a mod or a mod class.

## Read first

- `tools/animate_closure.py` and `mods/horizon/src/java-mods-host/devtools/dev/horizonlab/javamods/devtools/Closure.java`. This is the same technique, for `animateTick`. Copy its structure.
- `B/Animate.java` `route`, `wholeLoop`: how the TSV is read at boot.
- `routes.rs` from SPEC-4.
- W12's design (`/home/umceko/hz-poc/agents/w12/DESIGN.md` once it exists) for the fact's name and API. Expected: Java `Routing.patched(String method)` and Rust `Host::patched(&str)`, keyed like `replaces.json`: `net/minecraft/...#name`.

## Do

1. **The devtool.** Add `devtools/.../WorldgenClosure.java`. Over the server jar (`.local/minecraft/26.3/jars/server.jar`), it builds the call graph with virtual dispatch resolved to every override in the jar, plus lambdas (`invokedynamic` bootstrap targets). For each seam root it emits the methods reachable from it, one row each: `seam<TAB>type<TAB>class<TAB>method<TAB>descriptor`. The `type` column is empty unless the seam is per type.

   | Seam | Roots |
   |---|---|
   | `biomes` | `ChunkGenerator.createBiomes`, `ChunkGenerator.doCreateBiomes`, `BiomeSource.getNoiseBiome`, every `createResolver*` lambda |
   | `fill` | `NoiseBasedChunkGenerator.doFill`, `RandomState.create` |
   | `surface` | `NoiseBasedChunkGenerator.buildSurface` (into `material/MaterialSystem.buildSurface`), `NoiseGeneratorSettings.materialRule` |
   | `terrain` | `NoiseBasedChunkGenerator.buildTerrain` itself: only methods called directly by it and not by the parts above |
   | `beard` | `Beardifier.forStructuresInChunk` |
   | `spawn` | `NoiseBasedChunkGenerator.spawnOriginalMobs` |
   | `carver:<carver type>` | each vanilla `WorldCarver` subclass's `carve`, as `NoiseBasedChunkGenerator.generateCarvers` calls it |
   | `feature:<feature type>` | each vanilla `Feature` subclass's `place`/`placeWithBiomeCheck`, plus `PlacedFeature.placeWithBiomeCheck` |
   | `start:<structure type>` | each vanilla `Structure` subclass's `findGenerationPoint`/`generate`, and `ChunkGenerator.tryGenerateStructure` |
   | `place:<structure type>` | `StructureStart.placeInChunk`, each `Structure.afterPlace`, and every `postProcess` of the piece classes that type's generation creates. Find them as the `StructurePiece` subclasses constructed under that type's `start` closure |

   Stop descending at `net.minecraft.world.level.levelgen.synth` noise primitives and at `java.*`. Those rows would only be noise: a mod patching them is routed by its own seam.
2. **The tool.** `tools/worldgen_closure.py` writes `mods/horizon/src/java-mods-host/boot/res/worldgen_closure.tsv` (sorted, deduplicated), like `animate_closure.py`. Commit the TSV with the change.
3. **Java at world start**, in the `worldgen_export` op of SPEC-2: read the TSV and keep the rows whose method `Routing.patched` says is patched. Add to the export file:

   ```json
   "routed": {"<seam>": ["<method key>", ...]}
   ```

   with the seam keys `biomes`, `fill`, `surface`, `terrain`, `beard`, `spawn`, `carver:<type>`, `feature:<type>`, `start:<type>`, `place:<type>`.
4. **Rust,** in `WorldgenRoutes` (SPEC-4): typed seams become id seams.
   - `feature:<type>` routes every placed feature whose configured feature has that feature type.
   - `start:<type>` and `place:<type>` route every structure of that type.
   - `carver:<type>` routes every configured carver of that type.
   - The rest map one to one.
   - `why` gets the method keys.
5. **Granularity.** `terrain` routes the whole stage and makes `fill` and `surface` redundant. The log says so.

## Tests you run

- `WorldgenClosure` on the real jar. Spot-check these rows:
  - `StructurePiece.placeBlock` appears under `place:*` for every legacy type (mineshaft, fortress...) and not under `start:*`;
  - `JigsawPlacement$Placer.tryPlacingChildren` appears under `start:minecraft:jigsaw`;
  - `MultiNoiseBiomeSource.getNoiseBiome(Climate$TargetPoint)` appears under `biomes`.

  Add these as assertions in a `tests/test_worldgen_closure.py` that reads the committed TSV.
- A Rust unit test: a `routed` block turns into the right id seams, for a hand-made export with two structures of type `minecraft:jigsaw` and one of `minecraft:mineshaft`.

## Acceptance (the parent)

The W10 pack's world-open log lists routed seams matching DESIGN.md section 4's table:
- biomes (TerraBlender);
- terrain (lithostitched `buildTerrain`);
- beard;
- spawn;
- place for every structure type;
- start for jigsaw, ruined_portal, nether_fossil, shipwreck and woodland_mansion.

No mineshaft, stronghold, fortress, temple or monument starts are routed.

## Not in this spec

Running the seams.
