# SPEC-4-2: crossings from inside a mob's own tick

Design: `DESIGN.md` §2 "Re-entrancy" (Java's writes during an entity's turn land in the turn's events, in order), Corrections (griefing and visibility call Java at the site). Index: `SPEC-2-8.md`. Size: ~280 lines. Needs phase 2.

## Why
Most of phase 4's sites are inside lib code running one mob's body (`Mob::tick` with `&MobWorld`, which borrows `Game`): fall damage, jumps, target changes in goals and brains, mob griefing (14 `w.mob_griefing` reads), visibility (`mobs.rs:2125`, `goals.rs:712`). A crossing there can't have `&mut Game`: the lib holds shared borrows. 2-17 split the tick around one site; that doesn't scale to 25.

## Mechanism: shared crossings with deferred writes
- `MobWorld` gains `pub java: Option<&'a InTurn>` (`None` without `java-mods` or when no site of this mob's type is admitted: one bit per type, computed with the pass).
- `InTurn` (bin, java-mods) holds a raw handle to the host and `RefCell<Vec<MobEvent>>` for deferred writes. `fn ask(&self, site: Site, subject: SubjectBytes) -> Answer` sends the site's op **in shared mode**.
- **Shared mode** (`H/bridge.rs` `with_shared(game: &Game, f)`): bridged natives run with `&Game`.
  - Reader bridges (queries, `groups`) answer from `&Game`. The `java_bridges!` macro gets a `reads` marker for entries whose body only needs `&Game`; their bodies take `&Game`.
  - Writer bridges (the analysis' writers) **don't run**: they append `MobEvent::Java { bridge, args }` to the `InTurn` queue, update the Java view's group locally (so later reads in the same crossing see the write, as on Forge), and return. The turn's event handling (2-7) runs them, in order, with `&mut Game`, at the position of the site among the mob's other events.
  - A writer with a **return value** can't be deferred honestly. The analysis (SPEC-0 `writes` per listener, made per listener method by 2-1's closure walker over mod listeners) lists, for each in-turn site's events, the non-void writers its admitted listeners reach; any is `in-turn-writer <event>: <key>` in `java_mods.py check`, and that event isn't served for the pack (refused loudly at boot, like `needs-bridge`).
- **Sites** (`Site` enum): `Fall`, `Jump`, `ChangeTarget`, `Grief`, `Visibility`, `Knockback`, `Teleport` (4-5, 4-6, 4-8). Each site is a call `w.java.map_or(default, |j| j.ask(…))` at the vanilla place in the lib.

## Order this gives
Mod reads at the site: exact (views refreshed in the request). Mod writes: applied after the mob's own body, before its turn's other effects that came later; the same model native uses for the mob's own effects (2-7). State it in the step's report as the one approximation phase 4 carries, with the user's earlier choice (DESIGN Corrections) noted; the exact alternative is a `MobWorld` that holds no borrow of `Game` (every field read through a host trait), about 1,000 mechanical lines over `mobs/`.

## Tests
- `cargo test -p horizon-java-host` (shared mode: a writer entry queues, a reader entry answers).
- `game_tests`: `a_shared_crossing_defers_a_write_to_the_turns_events` (a test site whose hook calls a writer; the write is applied after the body, before `MobAfter`).
- `python3 tools/java_mods.py check --analysis` on a synthetic analysis with an in-turn listener calling `hurtServer`: reports `in-turn-writer`.
