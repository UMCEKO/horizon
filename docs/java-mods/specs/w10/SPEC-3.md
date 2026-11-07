# SPEC-3: `ServerAboutToStart` before native builds its levels; the export runs then and is cached

Size: ~250 lines (Rust ~200, Java ~50). Depends on SPEC-2. Read DESIGN.md sections 2.2 and 2.3.

## Goal

The world-start order follows vanilla's (`MinecraftServer.runServer`): `ServerAboutToStart`, then the levels are created, then `ServerStarting`, `LevelEvent.Load` and `ServerStarted`. Today Horizon fires all of them after its levels exist (`WorldOpenReady`).

The worldgen export (SPEC-2) runs between `ServerAboutToStart` and native's generator creation. Native's generators are then built from it. A cache keeps a second open of the same world and pack set from waiting on Java.

## Read first

- `B/GameBridge.java` `lifecycle`, which fires everything at once.
- `G/java_hooks.rs` `java_hook_world`, `G/native_hooks.rs` `native_world_lifecycle`, `G/native_lifecycle.rs` (where `WorldOpenReady` fires).
- `G/world_loading.rs` `make_streaming` (line ~463, `VanillaTerrain::for_world_with(..., &overrides, ...)`), `world_worldgen_overrides`, `level_shapes`. `G/levels.rs` (line ~144) builds a level's streaming.
- `G/java_content.rs`: `fingerprint_of`, the cache folder and `verify`. Copy that pattern.

## Do

1. **Java.** Split `lifecycle(input, true)` into two ops:
   - `server_about_to_start`: sets `worldFolder`, makes the three `level(id)` views, runs `openWorld`, fires `ServerAboutToStart`.
   - `server_start` (the rest): `ServerStarting`, `customSpawners`, `LevelEvent.Load`, `ServerStarted`.

   `serverStarted` becomes true after the first op. `server_start` without the first one is an error that is logged and ignored, never a silent fallback.
2. **Rust.** Add a hook `java_hook_world_opening(world_root, seed) -> Option<WorldgenExport>`, a `cfg(java-mods)` / `cfg(not)` pair in `java_hooks.rs`.
   - It is called in the server thread's world open, before the first `make_streaming` of any level. Find the earliest point in `levels.rs` and `integrated.rs` where the world root and seed are known.
   - It calls `server_about_to_start`, then gets the export (step 3) and returns it parsed (SPEC-2's `from_java_export`).
   - `WorldOpenReady` keeps calling `java_world_phase`, which now sends `server_start` only.
3. **Cache.** The key is the content fingerprint from `java_content.rs` plus the sorted ids of the world's enabled data packs. The file is `.horizon-cache/java-mods/<fingerprint>/worldgen-<pack-key>.json`.
   - On a hit, use the cached file at once, and let the Java side export to `<file>.next` on the `Java mods' facts` thread after `server_start`.
   - If `.next` differs, log loudly (`Horizon: Java mods' worldgen export changed; the next world open uses the new one`) and replace the file for the next open. This is `verify`'s rule for facts: never mid-world.
   - On a miss, export synchronously and wait. The first open of a pack waits, about 0.1 to 0.5 s; log the time.
4. **Plumbing.** Thread the parsed export to `world_worldgen_overrides`, which stacks it under the world's own packs (SPEC-4 uses it). Keep it in `Game` next to where `overrides` are built, as an `Arc`.

## Tests you run

- Rust unit tests: the cache key changes with the pack list; a hit reads the file and a miss calls the op. Use a fake closure for the op.
- `java_mods.py check`, `bridges`.
- In the test mod, `Checks` asserts the order: `ServerAboutToStart` is seen before `LevelEvent.Load` and before the first `worldgen_export`. A counter in the test mod's listeners is enough.

## Acceptance (the parent)

1. The W10 pack world opens.
2. The log shows `worldgen export: N entries, M with mod types, T ms`, before `worldgen data loaded`.
3. A second open shows the cache hit with no wait.
4. `LevelEvent.Load` and `ServerStarted` still fire, after the export.
5. An M1 boot comparison of the lifecycle events keeps vanilla's order (`bootcmp` agg: `ServerAboutToStartEvent` before `LevelEvent$Load`).

## Not in this spec

Native reading the export (SPEC-4).
