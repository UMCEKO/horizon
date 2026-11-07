# SPEC-2-15: the tracker pairs at vanilla's moments

Index: `SPEC-2-8.md` (E5). Size: ~300 lines. Needs 2-13. **Changes behaviour toward vanilla**: when pairings happen, and one tick of latency on entity updates (see "Visible change").

## Vanilla
- **Add** (`PersistentEntitySectionManager.addEntity`, patched): join event → section add → `onCreated` → `startTracking` → `ServerLevel.EntityCallbacks.onTrackingStart` → `ChunkMap.addEntity` (`ChunkMap.java:1142`): a `TrackedEntity`, then `updatePlayers(level.players())` **at once**: every player in range gets the pairing bundle (`ServerEntity.addPairing`). A `ServerPlayer` added also pairs with every other tracked entity (`e.updatePlayer(player)`). Then `startTicking`.
- **Remove** (`onTrackingEnd`, `ServerLevel.java:2067`): `ChunkMap.removeEntity` first (`:1168`): `broadcastRemoved` → `removePairing` for each player that saw it (one `ClientboundRemoveEntitiesPacket` with that one id each), then the rest of `onTrackingEnd`, ending with `onRemovedFromLevel` and `EntityLeaveLevelEvent`. A `ServerPlayer` removed is also dropped from every `TrackedEntity` (`removePlayer`).
- **Per tick** (`ChunkMap.tick()`, `:1186`, from `ServerChunkCache.tick` inside `ServerLevel.tick`'s `chunkSource` step, **before** the entity loop): players' chunk views; for each tracked entity whose section changed, `updatePlayers`; then `sendChanges` for each; then `updatePlayers(movedPlayers)` for every entity.
- **Player move** (`ChunkMap.move(player)`, `:1064`, from the move packet's handling): every tracked entity `updatePlayer(player)` at once (the player's own: `updatePlayers`).

## Native today (`lan/entity.rs:740` `Entities::tick`, from `Hub::track_entities` in `tick_lan`)
One pass a tick, **after** the entities ticked: new entities are found by diff and paired then; removed ones leave each client in one batched `RemoveEntities` at the next pass; a player's move is caught at that pass (`v.moved`). Ledger: `ChunkMap.addEntity` "mentioned", `ChunkMap.removeEntity` "mapped inline: … at the next tick" (`port_map/game-001.tsv:71`). These are the missing ports (P2: `StartTracking`/`StopTracking` are listened by Mekanism; P3 of 2-18).

## Change
### 1. Lib (`lan/entity.rs`)
- `Entities::add(&mut self, e: &EntityState, riding: &Riding, viewers: &mut [Viewer], kind) `: `ChunkMap.addEntity`. Creates the state, runs `updatePlayer` for each viewer (range, `broadcastToPlayer`, chunk tracked: the existing `sees`), pairs those that see it (`pairing` bundle, `tracker.seen`, `changes`). For a player entity, then `updatePlayer(that viewer)` for every other state.
- `Entities::remove(&mut self, id: i32, viewers: &mut [Viewer])`: `removeEntity`. For each viewer whose `tracker.seen` has it: one `RemoveEntities { ids: [id] }`, `changes.push((id, false))`. Drop the state. For a player entity, `removePlayer` of that viewer from every state.
- `Entities::player_moved(&mut self, viewer: &mut Viewer, …)`: `ChunkMap.move`'s entity part, for one viewer.
- `Entities::tick` keeps vanilla's per-tick part only: section changes, `sendChanges`, moved players. Its "appeared since last pass" and "gone since last pass" branches are deleted: `add`/`remove` do them now. A state never added (a bug) is logged once and added.
- The batched `RemoveEntities` per client goes: vanilla sends one per removal.

### 2. Bin
- `level_add` (after the push, before `java_hook_entity_added`... check 1d's placement of `onAddedToLevel` against `PersistentEntitySectionManager.addEntity`'s order: join event, section add, `onCreated`, `startTracking`, `startTicking`; keep tracking before ticking) calls `self.lan_track_add(id)`: builds that entity's `EntityState` (the per-kind function `lan_object_states` and friends use, for one id) and calls `Entities::add`. Players: at 1d's player join site.
- `level_remove` (before `java_hook_entity_left`, which is `onRemovedFromLevel` + leave event) calls `lan_track_remove(id)`. Players: at 1d's player leave site.
- The move handling of a guest (`lan_actions.rs`, where `Hub::update_view` runs for a move) calls `player_moved` for that guest.
- `track_entities`' per-tick call moves out of `tick_lan` to `tick_level`, **before `tick_entities`** and after the block ticks, spawning and raids (vanilla's `chunkSource` step). It still builds every entity's state, as now.
- **Pairing report at once:** each of these calls returns its `changes` to the bin, which hands them to `java_tracking` immediately (same op, same layout), so Java's `Tracking.SEEN_BY` is right at every moment. `Hub::take_tracking`'s batched pairings are deleted (view, watch and unwatch entries stay as they are).

### Visible change
Movement and data updates (`sendChanges`) now go out at the start of the next tick's level step, as vanilla's do, so clients see entity movement one tick later than with today's native order (but at vanilla's latency). Tell the parent in the report; the user decides if a playtest is needed.

## Tests
- `lan/entity.rs` tests:
  - `an_entity_added_is_paired_at_once`;
  - `a_removed_entity_gets_one_remove_packet_per_viewer_at_once`;
  - `a_player_joining_pairs_with_every_entity_in_range_at_once`;
  - `a_moving_player_is_paired_at_its_move`;
  - the existing `tracker_reports_its_pairings_once` adapted.
- `game_tests`: `pairing_precedes_the_leave_hook` (the leave hook sees the entity's pairings already gone); `pairing_happens_before_the_entity_ticks` (a test hook in `turn`: the entity's viewers are set).
- `python3 tools/parity.py server_entities dedicated_server` and what `--changed-files` lists for `lan/entity.rs`, `lan/hub.rs`.
- `python3 tools/lan_e2e.py`: the full list (tracking touches every check). Report failures with their check names.
- `python3 tools/java_mods.py bridges`: the tracking-dependent checks (chunk watch, `sendToTracking`) still pass.
