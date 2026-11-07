# W10: mod worldgen on Horizon (design)

Status: design, 2026-11-05. Base: `java-mods` at 9ade5eade. Binding: COVERAGE-SPEC rule 0 and principles 1 to 5, RULES.md, PLAN.md.

## 0. Summary

- **Today, none of the pack's worldgen reaches native.**
  - A new world (seed 1) with the showcase pack generates vanilla 26.3 terrain.
  - It has no Biomes O' Plenty biome, no Terralith biome or reshaped overworld, and no Dungeons and Taverns, Terralith, AE2 or Waystones structure.
  - Measured in section 1.
- **Data:** the final worldgen registries, as Java's real loader builds them, go to native at world start. This happens after NeoForge's biome and structure modifiers, TerraBlender's level-stem init and lithostitched's modifiers have run. Native layers them over its prepared bundles, as it already layers data packs (`WorldgenOverrides`).
  - Native never reads mod jars as packs. The pack proves why (section 1.4): conditions, overlays and library-specific files change what a jar's data means.
- **Code:** each generation step is split into *seams*: biome resolve, terrain fill, surface, each carver, each placed feature, each structure's start, each start's placement, beardifier input, original mobs.
  - A seam runs in Java only when it is detected at world start. Detection has two sources:
    - **product:** the registry entry it uses holds a type a mod owns;
    - **method:** W12's table says a loaded mod patches a vanilla method the seam reaches.
  - Everything else stays native.
  - Java seams run on the native worker thread that is generating the chunk, in parallel, without `GameBridge.LOCK`, as vanilla's worker threads do.
  - They run on a mirror of native's proto chunk (or 3x3 region) and send back the writes.
- **Acceptance:** a seed comparison against a real NeoForge server with the same jars (SPEC-1). It compares:
  - the biome grid;
  - every structure start in a square (id, chunk, bounding box, pieces);
  - section hashes of fully generated chunks.

## 1. Measurements (2026-11-05)

### 1.1 Setup and the four runs

Scratch: `/home/umceko/hz-poc/agents/w10/`.
- `pack/` holds the showcase jars (`play/pack3`) plus its three missing required dependencies. carryon is left out (1.2).
- `install/` was built with `scripts/install.py`. That is `java_mods.py install` with the entity analysis made non-fatal (1.2).
- `bin/` is the java-mods build from `play/bin-test` (05:29). `jars/` holds the host jars of 03:27.

| Run | What | Result |
|---|---|---|
| 1 | Horizon, fresh world | Boot refused: `carryon-neoforge-26.3-2.12.1.jar needs language provider javafml:26.3.0.37-beta or above` |
| 2 | Horizon, carryon removed | Boot failed: `java.lang.OutOfMemoryError: Java heap space` at the 640 MB cap (`horizon-java-host/src/lib.rs` `MEMORY`) |
| 3 | Horizon, `jvm -Xmx2g` in the scratch install's args | World opened. Queried with horizon-ctl (`run3/`) |
| 4 | Real NeoForge 26.3.0.101 dedicated server, same jars, seed 1, JFR `profile` | `forge/server.log`, `forge/forge.jfr` |

### 1.2 The pack itself

- `play/pack3` cannot boot on real NeoForge as it is. These required dependencies are missing:
  - `terrablender` (needed by biomesoplenty);
  - `prickle` (needed by attributefix and enchdesc);
  - `shogi` (needed by waystones).

  All three are in `.local/java-mods/library-jars/`.
- carryon 2.12.1's `loaderVersion = "[26.3.0.37-beta,)"` is refused by FML 12.0.12. Real NeoForge refuses it too.
- The task brief describes Biomes O' Plenty's placement as "TerraBlender-style regions via GlitchCore". In fact Biomes O' Plenty uses TerraBlender itself. The real server's log shows `Registered region biomesoplenty:overworld_primary to index 1`, and so on.
- `showcase-pack/replaces/replaces.json` was made without TerraBlender, so it lacks TerraBlender's mixins. Section 4 lists them from this install's analysis (`install/analysis/mixins/mixins.json`).
- `java_mods.py install` exits on this pack. Its entity analysis (`mod_replaces.install_analysis`) refuses 85 entity methods that have no state group in `mod_entity_facts.py` `GROUPS`.

