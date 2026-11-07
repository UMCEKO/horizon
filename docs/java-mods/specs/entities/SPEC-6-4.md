# SPEC-6-4: mod data items and complex spawn data on the wire

Design: `DESIGN.md` §2 table ("Tracking: native tracker; Java adds only the bytes of Java-owned data items and complex spawn data"), §8. Index: `SPEC-2-8.md`. Size: ~250 lines. Needs 6-2, 2-15.

## Mod data items
- A mod data item (6-1's list) is Java-owned: mod code sets it on the view (`entityData.set(DATA_X, v)`), Java's `SynchedEntityData` marks it dirty.
- **Dirty to native:** every crossing's reply effects block gains `int n` × `int id` of entities whose mod items became dirty during the call (Java tracks it in an `IntOpenHashSet` from a `SynchedEntityData.set` mixin on backed entities, mod accessors only). Native keeps the set per level.
- **Sending:** in `ServerEntity::send_changes` (`lan/entity.rs`), for an entity in the dirty set, the bin asks Java once per tracker tick for all dirty entities at once: op `mod_data` (`int n` × `int id`) → `int n` × (`int id`, `bytes` = the dirty `DataValue`s packed by the real `DataValue.STREAM_CODEC`, then cleared with `packDirty`). Native appends them to that entity's `SetEntityData` items (its own vanilla items first, ids ascending, as vanilla's `packDirty` order is by id).
- **Pairing:** `getNonDefaultValues` for mod items: at pairing (2-15's `add` and per-viewer pairing), op `mod_data_all` for the entity → the non-default mod items, appended to the pairing `SetEntityData`.
- On the native **client** (`remote.rs` `SetEntityData`), mod item ids are unknown to native: keep their raw bytes untouched and pass them through the 5-1 `DATA` feed; native ignores them (phase 8's renderers read them in Java).

## Complex spawn data
- `IEntityWithComplexSpawn` types (6-1 `complexSpawn`): NeoForge sends `AdvancedAddEntityPayload` (javap its record components) right after the add-entity packet in the pairing bundle. At pairing, op `spawn_data` (`int id`) → Java calls `writeSpawnData(buf)` on the view into a `RegistryFriendlyByteBuf` and returns the bytes; native appends the payload (as a custom payload packet, `CustomPayload` with NeoForge's channel id from `javap` of the payload's `TYPE`) to the bundle, after the add-entity packet.
- Native client: pass it into the 5-1 feed as an `ADD` tail; Java calls `readSpawnData` on the client view.

## Tests
- `lan/entity.rs` tests: `mod_items_follow_vanilla_items_in_id_order`.
- Test mod: `TestMob.DATA_GLOW` set server side in `later`; client check `mod_data_reaches_the_client_view` (the client view's `entityData.get(DATA_GLOW)` is true within 5 ticks). `TestTnt implements IEntityWithComplexSpawn` writing an int 42: client check `complex_spawn_data_read` (42 on the client view).
- `python3 tools/lan_e2e.py --check chest` (vanilla traffic unchanged).
