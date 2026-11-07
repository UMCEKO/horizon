# SPEC-13 (W13): `ChunkTicketLevelUpdatedEvent`

Design: `DESIGN.md` §1.2 (60,649 on Forge in the M1 run; Mekanism listens), §1.4, D4. Read first: SPEC-1,
`crates/horizon-client/src/ticket_levels.rs` (the module doc, `Sources`, `ChunkTickets::update`, `compute`, `reach`,
the constants), `G/game.rs` near line 5278 (`self.tickets.update(sources)`), `G/levels.rs` (one `tickets` per level,
swapped in), `B/ChunkViews.java` (how a view chunk is made), vanilla `ChunkMap.updateChunkScheduling`,
`DistanceManager`, `ChunkLevel` (`MAX_LEVEL` = 44, `isLoaded`), `TicketType` in `.local/minecraft/26.3/sources/server`.

Depends on SPEC-1. About 350 changed lines. Code comments: one line at most, only for a non-obvious why.

## Vanilla + NeoForge
`DistanceManager` propagates each ticket's level outwards, +1 per chunk of Chebyshev distance (`ChunkTracker`), each
chunk taking the lowest; a chunk with no ticket in reach has `MAX_LEVEL + 1` = 45. `ChunkMap.updateChunkScheduling(pos,
newLevel, holder, oldLevel)` runs for every chunk whose level changed and, when `isLoaded(old) || isLoaded(new)`
(level ≤ 44), creates or updates its `ChunkHolder`, then (NeoForge, last) `EventHooks.fireChunkTicketLevelUpdated(level,
pos, oldLevel, newLevel, holder)`. Ticket levels: a player's loading tickets are level 31 on every chunk within the view
distance (`PlayerTicketTracker`), so a chunk at distance `d` has `31 + max(0, d - view)`; `FORCED` 31 at the chunk;
`PORTAL` 30 (`PORTAL_LEVEL` in `ticket_levels.rs`); `DRAGON` 24 (`ARENA_LEVEL`). Native's `compute` already uses these
sources but stops at 33 (`NONE`), since nothing native needs more.

## Rust
- `ticket_levels.rs`:
  ```rust
  /// `ChunkLevel.MAX_LEVEL`: the highest level a chunk holder exists at.
  pub const MAX_LEVEL: u8 = 44;
  /// Each chunk's loading level up to `MAX_LEVEL`, as `DistanceManager`'s loading tracker has it (chunks beyond are absent).
  pub fn loading_levels(s: &Sources) -> FxHashMap<(i32, i32), u8>
  /// The chunks whose loading level changed, as `(chunk, old, new)` with 45 for absent, ordered by new level then chunk.
  pub fn level_changes(old: &FxHashMap<(i32, i32), u8>, new: &FxHashMap<(i32, i32), u8>) -> Vec<((i32, i32), u8, u8)>
  ```
  `loading_levels`: for each source `(center, base, flat)`: players `(31, view)`, forced `(31, 0)`, portal
  `(PORTAL_LEVEL, 0)`, dragon `(ARENA_LEVEL, 0)`; every chunk at Chebyshev distance `d` with `base + max(0, d - flat) <=
  MAX_LEVEL` gets that level, the lowest one winning. `level_changes` orders by new level ascending, then
  `ChunkPos.asLong` ascending (D4).
- `ChunkTickets::update` returns `bool`: true when the sources changed (it already compares them).
- `ChunkTickets` keeps `java_loading: Option<FxHashMap<(i32, i32), u8>>` (the last map sent; `None` until the event is
  listened for this level). Only touched when listened.
- `G/game.rs` at the `self.tickets.update(sources)` call: `let changed = self.tickets.update(sources);` then, when
  `changed && self.java_listens(JavaEvent::ChunkTicket)`: `self.java_ticket_levels()`, which computes
  `loading_levels` of the new sources, diffs it against `java_loading` (empty on the first call: every chunk enters from
  45), stores the new map, and sends the changes in one `level_event` id 24 call for the level in place: tail `int n` x
  (`long chunkPos` as `ChunkPos.asLong`, `int old`, `int new`). Result empty.