### 1.3 What native generates (run 3, seed 1, player at 151 75 155)

| Query | Horizon | Real NeoForge (`execute positioned 151 75 155 run ...`) |
|---|---|---|
| `locate biome biomesoplenty:bayou` | `Can't find element ... worldgen/biome` | [2839, 75, -1509] |
| `locate biome terralith:alpine_highlands` | `Can't find element` | [183, 75, -1029] |
| `locate biome minecraft:plains` | [407, 75, 315] | [279, 75, 347] |
| `locate biome #minecraft:is_forest` | minecraft:forest [183, 75, 187] | biomesoplenty:redwood_forest [215, 75, 187] |
| `locate structure minecraft:village_plains` | [640, ~, 816] | [240, ~, 128] |
| `locate structure terralith:desert_outpost` | not found | [9936, ~, -8976] |
| `locate structure nova_structures:hamlet` | not found | not found (both) |
| `place feature biomesoplenty:fir_tree`, `waystones:waystone` | `Can't find element` | not run |

- Native reported `67 biomes`: the vanilla table only.
- Even vanilla results differ. Terralith reshapes the overworld and Biomes O' Plenty's regions take climate space, so plains and the village move.
- `content mods not loaded: biomesoplenty:blood: template minecraft:lava has no item` (`mod_content.rs:770`). This one carried fluid block aborts every content mod, so `44192 Java block states have no state in Horizon's registry`.
  - It is a carry bug, outside W10. But no mod block can be placed natively until it is fixed.

### 1.4 Why native must not read jars itself

