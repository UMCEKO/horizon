# SPEC-1a: re-entrant entity loops (Rust only)

Design: `DESIGN.md` §0 decision 4 (exact order) and §2 "Re-entrancy". Prerequisite of SPEC-1b and every event phase.

## Why
- Exact vanilla order needs synchronous Rust→Java crossings in the middle of native entity loops: an entity's leave when the loop removes it, its tick events, its damage events.
- Today 14 loops take an entity vector out of `Game` (`std::mem::take(&mut self.mobs)` …). The mob loop also holds `MobWorld`, which borrows `&self` for the whole loop (`creatures.rs` 652–851, `mob_world_base` at 537). So `&mut Game` isn't available between two entities.
- A crossing needs `&mut Game` with every entity in place, because Java's bridges read and write them. After it, the loop must go on with exactly the state the crossing left.

## What this phase delivers (final)
1. `Game::lend`: puts the vectors a loop took back into `Game` for the duration of a closure, then takes them again.
2. Every one of the 14 loops can make a crossing at the boundary of each entity, under three invariants that match vanilla's `EntityTickList` iteration:
   - **Fixed bound:** the loop ticks the entities present when it started. An entity added during a crossing is appended, and is not ticked this pass (vanilla ticks it from the next tick).
   - **Removal marks:** an entity removed during a crossing is marked, skipped from then on, and dropped by the loop's existing final `retain`/filter. Positions never shift mid-loop.
   - **Fresh after a crossing:** borrow-derived state (`MobWorld`, snapshots, grids) is rebuilt from `Game` after a crossing. Only the parts that crossing could have changed are rebuilt, via `Touched`.
3. `MobWorld` no longer needs a borrow of `Game` that lasts the loop. The costly parts are built once into an owned `MobShared`; the cheap borrows are taken again after each crossing.
4. In this phase the crossing points call a hook that does nothing outside tests. SPEC-1b and later phases put work there. Nothing behaves differently: parity harnesses and timing are unchanged.

## Code
### 1. New `crates/horizon-client/src/game/crossing.rs` (always compiled)
```rust
//! Native entity loops lend their vectors back to `Game` so Java can run between two entities, in vanilla's order.
use super::*;

/// What a crossing changed through the native entity API, for the loop to refresh.
#[derive(Default)]
pub(in crate::game) struct Touched {
    pub ids: Vec<u32>,
    /// Players, teams, rules or blocks changed: the loop's shared state must be rebuilt.
    pub shared: bool,
}

/// One vector a loop took out of `Game`.
pub(in crate::game) enum Lent<'a> {
    Mobs(&'a mut Vec<Mob>),
    Items(&'a mut Vec<ItemEntity>),
    Orbs(&'a mut Vec<ExperienceOrb>),
    Projectiles(&'a mut Vec<Projectile>),
    Stands(&'a mut Vec<horizon_client::entities::ArmorStand>),
}

impl Game<'_> {
    /// Runs `f` with `lent` back in place; what `f` appended stays at the end, what it removed is marked (SPEC-1b).
    pub(in crate::game) fn lend<R>(&mut self, lent: &mut [Lent<'_>], f: impl FnOnce(&mut Self) -> R) -> R { … }
    /// Records that the native entity API changed `id` (or shared state) during a crossing.
    pub(in crate::game) fn note_touched(&mut self, id: Option<u32>) { … }
    pub(in crate::game) fn take_touched(&mut self) -> Touched { … }
    /// Which kinds are lent right now: `level_remove` (SPEC-1b) marks instead of removing them.
    pub(in crate::game) fn lent(&self) -> LentKinds { … }
}
```
- `lend` swaps each `Lent` vector with the `Game` field (`self.mobs`, `self.dropped`, `self.orbs`, `self.creatures.projectiles`, `self.armor_stands`) and sets the `LentKinds` bit. It runs `f`, swaps back and clears the bit.
- A nested `lend` of the same kind is a bug: `debug_assert`.
- New `Game` fields, in `game.rs` beside `next_entity`: `touched: Touched` and `lent: LentKinds`, a `u8` bitset. Add the `mod crossing;` line.
- `Projectile` and `ExperienceOrb` paths are the ones `game.rs` already imports. Check `creatures::Creatures` for `Projectile`'s path.

### 2. The crossing point
```rust
/// Between two entities of a loop: where later phases cross into Java. Does nothing outside tests in this phase.
pub(in crate::game) fn cross_point(&mut self, point: CrossPoint, lent: &mut [Lent<'_>]) -> Touched {
    #[cfg(test)]
    if let Some(hook) = self.test_cross.take() {
        let hook = self.lend(lent, |g| { hook(g, point); hook });
        self.test_cross = Some(hook);
    }
    self.take_touched()
}
```
- `CrossPoint`: an enum naming the loop and the side: `MobBefore(id)`, `MobAfter(id)`, `ItemAfter(id)`, `OrbAfter(id)`, `ProjectileAfter(id)`, `ExplosionMob(id)`, `ExplosionItem(id)`, `WindMob(id)`, `WindStand(id)`, `WindProjectile(id)`, `VillagerWake(id)`, `Creaking(id)`, `TouchArrow(id)`, `GuestArrow(id)`, `RemoteProjectile(id)`. Later phases match on it.
- `test_cross: Option<Box<dyn FnMut(&mut Game, CrossPoint)>>` is a `#[cfg(test)]` field on `Game`.
- Release builds compile `cross_point` to `take_touched`. When nothing was touched, that is a `Vec::new()` swap.

