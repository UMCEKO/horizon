# SPEC-1: `tools/worldgen_compare.py`, the W10 seed comparison against real NeoForge

Size: ~400 lines (Python ~180, Java ~150, Rust ~80). Depends on nothing. Read DESIGN.md sections 1 and 6 first.

## Goal

One command compares Horizon's world generation with a real NeoForge 26.3.0.101 server running the same jars and seed:

```
python3 tools/worldgen_compare.py --install DIR --seed S --out OUT [--square 2048] [--starts 64] [--chunks 5] [--skip-forge] [--skip-horizon]
```

It writes `OUT/forge.json`, `OUT/horizon.json` and `OUT/diff.json`, and exits non-zero on any difference. Today it must run and report differences: the baseline is red.

## Read first

- `tools/neoforge_oracle.py`: `install()`, `instance()`, `start()`. This spec's server runs the same way: `java @unix_args.txt nogui`.
- `tools/mod_parity.py` `build_parity_jars`. Build the new dump jar the same way.
- `tools/hz_scenario.py`: Horizon runs, gate, `--keep`.
- `crates/horizon-client/src/game/control.rs`: the `"mobs"` arm, which shows the `self.integrated` → `server.call(|g| ...)` pattern.
- `crates/horizon-client/src/vanillagen.rs`: `noise_biomes`, `starts_at`, `column`.
- Vanilla: `ChunkStatusTasks.generateBiomes`, `ChunkGenerator.createBiomes`, `ChunkGenerator.createStructures`.

## Do

### 1. The Forge dump mod

Create `mods/horizon/src/java-mods-host/parity/worldgen/dev/horizonlab/javamods/parity/WorldgenDump.java`. It is a NeoForge mod (with a `neoforge.mods.toml` under `res/`, mod id `horizon_wgdump`). On `ServerStartedEvent` it reads `-Dhorizon.wg.request=FILE` (JSON) and writes `-Dhorizon.wg.out=FILE`, then calls `server.halt(false)`.

For the overworld (and every level in `request.levels`, default overworld only), it writes four things:

- **`biomes`:** for chunk x, z in the square centred on 0,0 at the request's step (default every 4th chunk), it makes a fresh `ProtoChunk` and calls `generator.createBiomes(randomState, Blender.empty(), structureManager, chunk).join()`. That is vanilla's path, so mixins on the resolver run. It then reads `chunk.getNoiseBiome(qx, qy, qz)` at the chunk's first quart column for qy at block Y -32, 64 and 160. Output `[[x, y, z, "ns:id"], ...]`.
- **`starts`:** for each chunk of the `starts` square (centre 0,0), it calls `generator.createStructures(registryAccess, generatorState, structureManager, protoChunk, templateManager, dimension)` on a fresh `ProtoChunk` with no writes to the world. It then lists `protoChunk.getAllStarts()` as `{"structure", "chunk", "box", "pieces": [[type, box], ...]}`, sorted.
- **`chunks`:** `/forceload add` the `chunks` x `chunks` square at 0,0 through `server.getCommands()`. It ticks until every one is `FULL` (`level.getChunk(x, z, ChunkStatus.FULL, true)` on the server thread), then hashes the center chunk and its 8 neighbours. The hash is defined under "Formats" below.
- **`seed`, `levels`:** the seed, and the level stem ids with their generator and biome source class names (for reading only).

Build the jar in `tools/worldgen_compare.py`, as `build_parity_jars` builds its jars (javac against `oracle.client_classpath`).

### 2. The Horizon side

Add a `"worldgen"` arm to `control.rs`: `worldgen biomes|starts|chunks <json>`. It takes the same request JSON. Through `self.integrated` and `server.call`, it answers from the overworld's production generator: the `VanillaTerrain` of the level the server holds. Then:

- **`biomes`:** `noise_biomes(quarts)` at the same quarts.

  When SPEC-8 lands, this goes through the same entry, which then routes to Java. The harness never calls a Java op itself.
- **`starts`:** `starts_at(x, z)` for each chunk. Each start becomes the same shape as the Forge side: structure id, chunk, bounding box, and its pieces' NBT `id` and `BB`.
- **`chunks`:** run `/forceload add` through the server's command path, wait until the chunks are full (poll the server's chunk map from the ctl handler, not from game code), then hash the same 9 chunks.

Also add a `worldgen` entry to `HELP`, and to `docs/horizon-ctl-commands.md`.

### 3. `tools/worldgen_compare.py`

1. Builds the dump jar.
2. Makes a server folder under `OUT/forge/` with the install's `mods/*.jar` and the dump jar linked, plus `eula.txt`. Its `server.properties` has `level-seed=S`, `online-mode=false` and `max-tick-time=-1`.
3. Runs the server through the gate (`HORIZON_GAME_GATE`, `GATE_OWNER=worldgen_compare`) with `-Xmx4G`. Kill it on every exit path (`try/finally`, `os.killpg`).
4. Runs Horizon through `hz_scenario.py run --install DIR --fresh --world W --keep` with a script that waits 100 ticks, then calls `horizon-ctl worldgen ...` three times, then `quit`.

   The Horizon world's seed must be S. If `hz_scenario` has no seed option, add `--seed S` to it: it passes `--seed` to `launch` instead of the fixed `1`.
5. Diffs. Rates and the first 20 differences of each part go in `diff.json`:
   - biomes per point;
   - starts as sets keyed by (structure, chunk), then comparing box and pieces;
   - chunks per section hash and per heightmap.

## Formats

Both sides compute the chunk hash the same way:
- per section, from the level's min Y: SHA-1 of the 4,096 block states in index order `y<<8 | z<<4 | x`. Each state is written `ns:id[k=v,...]`, with properties sorted by name and a `\n` after each state;
- per chunk: the heightmaps `WORLD_SURFACE`, `OCEAN_FLOOR`, `MOTION_BLOCKING` and `MOTION_BLOCKING_NO_LEAVES` as 256 ints each;
- the block entities' sorted list of (x, y, z, id).

## Tests you run

- `python3 -m unittest tests.test_worldgen_compare`: a new test of the diff function on two small hand-made JSONs.
- `cargo test -p horizon-client --bins control`, if a unit test fits the arm's argument parsing.
- One real run on vanilla jars only: an install with no worldgen mods, for example `play/install-rs`. It is the harness's own check.
  - A biome or start difference there is a bug in the harness or in native parity. Report it with the first differences; don't fix native here.
  - Center-chunk hashes may differ where native's generation order differs from vanilla's. Report which sections differ.

## Acceptance (the parent)

- On a vanilla-mods install: `diff.json` shows 0 differences.
- On the W10 pack install (`/home/umceko/hz-poc/agents/w10/install`, heap raised): it runs to the end and reports large differences, for example missing `biomesoplenty:*` biomes. That red result is the baseline.

## Not in this spec

Any routing, export or generation change.
