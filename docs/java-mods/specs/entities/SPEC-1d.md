# SPEC-1d: EntityJoinLevelEvent and EntityLeaveLevelEvent at the real sites

Design: `DESIGN.md` §4 rows 1 and 2. Needs SPEC-1b (choke points) and SPEC-1c (views at join and leave, the `entity_view` and `entity_left` ops).

## Vanilla, from the patched jar (javap)
- **Join:**
  - `PersistentEntitySectionManager.addEntity(entity, loaded)` first posts `new EntityJoinLevelEvent(entity, entity.level(), loaded)`. If it is canceled, it returns false; the entity is never in the level.
  - After a successful add, NeoForge's `Entity.onAddedToLevel()` runs as the entity starts being tracked.
- **Leave:**
  - `Entity.setRemoved(reason)` sets `removalReason`, then the level callback's `onRemove` → `ServerLevel.EntityCallbacks.onTrackingEnd`, whose last steps are `entity.onRemovedFromLevel()` and then posting `new EntityLeaveLevelEvent(entity, level)`.
  - A canceled join has no leave.
- **Players** join through `ServerLevel.addPlayer` (posted the same way) and leave through `removePlayerImmediately`.

## Goal (final)
**Non-players:**
- The java-mods arm of `java_hook_entity_joining` posts the join in op `entity_view`, after the view is built (or adopted for a Java spawn) and before Rust adds the entity.
  - It is posted only if `facts.admits(EntityJoinLevelEvent, type, server)`. NeoForge itself listens, so in practice always, but the fact decides.
  - A cancel makes the reply `false`. Java drops the view (removal reason `DISCARDED`, raw, no leave event), and Rust doesn't add the entity.
  - On success, `onAddedToLevel()` runs in Java after Rust has added the entity, in op `entity_added` (`int id`), sent from `level_add` right after the push. That is `startTracking`'s moment.
- `java_hook_entity_left` (op `entity_left`): the removal reason is set raw, then `onRemovedFromLevel()`, then `EntityLeaveLevelEvent` (if admitted), in that order.

**Players:**
- A player's view joins its level where native places it in that level, with a join event (`loadedFromDisk` false, as `addPlayer`'s is):
  - login: the `GameBridge.fill` "joined" branch, before `PlayerLoggedIn`, which is the order of `placeNewPlayer`;
  - respawn and dimension change: native's site (`levels.rs`/`server_travel.rs`, where the player's level changes).