The rest of the evidence for this rule is the code read below (`mod_content.rs`, `overrides.rs`), plus `datapack.rs` and `world_loading.rs`.
- Terralith's `data/minecraft/dimension/overworld.json` carries `neoforge:conditions: not mod_loaded lithostitched`. With lithostitched loaded, the file does not exist for the real loader.
- Terralith also ships `disable.*` overlays and lithostitched `worldgen_modifier`s. Waystones adds its village pieces only through lithostitched `worldgen_modifier`s.
- Mekanism, MekanismAdditions and balm change biomes through NeoForge `biome_modifier`s (applied by `ServerLifecycleHooks.runModifiers` at `handleServerAboutToStart`). MekanismAdditions also uses `structure_modifier`s.
- TerraBlender initializes level stems at server start. Horizon's Java server does this already (`Initialized TerraBlender biomes for level stem minecraft:overworld` at 05:21:13 in run 3).
- What does reach native today:
  - Java mods' content carry adds only six worldgen registries (`mod_content.rs` `WORLDGEN_BUNDLES`: structure, structure_set, template_pool, processor_list, feature, placed_feature), and only in a namespace a jar's mod id names.
  - Mod jars are not data packs for `WorldgenOverrides`.
  - `overrides.rs` `finish` drops new biomes on this branch. The data-pack biome table is on main (#4577 and #7528, merged after this branch's merge base).
  - Nothing reads biome or structure modifiers.

### 1.5 Type census of the pack's worldgen JSON (`type` fields outside `minecraft`)

| Mod | Files | With mod types |
|---|---|---|
| Terralith | 1,640 (biomes, densities, noise, material rules, features, structures, pools...) | 0 (its overworld stem uses only `neoforge:conditions`) |
| Dungeons and Taverns | 1,087 | 0 |
| Biomes O' Plenty | 721 | 152 of 305 configured features (trees, rocks...), 3 biomes |
| Mekanism (+Additions) | 63 | all: own feature types, NeoForge modifiers |
| Waystones | 30 | features, placements and lithostitched modifiers |
| AE2 | 3 | the meteorite structure type |
| lithostitched | 33 | 3 density functions, 2 modifiers |

Terralith and Dungeons and Taverns are pure vanilla-typed data. Once the data reaches native, they need no Java at generation, unless a method route (section 4) forces a seam.

### 1.6 Where real NeoForge spends worldgen time (JFR, run 4)

3,355 `Worker-Main` samples, classified by the outermost recognisable step (`scripts/jfr_stages2.py`). About 1,700 of them are the boot's resource reload and DFU.

| Step | Samples | With a mod frame on the stack | Mod code it was in |
|---|---|---|---|
| biomes | 780 | 69 | `terrablender.worldgen.noise.Area` (region layers) |
| terrain | 559 (+~200 noise with cut stacks) | 90 | `terrablender...NamespacedSurfaceRuleSource`, `MaterialRuleCompiler` |
| features | 119 | 11 | `mekanism...ResizableOreFeature`, `DisableableFeaturePlacement`, `biomesoplenty...BOPTreeFeature` |
| structure starts | 94 | 89 | `lithostitched...AlternateJigsawGenerator`/`AlternateJigsawStructure`, `appeng...MeteoriteStructure` |
| light, IO | 73 | 0 | |

Mod frames are on the stack in only about 15% of generation samples. The rest of the time is vanilla code: noise, density functions and climate, even inside mod-driven steps. This is why the seams are fine-grained. Java runs the mod's decision, and native supplies the vanilla inputs: climate targets, noise fill and vanilla features.

## 2. Data: how mod worldgen reaches native

### 2.1 Source of truth

Java's real `WorldLoader.load` already runs at boot (`GameBridge.prepare`). It runs with NeoForge's mod packs (`ResourcePackLoader.populatePackRepository`) and `WorldDataConfiguration.DEFAULT`. Its registries are the result of the real pack stack: conditions, overlays and lithostitched's loader hooks. Two more things happen at world start:
- NeoForge's biome and structure modifiers apply in `handleServerAboutToStart`;
- TerraBlender initializes its stems.

### 2.2 The export (SPEC-2)

At world start, right after `ServerAboutToStart`, Java writes one file.
- **Content:** every entry of the worldgen registries native reads (the `overrides.rs` `KINDS` list, plus `multi_noise_biome_source_parameter_list`, `world_preset`, `flat_level_generator_preset` and `configured_carver` under 26.3's names) that differs from vanilla's built-in entry. That covers entries added, replaced, or modified by a modifier.
- **Encoding:** each entry is encoded with its registry's codec through `RegistryOps` over `JsonOps`, as the JSON text `WorldgenOverrides` takes.
- **Biomes:** a biome's `features`, `carvers` and spawn settings come from the getters after the modifiers have run (`getGenerationSettings`, `getMobSettings`). They do not come from the codec. A check logs any entry where codec and getters disagree.
- **Level stems:** encoded from `server.registryAccess()`'s `LEVEL_STEM` after TerraBlender's init.
  - Noise settings' material rule comes from `NoiseGeneratorSettings.materialRule()`, the getter, because TerraBlender injects at its HEAD.
  - The noise router comes from the `RandomState` Java built through the real `ChunkMap` constructor (SPEC-6), because lithostitched wraps `RandomState.create` there.
- **Tags:** the worldgen tags native reads: biome tags; structure and feature tags if native gains them.
- **Owners:** per entry, the list of mod-owned types in its tree.
  - A type is mod-owned when `BootFacts.owns` says its type registry entry is a mod's. This covers `FEATURE`, `PLACEMENT_MODIFIER_TYPE`, `BLOCKSTATE_PROVIDER_TYPE`, the tree placer types, `BLOCK_PREDICATE_TYPE`, `DENSITY_FUNCTION_TYPE`, the material rule and condition types, `BIOME_SOURCE`, `CHUNK_GENERATOR`, `STRUCTURE_TYPE`, `STRUCTURE_PIECE`, `STRUCTURE_PROCESSOR`, `STRUCTURE_POOL_ELEMENT`, `POOL_ALIAS_BINDING`, `STRUCTURE_PLACEMENT`, `CARVER`, the value provider types and the rule test types.
  - A type is also mod-owned when the object's class is not the vanilla class its type names. That catches a subclass installed by a codec redirect, such as lithostitched's `MultiNoiseBiomeSource` codec wrap.

### 2.3 Native side (SPEC-3 and SPEC-4)

- The stack: `WorldgenOverrides` = export, then the world's own enabled data packs (as today). `GeneratorData::load_with` reads that stack over the prepared bundles.
- Today Java's server ignores a world's data packs (DEFAULT config). So a world pack's entry is native-only until Java reloads per world. That is decision D5.
- The biome table is built with main's `Biomes::with_packs` from the exported biomes. Those new biomes generate, colour, spawn, save and sync by name (main's #4577, #7528).
- Undecodable is not fatal. An entry native cannot decode (a mod type) keeps its id and its slot in every ordering:
  - feature indices in `FeatureSorter` order;
  - structure set and step order.

  It is marked Java for its seam.
- Cached under the Java content cache, keyed by the content fingerprint plus the world's pack list. A cache hit opens the world without waiting. Java then re-exports behind it and warns on any difference, as `java_content.rs` `verify` does for facts.

### 2.4 Region APIs

TerraBlender regions and lithostitched regions are code: they read noise of their own and choose per quart. They cannot become data without per-mod native code, which rule 4 bans. So they run in Java through the biome seam (3.3), with native supplying vanilla's climate targets. The pack shows this split: TerraBlender's own `Area` code is 69 of 780 biome samples, and the rest is vanilla climate noise.

## 3. Code: the Java generation step interface

### 3.1 Vanilla's steps (26.3, `ChunkStatusTasks`, `ChunkPyramid`) and the seams

| ChunkStatus | Vanilla call | Native today | W10 seams (each native unless detected) |
|---|---|---|---|
| STRUCTURE_STARTS | `ChunkGenerator.createStructures` | `generator_structures.rs` `starts_at` (lazy, cached) | **S-start(structure)**: one structure's `tryGenerateStructure` for one chunk |
| STRUCTURE_REFERENCES | `createReferences` (radius 8) | `references` | none: native computes them from the starts' boxes, whoever made the starts |
| BIOMES | `createBiomes` | `generator.rs` `biome_chunk` | **S-biome**: the chunk's biome palette from native climate targets |
| TERRAIN | `buildTerrain`: fill, material surface, carvers | `generate_with_context` (fill, surface, carvers in order) | **S-terrain** (the whole stage), or its parts: **S-fill**, **S-surface**, **S-carver(id)**; **S-beard** (Beardifier inputs) |
| FEATURES | `applyBiomeDecoration` on a 3x3 `WorldGenRegion` | `generator_decoration.rs` `decorate` | **S-feature(placed id)**, **S-place(start)** (a start's `placeInChunk`) |
| SPAWN | `spawnOriginalMobs` | `game/natural_spawning.rs:604` | **S-spawn** |

LIGHT stays native: no mod in the pack patches it, and no mod type feeds it.

### 3.2 Routing decision (SPEC-4 product, SPEC-5 method), made once per world at start, never per chunk

- **Product.** A seam is Java when its registry entry's export lists a mod-owned type. Each seam reads one kind of entry:
  - S-feature: the placed feature (placement and configured feature);
  - S-start, S-place: the structure and its pieces' pool elements and processors;
  - S-biome: the level stem's biome source;
  - S-fill: the router;
  - S-surface: the material rule;
  - S-carver: the carver.
- **Method.** A seam is Java when W12's routing table marks as patched a vanilla method that the seam can reach.
  - "Can reach" is computed statically from the vanilla jar's call graph, per seam root and per polymorphic type. This is the same approach as `tools/animate_closure.py` for `animateTick`.
  - New tool: `tools/worldgen_closure.py` writes `J/boot/res/worldgen_closure.tsv`. Its rows are seam, type id and reached method.
  - At world start Java joins the table with W12's patched set and returns the routed seams.
  - No mod names appear anywhere in this, only methods and registry ownership.
- **Granularity rule.** The smallest seam that contains every call of the patched method wins. For example:
  - `StructurePiece.placeBlock` is called only under `placeInChunk`, so it routes S-place for the starts whose pieces reach it.
  - A HEAD inject on `NoiseBasedChunkGenerator.buildTerrain` routes S-terrain as a whole.
- **No polling, no fallback.** A routed seam always runs in Java for that world. An unrouted one never calls Java.

### 3.3 Interfaces (all ops `Crossing.free`, so no `LOCK`; on the native worker thread that is generating)

| Op | In | Java runs | Out |
|---|---|---|---|
| `wg_biomes` | level, chunk, the chunk's climate targets (one per quart, vanilla order: x, z, y inner) | `ChunkGenerator.createBiomes` on a mirror `ProtoChunk`, with a `Climate.Sampler` that answers native's targets. It falls back to Java's own sampler off the batch, and counts those fallbacks | biome name per quart (palette per section) |
| `wg_biomes_at` | level, list of quarts | the same resolver, for `/locate biome`, structure biome checks, LOD | names |
| `wg_start` | level, chunk, structure id, the set's placement args (seed, salt) | `ChunkGenerator.tryGenerateStructure` for that structure; the start stays in Java's per-level start cache | `StructureStart` NBT (`createTag`) or none |
| `wg_terrain` | level, chunk, mirror (biomes, heights, starts referenced for the beardifier) | `NoiseBasedChunkGenerator.buildTerrain` | sections, heightmaps, carving state |
| `wg_surface`, `wg_carver` | mirror after the previous native part | `NoiseBasedChunkGenerator.buildSurface` (`MaterialSystem.buildSurface`) with the real rule, or one configured carver's `carve` in vanilla order | changed sections |
| `wg_beard` | level, chunk | `Beardifier.forStructuresInChunk` over the starts' Java objects | rigid boxes and adaptations, junctions |
| `wg_decorate` | level, center chunk, a run of consecutive decoration items (step, index, placed id, or start ids for structure placement), the region mirror (first run) or native's writes since the last run | for each item: `setFeatureSeed` then `placeWithBiomeCheck`, or `placeInChunk`, on a real `WorldGenRegion` over mirror `ProtoChunk`s | writes: block changes in order, block entity NBT, entity NBT, scheduled ticks, post-processing marks |
| `wg_spawn` | level, chunk, mirror | `spawnOriginalMobs` on a `WorldGenRegion` | entity NBT |

**Mirrors (SPEC-7).**
- Native's `ProtoChunk` crosses as its sections' palettes plus packed data, biomes, heightmaps, block entities, entities, ticks, post-processing and status.
- Java builds real `ProtoChunk`s, so heightmap updates, write-zone checks and `markPosForPostprocessing` are vanilla's own.
- One region mirror lives for one chunk's decoration. Native and Java alternate runs over it, each sending only its writes since the last run. Consecutive Java items in a step go in one call, which keeps the crossings per chunk at the number of native/Java switches, not the number of features.
- Block states cross as Java state ids through the existing `StateMap`; biomes cross by name.

**Java's generation context (SPEC-6).** Per level at world start:
- the level stem's real `ChunkGenerator`;
- a real `ChunkMap` built through its constructor, so `ChunkMap.<init>` mixins run. It gets scratch storage that is never saved: native saves;
- its `RandomState` and `ChunkGeneratorStructureState`;
- `StructureManager` with `StructureCheck`;
- the server's `StructureTemplateManager`.

`level.getChunkSource().getGenerator()` returns that generator, so mods that search biomes or structures (Nature's Compass) see the real one.

### 3.4 Threads

- Vanilla generates on `Worker-Main` threads in parallel, and mods' worldgen code is written for that. Horizon's worker pool (`streaming.rs`, 23 threads here) calls the `wg_*` ops directly.
- Each worker attaches once and owns its crossing arena (`H/lib.rs` `Arena`).
- No lock is taken. These ops touch only the mirror, the level's generator objects and Java's start cache. The start cache is a concurrent map, sharded like native's `generator_structures.rs` cache.
- The server thread never waits on them, except where vanilla's main thread would wait on generation too: a chunk the player needs.

### 3.5 Saves and starts

- Java-made starts are saved by native as their NBT, untouched. Native must keep unknown structure ids in `structures.starts` and `references`.
- When a chunk loads, Java rebuilds a start from its NBT (`StructureStart.loadStaticStart`) only when a Java seam asks for it: S-place, S-beard or locate.
- A native start that a Java seam needs crosses as native's NBT. Its parity is already harnessed (`test_structure_start_nbt_parity.py`).

## 4. lithostitched (and TerraBlender, Waystones) through W12

lithostitched has 39 mixin handlers on worldgen methods that `replaces.json` routes `java`. Waystones has 2. TerraBlender's come from this install's analysis. Grouped by the seam each one forces under the granularity rule:

| Patched methods (mod) | Seam routed | Effect for this pack |
|---|---|---|
| `ChunkMap.<init>` wrap of `RandomState.create` (litho) | none directly: Java builds the real `RandomState` (SPEC-6) and exports its router; product rule decides S-fill | S-fill stays native unless the router gains a mod density function |
| `NoiseBasedChunkGenerator.buildTerrain` HEAD inject `fillBands` (litho) | **S-terrain** whole | All terrain in Java (decision D4) |
| `Beardifier.forStructuresInChunk` wrap (litho) | S-beard | One small call per chunk with structures nearby |
| `NoiseBasedChunkGenerator.spawnOriginalMobs` wrap (litho) | S-spawn | Java per chunk at SPAWN |
| `StructureStart.placeInChunk`, `StructurePiece.placeBlock/fillColumnDown/createChest`, Mineshaft, RuinedPortalPiece, NetherFossilPiece, MonsterThrone, PortalRoom, DesertPyramid, WoodlandMansion `afterPlace`, the `makeSettings` returns (litho); `SinglePoolElement.place` (waystones) | S-place for every start | All structure placement in Java. Native starts cross as NBT |
| `JigsawStructure.findGenerationPoint`, `PoolAliasLookup.create` (litho), `JigsawPlacement$Placer.tryPlacingChildren` (waystones), `RuinedPortalStructure`/`NetherFossilStructure.findGenerationPoint`, `ShipwreckStructure`/`WoodlandMansionStructure.generatePieces`, mansion room collections (litho) | S-start for those structure types | Jigsaw (villages, D&T, Terralith), ruined portals, fossils, shipwrecks and mansions start in Java. Strongholds, mineshafts, temples, monuments and fortresses stay native |
| `MultiNoiseBiomeSource`, `MultiNoiseBiomeSourceParameterList`, `Biome`, `BlockPredicate` `<clinit>` codec redirects (litho) | none: decoding, caught by the export's product rule | |
| `VillagerType.byBiome` (litho) | not worldgen: entity spawning (W9/W12) | |
| TerraBlender: `MultiNoiseBiomeSource` resolver lambdas wrap `getNoiseBiome(TargetPoint)`; `ChunkGenerator.doCreateBiomes`; `TheEndBiomeSource.getNoiseBiome` | **S-biome** (overworld, nether, end) | Biomes in Java on native climate |
| TerraBlender: `NoiseGeneratorSettings.materialRule` HEAD | none: the getter's product (`NamespacedSurfaceRuleSource`) routes **S-surface** | Surface in Java |

**What W10 needs from W12:**
- a boot fact naming each vanilla method a loaded mod patches, in the `replaces.json` key form (`net/minecraft/...#name`);
- `Host::patched(method)` in Rust and `Routing.patched(method)` in Java.

W10 does not need W12's per-call delegation: its seams are the delegation points. Until W12 ships the fact, only product routing is active. That is the final mechanism with one of its inputs empty, not a transitional mode.

## 5. Cost model

- **Native-only world** (no mod worldgen detected): no change. No `wg_*` call is made, and the routing table is empty.
- **Per routed seam, Java CPU** equals what NeoForge spends on that seam: it is the same code. Horizon adds only:
  - crossing: about 0.1 µs per request through the arena (`java_mods.py crossing`), plus about 2 to 5 µs per JNI attach-free call with a few KB;
  - mirror encode and decode: one chunk's sections are about 20 to 60 KB, and a 3x3 region about 150 to 450 KB, so about 0.05 to 0.4 ms per chunk at about 1 GB/s each way;
  - `wg_biomes` input: 1,536 targets x 48 B (74 KB) per overworld chunk, and 6 KB out.
- **This pack, per new overworld chunk**, if D4 accepts whole-stage terrain routing:
  - S-terrain is Java (lithostitched HEAD inject): vanilla Java terrain, about 5 to 10 ms of one worker. Native's own is about 4.8 ms noise plus 3.2 ms surface on a 5600X (`docs/noise-aquifer-benchmark.md`, `docs/surface-generation-benchmark.md`).
  - S-biome is Java on native targets: TerraBlender's Area and the parameter tree only, under 1 ms.
  - S-place for every start, and S-start for jigsaw and other patched types: amortized under 1 ms.
  - S-feature for Biomes O' Plenty's trees and Mekanism's ores: Java only for those items, other features native. 2 to 10 runs per chunk.
  - The total is about NeoForge's speed for worldgen, spread over Horizon's worker pool, with vanilla features, carvers, light and saving native.
  - Without lithostitched's `buildTerrain` inject, S-fill stays native, which is the bulk of the 1.6 table's terrain time.
- **Memory:** mirrors are per worker and per call (≤ 1 MB). Java's start cache is bounded by the chunks native has loaded and is evicted on `java_chunks_leaving`. The boot heap for this pack needs more than 640 MB (decision D2).
- **Measure each SPEC:** `/pregen` chunks per second (`docs/pregeneration.md`) on the pack against vanilla and against NeoForge with Chunky (`docs/modpacks.md` row "World generation"), plus `HORIZON_PROFILE` Java time per `wg_*` op.

## 6. Determinism and acceptance

- Every seam takes only seed-derived inputs, as vanilla's do:
  - `WorldgenRandom.setDecorationSeed` and `setFeatureSeed` per item;
  - structure placement salts;
  - climate per quart.

  So native and Java parts can interleave and run in parallel without changing results.
- The only order effect is vanilla's own: features writing into a neighbour depend on generation order. Comparisons therefore use the center chunks of a fully generated square.
- **The harness (SPEC-1),** `tools/worldgen_compare.py --install DIR --seed S --out OUT`:
  - **Forge side:** a real NeoForge 26.3.0.101 dedicated server with the install's jars and a small dump mod, `mods/horizon/src/java-mods-host/parity/worldgen/`. It runs:
    - the biome grid through `createBiomes` on fresh `ProtoChunk`s (the path chunks take, so TerraBlender's resolver wrap runs) over a square, at three Y;
    - `createStructures` per chunk of a square, with no world writes;
    - forceload of a small square, then section hashes and heightmaps of its center chunks.
  - **Horizon side:** `horizon-ctl worldgen biomes|starts|chunks` answered by the server thread from the production generator.
  - **Output:** `OUT/diff.json` with the biome mismatch rate, missing or extra or different starts, and chunk hash mismatches. The acceptance numbers are:
    - biomes 100%;
    - starts 100% (id, chunk, box, piece types and boxes);
    - center-chunk hashes 100% on the pack's seed 1, and on a second seed.
- **Baseline now** (section 1.3): the harness fails everywhere. Each SPEC states which part must turn green.

## 7. Prerequisites and decisions for the user

- **P1 / D1.** Merge main into java-mods, or bring #4577 and #7528 over. They give native a world-specific biome table: generation, colours, spawns, saves, network registry and `/locate biome`. W10 builds on them.
- **D2. The Java heap cap.** 640 MB OOMs at this pack's boot; 2 GB worked. Pick a policy: a fixed larger cap, a cap scaled by registry size, or a player setting.
- **D3. The showcase pack.**
  - Add TerraBlender, PrickleMC and shogi.
  - Drop carryon 2.12.1, or replace it with a build that FML 12.0.12 accepts.
  - Regenerate `replaces.json` with TerraBlender in the pack.
- **D4. HEAD injects on a stage method.** lithostitched's `buildTerrain` HEAD inject routes all terrain to Java under rule 0, which makes terrain run at NeoForge's speed for any pack with lithostitched (a very common library). Rule 0 leaves no middle ground: the inject may change the result, and only running it shows whether it does. Confirm that this is accepted.
- **D5. World data packs and Java.** Java's server loads `WorldDataConfiguration.DEFAULT` and never a world's own packs. W10 layers world packs natively over the export. Making Java reload per world (vanilla does) is server lifecycle work. Should W10 own it?
- **P2. Content carry.** `biomesoplenty:blood` (a fluid block declared with an item whose template has none) aborts all content mods. This must be fixed before any Biomes O' Plenty block can generate natively.
- **P3. The install tool.** The entity analysis refuses this pack (85 methods without a state group). That belongs to the entities workstream.
- **Note.** Run 4 started a real NeoForge dedicated server with `eula=true` in my scratch folder, as `neoforge_oracle.py` does.

## 8. Step list

| SPEC | Title | Depends on | Turns green |
|---|---|---|---|
| 1 | `worldgen_compare.py`: Forge dump mod, `horizon-ctl worldgen`, diff | none | the harness runs (red) |
| 2 | Java registry export at world start | none | export file complete |
| 3 | Lifecycle: `ServerAboutToStart` before native builds levels; export cache | 2 | export before generation |
| 4 | Native reads the export; undecodable entries keep their slots; routing table (product half) | 3, P1 | biomes and starts on a Terralith + D&T pack |
| 5 | `worldgen_closure.py` and the method half of the routing table (W12's patched set) | 4, W12 boot fact | section 4's rows routed |
| 6 | Java generation context per level (real `ChunkMap` constructor, generator, structure state) | 3 | Java-side locate equals Forge |
| 7 | Chunk and region mirror wire format, both sides, round-trip tests | none | none (unit) |
| 8 | S-biome: `wg_biomes` with native climate targets; `wg_biomes_at` for locate and structure checks | 5, 6, 7 | biome grid 100% on the full pack |
| 9 | S-start and Java's start cache; native keeps opaque starts; `/locate structure` through Java for routed structures | 5, 6 | starts 100% |
| 10 | TERRAIN seams: S-terrain, S-surface, S-carver, S-beard | 7, 8, 9 | section hashes before features |
| 11 | FEATURES session: S-feature runs and S-place | 7, 9 | center-chunk hashes 100% |
| 12 | S-spawn; structure modifiers' spawns from the export | 11 | entities of center chunks |

W12's boot fact is the only outside input. Without it, SPEC-5 routes nothing and the pack's TerraBlender, lithostitched and Waystones seams stay native, which is wrong. So W12's fact is on the critical path from SPEC-8 on.

Deferred until a pack needs them, with the evidence written down:
- mod dimensions as native levels: AE2's spatial storage creates its level in code;
- far terrain (LOD) through `wg_biomes_at`.

## Decisions (2026-11-05)
- **Heap:** the user rejected raising the cap: "diagnose why its using that much". A memory investigation is in /home/umceko/hz-poc/agents/mem/.
- **Data packs per world:** Java's server reloads registries per world, so each world's own data packs apply. Part of W10.
- **lithostitched `buildTerrain` HEAD inject:** must follow W12's edge semantics (only the handler runs in Java; terrain stays native) unless proven to need the body. Revisit SPEC-10 accordingly.
- **The main merge into java-mods:** to be done (it brings #4577 and #7528).
- **Showcase pack:** add TerraBlender, PrickleMC and shogi; drop or replace carryon 2.12.1; regenerate replaces.json.
- **Blockers to fix:** `biomesoplenty:blood` aborting the content carry (mod_content.rs:770); `java_mods.py install` refusing on entity methods without a state group.
