# SPEC-5-2: client entity events fired by native

Design: `DESIGN.md` §6 item 7 (client events fired by native: join and leave at `remote.rs` add and remove, `EntityTickEvent` at the remote entity tick, only when listened and guard-admitted). Index: `SPEC-2-8.md`. Size: ~200 lines. Needs 5-1, 2-1 (client-side admits: `facts.admits(event, type, true)`).

## Vanilla client
- `ClientLevel.addEntity(entity)` → `EntityJoinLevelEvent` (patched `TransientEntitySectionManager`/`ClientLevel` path: confirm with `javap -c -p` of the patched **client** jar `minecraft-client-patched-26.3.0.101.jar`), cancel → not added.
- `ClientLevel.removeEntity(id, reason)` → `onClientRemoval`, `EntityLeaveLevelEvent`.
- `ClientLevel.tickEntities` → `tickNonPassenger` with `EventHooks.fireEntityTickPre/Post` (find the client's caller in the patched client jar with `callers.sh` pointed at it).

## Change
- These fire **in Java's client tick**, where `ClientEntities` applies the feed (5-1), because that is the moment the Java client's level changes; the window has already applied the packet.
  - `ADD`: if `admits(EntityJoinLevelEvent, type, client)`: post `new EntityJoinLevelEvent(view, clientLevel)` before putting it in the map; canceled → not added, and Rust is told in the tick's status (`int n` × `int id` refused). Native then removes the entity from its client vectors (vanilla's client never had it): `remove_entity(id)`.
  - `REMOVE`: `onClientRemoval` (`safely`), then `EntityLeaveLevelEvent` if admitted.
- **Client tick events:** native's `tick_remote_entities` (`remote.rs:5010`) ticks the client's entities. For each entity whose type admits `EntityTickEvent.Pre`/`Post` on the client, it appends a `TICK` feed entry (`int id`, `byte` mask of Pre/Post) in its tick order, after the entity's `MOVE`. Java's client tick, at its `ClientLevel.tickEntities` moment (inside `Minecraft.tick`'s level tick, which `ClientBridge.tick` runs), posts Pre and Post for those entries in order, with no body between (native ticked it). A Pre cancel can't skip native's already-run tick: the client tick event's cancel is reported once per type as `Horizon: client EntityTickEvent.Pre canceled for <type>; native ticks client entities` (DESIGN decision 3: the Java client duplicates nothing). List it in the report for the user.
- Admission bits per type for the client side come from `entities.bin` (`side`).

## Test mod (client)
`ClientEvents`: on the client bus, count `EntityJoinLevelEvent`/`EntityLeaveLevelEvent` for pigs and `EntityTickEvent.Post` for a tagged pig. Checks: `client_join_once`, `client_leave_once`, `client_tick_post_per_tick` (≈ 20 in 20 ticks).

## Tests
`java_mods.py build`, `bridges`; `cargo test … java_screens` (the `TICK` entry).
