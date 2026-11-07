# SPEC-2-5: the level's tick list, as `EntityTickList` and section visibility keep it

Index: `SPEC-2-8.md` (E1). Size: ~250 lines. Needs 2-2, 2-3, 2-4 (every entity is a kind). **No behaviour change**: the list is kept exact, but only tests read it until 2-13.

## Vanilla
- `EntityTickList` (`world/level/entity/EntityTickList.java`): an insertion-ordered map by id. `add` appends, `remove` deletes. `forEach` iterates the map **as it was when the pass started** (`iterated`; a change during the pass copies to a new map), so entities added during a pass aren't ticked by it, and entities removed are still visited (the loop skips them by `isRemoved()`).
- Membership is "ticking", from `ServerLevel.EntityCallbacks.onTickingStart`/`onTickingEnd`, which `PersistentEntitySectionManager` calls:
  - **at add** (`addEntity` → `startTicking` when the entity's section is `Visibility.TICKING`, or the entity `isAlwaysTicking()`: players);
  - **at removal** (`stopTicking` in `Callback.onRemove`);
  - **when a chunk's visibility changes** (`updateChunkStatus(pos, status)`, `PersistentEntitySectionManager.java:138`): for each existing section of the chunk, in `SectionPos.asLong` order, each entity of the section in insertion order (players excluded: `isAlwaysTicking`): lost ticking → `stopTicking`, gained ticking → `startTicking`;
  - **when an entity moves to another section** (`Callback.onMove`): its section changes; if the new section's visibility differs, it stops or starts ticking.
- `Visibility.TICKING` is `FullChunkStatus.ENTITY_TICKING`: ticket level ≤ 31 (`ChunkLevel.ENTITY_TICKING_LEVEL`). That is not the same test as the loop's `inEntityTickingRange` (simulation distance), which 2-13 checks per turn.
- Across chunks, vanilla's order is `DistanceManager.chunksToUpdateFutures`, a `ReferenceOpenHashSet` (identity hash): not reproducible. Native uses the order `update_chunk_tickets` reports changes in (state it in a one-line comment where you sort).

## 1. `game/tick_list.rs` (new, always compiled)
```rust
pub(in crate::game) struct TickList {
    order: Vec<Option<Ent>>,              // insertion order, tombstones compacted when over half
    slot: FxHashMap<Ent, usize>,
    section: FxHashMap<Ent, (i64, u64)>,  // SectionPos.asLong of its section, and when it entered it (a counter)
    entered: u64,
}
impl TickList {
    pub fn start(&mut self, e: Ent);                 // onTickingStart: append (debug_assert not present)
    pub fn stop(&mut self, e: Ent);                  // onTickingEnd
    pub fn contains(&self, e: Ent) -> bool;          // tickPassenger's entityTickList.contains
    pub fn pass(&self) -> Vec<Ent>;                  // forEach's snapshot
    pub fn enter_section(&mut self, e: Ent, section: i64);   // stamps `entered`
    pub fn in_chunk_ordered(&self, chunk: (i32, i32)) -> Vec<Ent>; // asLong order, then entry order
}
```
- `SectionPos.asLong` ported exactly (x 22 bits, z 22 bits, y 20 bits, signed compare), so negative-y sections sort as vanilla's sorted set does.
- `Game.tick_list: TickList`, swapped per level in `levels.rs` `Parked` beside `riding_table`.

## 2. Keeping it
- **`level_add`** (after the push): `enter_section(e, section of its position)`; `start(e)` if its chunk is entity-visible.
- **`level_remove`** (and `left_in_loop`, `level_refuse`): `stop(e)`, forget its section.
- **Players** (`Ent::Guest(c)`): `start` where 1d's `player_level` join is sent; `stop` at its leave. Always ticking, never section-driven.
- **Chunk visibility:** `Levels::entity_visible(cx, cz) -> bool` (loading level ≤ 31) in `ticket_levels.rs`. In `update_chunk_tickets`, keep the previous visible set; for each chunk that left it, `stop` its entities; for each that entered, `start` its entities in `in_chunk_ordered` order. Rebuild only when the levels were rebuilt (`ChunkTickets::update` already knows).
- **Section moves:** `fn note_moved(&mut self, e: Ent, pos: Vec3)`: if the section changed, `enter_section`; if the visibility of the new chunk differs from the old, `stop` or `start`. Call it:
  - at the end of each entity loop's per-entity body (all kinds), until 2-13 replaces them with one call per turn;
  - after each crossing, for the touched ids;
  - at the native movers outside an entity's own tick: explosion and wind knockback, piston pushes, `/tp`, `/spreadplayers`, portal arrivals, `teleport_to_in_level`, and the position bridges (`setPos`, `teleportTo`, `snapTo`, `teleport`, `move`). Grep each of those families and list in the report which functions you added it to.
- The reversed ledger verdicts: in `tools/port_map/classes-05.tsv`, replace the `skip:` of `EntityTickList.forEach` and `PersistentEntitySectionManager.updateChunkStatus` with the ports (`tick_list.rs`, `update_chunk_tickets`). Run `python3 tools/port_ledger.py check --base origin/main`.

## Tests (`game/tick_list.rs`, and `game_tests` where a level is needed)
- `as_long_sorts_negative_sections_after_positive_ones`.
- `the_pass_is_the_order_entities_started_ticking` (summon pig, item, armor stand → that order).
- `an_entity_added_during_a_pass_is_not_in_it` and `one_removed_during_a_pass_is_still_visited` (`pass()` snapshot semantics).
- `a_chunk_becoming_ticking_appends_its_entities_by_section_then_entry`.
- `an_entity_walking_out_of_ticking_range_and_back_goes_to_the_end`.
- `players_are_always_ticking`.
- Build and clippy with and without `--features java-mods`; `python3 tools/parity.py server_entities mob_ai` unchanged.
