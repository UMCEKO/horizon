# SPEC-5-1: the Java client's entities, fed by native's packets

Design: `DESIGN.md` §6 (variant (a)), decision 3, COVERAGE-SPEC W0/W2. Index: `SPEC-2-8.md`. Size: ~300 lines. Needs phase 1c/1d (server views), the riding client table (1c2c-3).

## Why
The Java client's `ClientLevel` has no entities. Mods' client code that asks it (`level.getEntity(id)`, `getEntities(box)`, `entitiesForRendering()`, `minecraft.crosshairPickEntity`) finds nothing: Jade can't show a mob, a mod's payload naming an entity id finds none. Vanilla's client fills its level from entity packets. Here the Java server sends no entity packets (native's hub does), so native feeds the Java client what its own client applied, in arrival order. Java never ticks, interpolates or renders these entities.

## 1. The feed (Rust, `G/remote.rs` + `G/java_screens.rs`)
- After `remote.rs` applies each of these packets (`handle_packet` arms, lines ~2429–2999), it appends a feed entry to the next client tick's batch (`java_client_tick_begin` collects; a new `Input` kind 16 `Entities`), **in arrival order**:
  - `ADD` (`AddEntity`): `int id`, `uuid`, `text type`, 3 `double` pos, 3 `byte`-angle-decoded `float` (pitch, yaw, head yaw), 3 `double` motion, `int data`;
  - `DATA` (`SetEntityData`): `int id`, the packet's raw items bytes as received (vanilla `DataValue` stream form);
  - `EQUIP` (`SetEquipment`): `int id`, `int n` × (`byte slot`, `bytes stack` as on the wire);
  - `PASSENGERS`: `int vehicle`, `int n` × `int`;
  - `MOVE` (`MoveEntity*`, `TeleportEntity`, `EntityPositionSync`, `RotateHead`, `SetEntityMotion`): `int id` and the resulting absolute position, rotation, head yaw, motion and `onGround` **after** native applied it (one shape for all);
  - `EVENT` (`EntityEvent`): `int id`, `byte event`;
  - `REMOVE`: `int n` × `int id`.
- Entries are written only while `java_client_running()`; the in-frame mode gets none (E6).
- The local player isn't fed (the Java client's `LocalPlayer` is already the player).
- **Order against the Java server's packets** (DESIGN §10 risk 6): in-process, the Java server's payloads to this client go through `Clients` queues; each hub send to a player carries a per-player sequence number (add `u64 seq` to the hub's per-player outbox and to `Clients`' queue entries); the feed entry carries the seq of the packet it came from, and `ClientBridge.tick` merges both streams by seq before applying. A remote (`Wire`) client gets the same through the `horizon:java` frame order, which is already the connection order.

## 2. Java (`B/ClientEntities.java`, new; `B/mixin/ClientLevelEntitiesMixin.java`)
- `ClientEntities`: `Int2ObjectOpenHashMap<Entity>` of views, a UUID index.
- `ADD`: `EntityType.byString(type)` → `type.create(clientLevel, EntitySpawnReason.LOAD)`; `setId`, `setUUID`; position, rotation and motion written raw (`setPosRaw`, `setYRot`, `setXRot`, `setYHeadRot`, `setDeltaMovement`, `setOldPosAndRot`), no `recreateFromPacket` (it is the packet handler: DESIGN §6 "Never"); put in the map.
- `DATA`: decode with the real `SynchedEntityData.DataValue.STREAM_CODEC` on a `RegistryFriendlyByteBuf` of the client's registry access, then write each raw: `DataItem.setValue` through 1c's accessor, **without** `onSyncedDataUpdated` (that is vanilla client logic native already runs).
- `EQUIP`: `setItemSlot` raw (no equip sound: `Entity.setItemSlot` on a client plays nothing, but go through the field: `LivingEntity` equipment map accessor).
- `PASSENGERS`: `startRiding(v, true, false)`/`removeVehicle` raw on the views (the client's `ClientLevel` keeps `vehicle`/`passengers` on entities).
- `MOVE`: raw position, rotation, motion, `onGround`.
- `EVENT`: nothing (vanilla's `handleEntityEvent` is client logic native runs; mod overrides of `handleEntityEvent` are phase 6).
- `REMOVE`: `setRemoved(DISCARDED)` raw, out of the map.
- The mixin makes `ClientLevel.getEntity(int)`, `getEntities(Entity, AABB, Predicate)`, `getEntities(EntityTypeTest, AABB, Predicate)`, `entitiesForRendering()` and `getEntityCount()` answer from `ClientEntities` (players: the client's player list as now). Vanilla's section manager stays empty. `ClientLevel.tickEntities` must tick nothing: assert it (W0: the Java client duplicates nothing native does).
- World change and disconnect clear the map (`ClientBridge` world-session lifecycle).

## Tests
- `cargo test -p horizon-client --features java-mods --bins java_screens`: the feed entry encoders against a Rust mirror of the reader; `feed_follows_arrival_order`.
- `java_mods.py unit`: `ClientEntitiesCheck` (feed bytes → entities; `getEntities(box)` answers; a REMOVE drops it), GPU-free with a stub level if `ClientLevel` can't be built: test the map and decoder only.
- Test mod, client side (`ClientChecks`, beside `ClientKeys`): `client_level_sees_the_pig` (a pig summoned by the server is found by `Minecraft.getInstance().level.getEntity(id)` within 5 ticks, at its position ±0.1, with its custom name); `client_level_loses_the_removed_pig`.
- `python3 tools/java_mods.py bridges`.
