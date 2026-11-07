# SPEC-7: Chunk and region mirrors, the wire format both sides share

Size: ~450 lines (Rust ~200, Java ~200, tests). Depends on nothing; use the block state table (`bridge::STATES`). Read DESIGN.md section 3.3 ("Mirrors").

## Goal

Native's proto chunk can be sent to Java as a real `ProtoChunk`, and Java's changes can come back. Two things cross:
- a whole chunk (stage seams: SPEC-10);
- a region session (3x3, decoration: SPEC-11). It is opened once, then each side sends only its writes since the last turn.

This spec is just the format and its two codecs, with round-trip tests. No seam uses it yet.

## Read first

- Rust: `crates/horizon-world/src/worldgen/generator.rs` `ProtoChunk` (`terrain`, `block_entities`, `entities`, `biomes()`, `structures()`), `terrain.rs` `TerrainChunk` (sections, heightmaps, `set_block_raw`, ticks, `generation_stage`).
- `horizon_core::paletted::PalettedContainer`. `crates/horizon-client/src/vanillagen.rs` `column`, `saved_proto`, `restore_proto` show how native moves blocks, block entities and ticks.
- Java: `ProtoChunk`, `LevelChunkSection`, `PalettedContainer` (its `Strategy` and `read`/`write` on `FriendlyByteBuf`, the network form), `ProtoChunkTicks`, `ChunkAccess.markPosForPostprocessing`.
- `H/wire.rs` `Out`/`In`; `B/Flat.java`.

## Format (big-endian, `H/wire.rs`)

**Chunk** = `int x`, `int z`, `int minY`, `int sections`, then:

- per section: `bool present`. If present: the block states in vanilla's network `PalettedContainer` form, with Java block state ids as values (native maps through `StateMap::to_java`), then the biomes in the same form with ids from the export's biome order (SPEC-4's table; both sides index the same name list, which the session header sends once).
- `int n` heightmaps x (`byte type`, `long[37]` packed as vanilla's `Heightmap` raw data).
- `int n` block entities x (`long pos`, `bytes tag`).
- `int n` entities x `bytes tag`.
- `int n` block ticks x (`long pos`, `int javaBlockId`, `int delay`, `byte priority`); the same for fluid ticks.
- `int n` post-processing marks x (`short packedPos`, `byte sectionIndex`).
- `byte status` (vanilla's `ChunkStatus` index).
- `int n` structure starts x (`text id`, `bytes tag`) and references x (`text id`, `int n` x `long chunk`).

**Session header** (once per session): `int format`, `text level`, `int n` biome names (the index order for biome values), `long seed`.

**Write log** (both directions in a region session): `int n` x entry, in the order the writes happened:
- `byte 0` block: `long pos`, `int javaState`, `int flags`;
- `byte 1` block entity set: `long pos`, `bytes tag`;
- `byte 2` block entity removed: `long pos`;
- `byte 3` entity added: `bytes tag`;
- `byte 4` block tick: `long pos`, `int javaBlockId`, `int delay`, `byte priority`;
- `byte 5` fluid tick: the same, with a fluid id;
- `byte 6` post-process mark: `long pos`.

Heightmaps are not sent in the log: each side's real `ProtoChunk.setBlockState` updates its own, and native recomputes from blocks as `restore_proto` does.

## Do

1. **Rust:** `crates/horizon-world/src/worldgen/mirror.rs` with:
   - `encode_chunk(&ProtoChunk, &StateMapping, &BiomeNames) -> Vec<u8>` and `decode_chunk_into(&mut ProtoChunk, bytes)`. Decoding replaces sections, block entities, entities, ticks and marks; it doesn't touch structures unless sent.
   - `WriteLog` (record, encode, apply to a `[ProtoChunk; 9]` region by position).

   `StateMapping` is a trait with `to_java(u32) -> u32` and `to_native(u32) -> u32`, so `horizon-world` doesn't depend on the host. The game passes `bridge::STATES`.
2. **Java:** `B/ChunkMirror.java`:
   - `ProtoChunk read(ByteBuffer, LevelHeightAccessor, Registry<Biome>, PalettedContainerFactory)` builds a real `ProtoChunk` with real sections, so `setBlockState` updates heightmaps and light sources as vanilla does.
   - In a region session, Java records every write at the chunk, not at the region, because mod features also write through `level.getChunk(pos)`.
     - The mirror is `MirrorProtoChunk extends ProtoChunk`. It overrides `setBlockState`, `setBlockEntity`, `removeBlockEntity`, `addEntity` and `markPosForPostprocessing`, appending to the session's `WriteLog` before calling `super`.
     - Its tick containers record `schedule`.
     - A block whose state doesn't change is not logged (`setBlockState` returns the old state; compare).
   - `ChunkMirror.writeChunk(ProtoChunk)` for whole-chunk replies.
3. **Recording native's writes:** native's decoration `World` (`generator_decoration.rs`, `feature_region.rs`) must append to a `WriteLog` while a session is open. Add an `Option<&mut WriteLog>` to the region writer, set by SPEC-11. In this spec, add the recording, off by default, plus its test.

## Tests you run

- **Rust:** a round trip of a generated vanilla proto chunk. `cargo test -p horizon-world mirror` encodes and decodes, and the blocks, biomes, block entities and ticks must be equal. A `WriteLog` applied to a 3x3 region sets the right chunk's blocks.
- **Java:** a `Checks` case in the test mod. It reads a chunk the Rust test wrote to a file (fixture under `mods/horizon/src/java-mods-host/test/res/`), sets two blocks (one through a `WorldGenRegion`, one through `getChunk(pos).setBlockState`), and asserts the write log bytes.
- `java_mods.py check`.

## Acceptance (the parent)

The unit tests and the Java check pass. There is no in-game effect yet.

## Not in this spec

Calling Java with a mirror.
