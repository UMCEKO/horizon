# SPEC-2-6: a per-pass context and one turn per mob

Index: `SPEC-2-8.md` (E3). Size: ~300 lines, mostly moved code. Needs 2-5. **No behaviour change.**

## Why
`tick_creature_entities` (`creatures.rs:770`) takes `self.mobs` for the whole pass and keeps its state in locals and three macros (`around!`, `cross!`, `leave!`). In the merged loop (2-13), other kinds' turns run between two mobs, and they need `self.mobs` in place (a projectile hits a mob, a boat pushes one). So the pass state moves into a struct, and one mob's tick becomes a function that takes **only that mob** out of `Game`, by id, for the length of its body.

## 1. `game/tick_pass.rs` (new)
```rust
/// What a pass over the level's entities builds once and every turn reads (vanilla has it live; native caches it and refreshes what a turn changed).
pub(in crate::game) struct TickPass {
    pub shared: MobShared,
    pub snapshot: Vec<Seen>,
    pub index: FxHashMap<u32, u32>,        // mob id → snapshot slot
    pub grid: SeenGrid,
    pub scratch: horizon_core::random::…,   // the level random lent to the mobs (as now)
    pub died: FxHashSet<u32>,
    pub pushes: PendingPushes,
    pub path_writes: Vec<…>,
    pub guests: Vec<…>, pub players: Vec<…>, pub bindings: …, pub guest_ids: Vec<(u32, u32)>,
    pub ticking: Box<dyn Fn(i32, i32) -> bool>,
    // what today's pass hands to its tail; 2-7 empties them into the turns
    pub events: Vec<MobEvent>, pub guest_events: Vec<(u32, MobEvent)>,
    pub dead: Vec<Mob>, pub corpses: Vec<Mob>, pub gone: Vec<(u32, Vec3, f64, f64)>,
}
```
Types: the ones the locals have today; keep their names.

- `Game::begin_pass(&mut self) -> TickPass`: the prelude of `tick_creature_entities` up to the loop (the killed-since-last-tick deaths, `charged_exploders` clear, snapshot, bindings, `leash_boats`, `mob_shared`, index), without taking `self.mobs`.
- `Game::end_pass(&mut self, pass: TickPass) -> Result<()>`: the tail after the loop (pushes for every mob, `level_random`, bindings back, the `gone` poofs, `sync_riders`/`tick_passengers`, the after-loop calls, `mob_died` of `dead`, `corpses`, `handle_mob_events`, guest events, `tick_projectiles`… `outbox`), in today's order.
- `Game::refresh_pass(&mut self, pass: &mut TickPass, t: Touched)`: the body of `around!` after the call: `shared` if `t.shared`, the touched mobs' slots, the appended mobs' slots. Mobs are looked up by id in `self.mobs`.
- The `self.world.random`/`self.died`/`mob_players` swaps of `around!` become `fn lend_pass<R>(&mut self, pass: &mut TickPass, f: impl FnOnce(&mut Self) -> R) -> R`.

## 2. `Game::turn_mob(&mut self, pass: &mut TickPass, id: u32) -> Result<()>`
The loop body of today, for one mob, in the same order:
1. Look the mob up by id. Gone or `removed` → return. Keep a `FxHashMap<u32, usize>` cache in the pass, rebuilt when `self.mob_generation` (new `u32`, bumped by `level_add`/`level_remove` of a mob) changed.
2. Guest binding, the unloaded-chunk branch, the non-ticking despawn branch, `MobBefore`, the tick, the after-tick bookkeeping, `MobAfter`: exactly today's code.
3. The mob's body runs with the mob **taken out of `self.mobs`** by `with_mob_outside` (`entity_life.rs:153`), so `MobWorld` can borrow `self` and the mob is `&mut`. It is back in place before any crossing.
4. A removal decided in the body (`kept == false`) is `level_remove(Kind::Mob, id, why)` at that point, after the mob is back. Nothing is lent: the vector is in `Game`. The `keep` vector, `Marks` and the final `retain` are deleted.
5. Crossings become `self.cross_point(point, &mut [])` followed by `refresh_pass`.

## 3. `tick_creature_entities`
```rust
let mut pass = self.begin_pass();
let ids: Vec<u32> = self.mobs.iter().map(|m| m.id).collect();
for id in ids { self.turn_mob(&mut pass, id)?; }
self.end_pass(pass)
```
The fixed bound is `ids`: mobs added during the pass are not visited, as now.

## 4. `crossing.rs`
`Lent::Mobs` and `LentKinds::MOBS` lose their last user here; delete them if the compiler says so, and `Marks` with them.

## Tests
- The existing `crossing_tests.rs` mob tests pass unchanged (a crossing sees every mob in place; an added mob ticks from the next tick; a removed one is skipped; a moved mob is seen moved).
- New `turn_mob_takes_only_its_mob_out`: in a `MobAfter` hook, `self.mobs` holds every mob, the current one included.
- `python3 tools/parity.py mob_ai server_entities server_player_damage` and what `--changed-files` lists for `creatures.rs`: identical.
- Timing: the 360-mob scenario under `HORIZON_PROFILE`; `tick_mobs` mean within ±3% of the base. Report both numbers.
- clippy `-D warnings`, with and without `java-mods`.