- It leaves with `UNLOADED_WITH_PLAYER` at logout (`player_left`), `CHANGED_DIMENSION` at travel, and `DISCARDED` at respawn.
- A canceled player join is logged loudly and ignored, as vanilla's `addPlayer` ignores the result.
- **Catch-up:** entities loaded before the Java server started (`java_level_running()` false at their add) get their view and a join with `fromDisk = true` when the level starts running (`java_chunks_changed`' catch-up point), in vector order, kinds in `Kind` order. That is vanilla's order: a level's entities load after the server's start.

## Wire changes
- `entity_view`'s reply bool is now the join result.
- New op `entity_added`: `text dim`, `long time`, `int id`. Reply: effects.
- New op `player_level`: `uuid` (2 `long`), `text dim`, `byte kind` (0 join, 1 leave), `byte reason`. Reply: `bool` (join result), effects.

## Java
- **`EntityViews.join`:**
  ```java
  var event = new EntityJoinLevelEvent(view, level, fromDisk);
  if (EntityFacts.admits(event, view)) GameBridge.safely("EntityJoinLevelEvent", () -> NeoForge.EVENT_BUS.post(event));
  ```
  If canceled: `LIVE.remove`, set removal reason `DISCARDED`, reply false.
  `EntityFacts.admits` reads the per-type bit of SPEC-0; it is a static map lookup.
- **`EntityViews.added(id)`:** `safely("onAddedToLevel", view::onAddedToLevel)`.
- **`EntityViews.left`:** after the raw reason, `safely("onRemovedFromLevel", view::onRemovedFromLevel)`, then the leave event if admitted.
- **Players:** `GameBridge.playerLevel(input)` posts the same events on the player view (`PLAYERS`), with `onAddedToLevel`/`onRemovedFromLevel` around them in the same order.
- **Debug:** with `HORIZON_JAVA_DEBUG`, one line per join, cancel or leave: `Horizon: entity <id> <type> joined|refused|left <reason>`. The scenario reads it.

## Rust
- `java_entities.rs`:
  - `java_entity_joining` returns the reply's bool;
  - `java_entity_added(id)` is called from `level_add` after the push (a third `java_hooks.rs` pair, `java_hook_entity_added`; the not-arm is empty);
  - `java_player_level(uuid, join, reason)` is called from the player sites above.
- **Catch-up:** in `java_mods.rs` where a level becomes running (`java_chunks_changed`' `behind` branch), call `java_entities_catch_up()`. It sends `entity_view` (fromDisk) then `entity_added` for every entity of the level, in order. A refused one is removed with no leave, through `level_remove`'s quiet variant `level_refuse(kind, id)`, which doesn't run the leave hook.

## Test mod
- **Listeners** in `TestMod` (server side only; the guard `!e.getLevel().isClientSide()` is the shape SPEC-0 proves):
  - record joins (count, fromDisk) and leaves (reason from `getEntity().getRemovalReason()`);
  - cancel the join of any `minecraft:bat`.
- **`ChecksEntities.later`:**
  - `join_fired_once_fresh` (the `run` chicken: count 1, not from disk);
  - `join_before_level`: inside the listener, `level.getEntity(e.getEntity().getId())` was null when it fired;
  - `leave_with_reason`: the wand's diamond, killed by the harness, left `DISCARDED` (`/kill` on an item is `kill()` → `KILLED`; use `discard` in `run` on a second item for `DISCARDED`, and `/kill` for `KILLED`);
  - `refused_bat_absent`: `java_mods.py` summons a bat before the probe; no bat in the box, and no leave recorded for it.
- **`afterChange`** (second session): `join_from_disk` (the chicken's join count 1 with fromDisk).
- **`tools/java_mods.py` `drive`:** before the probe, `command summon minecraft:bat ~ ~1 ~`, `command kill @e[type=item,nbt={Item:{id:"minecraft:diamond"}}]`, `wait 5`.

## hz_scenario: `tools/scenarios/entities.json` (new)
1. `/gamemode creative`; `wait 40`.
2. 20 × `/summon minecraft:pig ~N ~1 ~2`; `wait 20`.
3. `check {"log": "Horizon: entity \\d+ minecraft:pig joined", "since": "start"}`.
4. `/kill @e[type=minecraft:pig]`; `wait 60`.
5. `check {"log": "Horizon: entity \\d+ minecraft:pig left KILLED"}`.
6. `check {"no_exceptions": true}`.

Run it with `HORIZON_JAVA_DEBUG=1 HORIZON_PROFILE=OUT/profile`. Report the `entity_view`, `entity_added` and `entity_left` means from `profile_report.py`, and the `EntityJoinLevelEvent` time. These are inputs for DESIGN §1.3 and §10 risks 2–3.

## Tests to run
- `cargo test -p horizon-client --features java-mods --bins java_entities`;
- `java_mods.py build`, `check`, `bridges` (all CHECKs, both sessions);
- one hz_scenario run of `entities.json` through the gate, with private jars;
- `python3 tools/parity.py server_entities dedicated_server`.

## How the parent tests it
- `bridges` ok.
- Scenario ok.
- In a Mekanism + Jade world: no `a Java mod threw during EntityJoinLevelEvent`/`EntityLeaveLevelEvent`; Jade's entity tracking works on the server (phase 5 shows it on screen).

No user steps.