- A level whose tickets all go (its last player leaves, the level closes): the sources become empty, every chunk goes
  to 45, so the same diff sends them. When the world closes (`java_close_levels`), do the same for each level before
  `server_stopping`, as vanilla's tickets are removed while the server stops (check the order against `MinecraftServer.
  stopServer` and keep it).
- `JavaEvent::ChunkTicket` into `SITED`.

## Java
- `ServerEvents` case 24: for each entry: `holder = ChunkViews.holder(level, pos, newLevel)` then
  `EventHooks.fireChunkTicketLevelUpdated(level, pos, old, new, holder)` through `ServerEvents.post`. Then, for an entry
  with `new > 44`, `ChunkViews.dropHolder(level, pos)` (vanilla drops the holder after unload: the event still gets it).
- `ChunkViews.holder(level, pos, ticketLevel)`: one `ChunkHolder` per loaded-level chunk, kept in a map per level, made
  as `ChunkViews` makes its `LevelChunk` views (no constructor run), with `pos` and the ticket level set through an
  accessor mixin `B/mixin/ChunkHolderAccess.java` (javap `ChunkHolder` and `GenerationChunkHolder` for the fields:
  `pos`, `ticketLevel`, `oldTicketLevel`, `queueLevel`, `levelHeightAccessor`).
- `B/mixin/ChunkHolderViewMixin.java`: for a view holder only (a marker interface or a set of view instances),
  `getFullChunkNow()`, `getTickingChunk()` and `getChunkToSend()` answer the `LevelChunk` view from `ChunkViews` when
  native has the chunk loaded and the holder's level reaches that status (`ChunkLevel.fullStatus(ticketLevel)`:
  `FULL` ≤ 33, `BLOCK_TICKING` ≤ 32, `ENTITY_TICKING` ≤ 31), else null, as a real holder whose future isn't done.
  Other `ChunkHolder` methods keep their bodies; a listener reaching one that needs a real future is found by the live
  test below (JFR) and added here with the same rule, never a guess.
- Debug line: `Horizon: server event ChunkTicketLevelUpdatedEvent n=<n>` per batch (not per chunk).

## Tests
- Rust (`ticket_levels.rs`), independent expected values from vanilla's rule written out by hand:
  - `a_lone_player_rings_out_to_max_level`: view 10, player at (0, 0): (0, 0) → 31, (10, 0) → 31, (11, 0) → 32,
    (23, 0) → 44, (24, 0) absent; the map has (2 * 23 + 1)² entries.
  - `forced_chunk_levels`: a forced chunk alone: 31 at it, 44 at distance 13, absent at 14.
  - `the_lower_source_wins`: a player and a portal ticket overlapping give the minimum at each chunk.
  - `changes_enter_move_and_leave`: from empty to one player: every chunk `(45, level)`; the player moves one chunk
    east: the changes are exactly the chunks whose level differs, ordered by new level then position; to empty: every
    chunk `(level, 45)`.
- Test mod (`ChecksServerEvents`): a listener counting events per chunk and recording one `(old, new)` for the player's
  own chunk; `drive` teleports the player 64 chunks away and back. Checks: `ticket_level_player_chunk_31` (the player's
  chunk reached 31 from 45 at login), `ticket_level_holder_pos` (`event.getChunkHolder().getPos()` equals
  `getChunkPos()` for every event), `ticket_level_leave` (after the far teleport, the old chunk had an event with new
  level 45).

## Done bar
- `cargo test -p horizon-client --lib ticket_levels`, `cargo test -p horizon-client --features java-mods --lib
  java_events`, clippy both ways, fmt.
- `bridges` CHECKs pass. M1 `w13` (whole run): `ChunkTicketLevelUpdatedEvent` count within the comparison's tolerance of
  Forge's. A difference in the set of tickets (a ticket type Horizon lacks, `START`/spawn chunks) shows as a count gap:
  report it to the lead as a native ticket gap (W17), not a W13 bug.
- `HORIZON_PROFILE` on a 5-minute walk: the `level_event` op's share of `java-ticks.log` stays under 1% of the tick.
- Append the debug-line check to `tools/scenarios/w13-events.json`.

## Live test (lead, install-rs, `HORIZON_JAVA_DEBUG=1`)
1. Start with `OPTS` JFR method tracing on `net.minecraft.server.level.ChunkHolder::*` (RULES.md's form), walk 200 blocks
   in a straight line, place a Mekanism Anchor Upgrade-equipped machine or a chunk loader if the pack has one. Dump, `jfr
   print`: list every `ChunkHolder` method Mekanism's handler called; any not served by the view mixin is a follow-up
   for this spec (no exception should appear: `a Java mod threw during ChunkTicketLevelUpdatedEvent` absent).
2. The log has one `ChunkTicketLevelUpdatedEvent n=...` line per chunk-boundary crossing, not per chunk.