### 3. `MobWorld` without a long borrow (`horizon-client/src/mobs.rs` 2140–2200, `game/creatures.rs` `mob_world_base`)
- **Split** `mob_world_base` into:
  - `fn mob_shared(&self) -> MobShared`: owned. Everything computed from scans: `player: Option<PlayerView>`, the weather and daylight bools, rule bools and ints (`monsters_burn`, `mob_griefing`, `mob_drops`, `max_entity_cramming`), `game_time`, `day_time`, `nether`, `end`, `world_seed`, `sea_level`, and `teams`.
  - `fn mob_world<'s>(&'s self, shared: &'s MobShared, others: &'s [Seen]) -> MobWorld<'s>`: only references (`world`, `registry`, `facts`, `fluid_tables`, `movement`, `collision`, `data`, `leash_boats`, `block_tags`, `biomes`, `items`, `lid_openers`) plus copies from `shared`. No scan, no allocation.
- **`Teams<'a>`** (mobs.rs 2196–2200) borrows scoreboard `Team`s, so it can't live in `MobShared`. Make it owned: `Teams { player: Option<Team>, mobs: HashMap<u32, u16>, teams: Vec<Team> }`, built once, with the same queries. Keep `Arc<Teams>`.
- **The mob loop** (`tick_creature_entities`): build `shared` once before the loop, and `let mut w = self.mob_world(&shared, &snapshot)` per mob, as `..base.clone()` does now. Nothing borrows `self` across iterations, apart from `w` inside one iteration.
- **After each crossing:**
  - if `touched.shared`, rebuild `shared = self.mob_shared()`;
  - for each touched id that is a mob of this loop, recompute `snapshot[index]` (`seen(m, uuid)`) and `grid.update`. The index comes from `index: FxHashMap<u32, u32>` (it exists), extended when entities are appended.

### 4. The 14 loops: the same pattern each
For each loop below:
1. Fix the bound: `let n = v.len();` and iterate `for i in 0..n` (or `while i < n`).
2. Skip marked entities at the top of an iteration: `if v[i].removed { continue; }`. For kinds without a `removed` field, add `pub removed: bool` (default false) to the lib struct: `ItemEntity`, `ExperienceOrb`, `Projectile`, `ArmorStand`. Mobs have one.
3. End the per-entity borrows (`m`, `w`, iterators) before the crossing, then `let t = self.cross_point(CrossPoint::…(id), &mut [Lent::…(&mut v)]);`, then refresh as §3 says. Loops with parallel vectors (`keep`, `snapshot`) extend them for entries appended by the crossing (`keep.push(true)`, `snapshot.push(seen(..))`).
4. The loop's existing final `retain`/filter also drops `removed` entries.

| Loop | Crossing points |
|---|---|
| `creatures.rs:672` `tick_creature_entities` (mobs) | `MobBefore(id)` before `apply_mob_pushes`; `MobAfter(id)` after `snapshot[i] = seen(m, uuid)`; also after the non-ticking `despawn_check` branch (`MobAfter`) |
| `game.rs:3171` `tick_items` | `ItemAfter(id)` after each item's tick |
| `game.rs:11707` `tick_experience` | `OrbAfter(id)` |
| `creatures.rs:2840` `tick_projectiles` | `ProjectileAfter(id)` |
| `creatures.rs:2797` `touch_arrows` | `TouchArrow(id)` |
| `explosion.rs:438` and `487` `explosion_hurt_entities` (mobs, then items) | `ExplosionMob(id)`, `ExplosionItem(id)` after each entity's hit |
| `wind.rs:271`, `297`, `347` `wind_knockback` | `WindMob`, `WindStand`, `WindProjectile` |
| `villagers.rs:511` `wake_villager` | `VillagerWake(id)` after the change |
| `creaking.rs:111` `with_heart` | `Creaking(id)` after the closure |
| `lan_actions.rs:3602` `guest_arrows` | `GuestArrow(id)` |
| `remote_projectiles.rs:556` `tick_remote_projectiles` | client side (`remote`): `RemoteProjectile(id)`; there for phase 5's client events |

Where a loop calls into another `Game` method while the vector is taken, keep that call inside the iteration. Only the crossing needs the borrows ended.

## Tests
- **Rust, `game_tests.rs` (or a new `game/crossing_tests.rs`), using `test_cross`:**
  - `a_crossing_sees_every_entity_in_place`: in `MobAfter(id)`, `self.mobs` holds every mob of the level, the current one included.
  - `an_entity_added_in_a_crossing_ticks_from_the_next_tick`: spawn a mob in the hook at the first `MobAfter`; its `age` is unchanged after this tick, and +1 after the next.
  - `an_entity_removed_in_a_crossing_is_skipped`: mark the next mob `removed`; it doesn't tick, and it is gone after the tick.
  - `a_moved_mob_is_seen_moved_by_the_next`: move mob A far away in the hook and `note_touched(Some(A))`; mob B's `others` snapshot shows A at the new position in the same tick.
  - The same three for items and orbs.
- **Benchmark:** `cargo bench` has nothing for mobs, so use the existing harnesses' wall time:
  - `python3 tools/parity.py mob_ai server_entities server_player_damage` (results identical);
  - an hz_scenario run with 360 mobs (`/tmp/hz-poc/agents/entities/mobs.json`) under `HORIZON_PROFILE`, where `tick_mobs` mean must stay within ±3% of 2.5 ms at 364 mobs (DESIGN §1.2).
- `cargo clippy -p horizon-client -- -D warnings`, and the same with `--features java-mods`.

## How the parent tests it
- The three parity harnesses pass unchanged.
- The `mobs.json` scenario passes, with the `tick_mobs` mean figure in the report.
- No user steps: nothing changes for players.
