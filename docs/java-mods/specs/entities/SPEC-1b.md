# SPEC-1b: one native choke point for entity adds and removes (Rust only)

Design: `DESIGN.md` §0 decision 4, §4 rows 1 and 2. Needs SPEC-1a (crossings). Site list: `/tmp/hz-poc/agents/entities/lifecycle-sites.txt` (60 adds, 113 removes, plus the dragon).

## Goal
Every non-player entity enters and leaves a native level through two functions, at the moment vanilla does:
- `level_add(entity, origin)` runs the join hook **before** the entity is in its vector. A `false` from the hook means it is never added. This is vanilla's `PersistentEntitySectionManager.addEntity(entity, loaded)`: `EntityJoinLevelEvent` first, and a cancel returns false.
- `level_remove(kind, id, reason)` marks or removes the entity, then runs the leave hook. This is vanilla's `setRemoved(reason)` → `onTrackingEnd` → `onRemovedFromLevel`, then `EntityLeaveLevelEvent`, synchronous at the remover's call.

The hooks are `cfg` pairs in `java_hooks.rs`. In this phase both arms are trivial: join returns `true`, leave does nothing. SPEC-1d gives the java-mods arm its body. Players are not in scope; their views are SPEC-1d's.

## Code
### 1. New `crates/horizon-client/src/game/entity_life.rs` (always compiled)
```rust
//! Where native entities enter and leave a level, as `PersistentEntitySectionManager.addEntity` and `Entity.setRemoved` do.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum Kind { Mob, Item, Orb, Projectile, Stand, Frame, Painting, Boat, Cart, Other, Crystal, Cushion, Dragon }

/// `addEntity`'s `loaded`: read from a chunk's saved entities, or new to the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum Origin { Fresh, FromDisk }

/// `Entity.RemovalReason`, in its declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum Removal { Killed, Discarded, UnloadedToChunk, UnloadedWithPlayer, ChangedDimension }

pub(in crate::game) enum NewEntity {
    Mob(Mob), Item(ItemEntity), Orb(ExperienceOrb), Projectile(Projectile), Stand(ArmorStand), Frame(ItemFrame),
    Painting(Painting), Boat(boats::Boat), Cart(minecarts::Minecart), Other(commands::OtherEntity),
    Crystal(dragon::Crystal), Cushion(Cushion), Dragon(Box<dragon::Dragon>),
}
impl NewEntity { pub fn kind(&self) -> Kind { … } pub fn id(&self) -> u32 { … } }

impl Game<'_> {
    /// Adds `e` to the level unless the join is refused; its id, or None. `saved` is the compound a chunk load read it from.
    pub(in crate::game) fn level_add(&mut self, e: NewEntity, origin: Origin, saved: Option<&Tag>) -> Option<u32> { … }
    /// `Entity.remove(reason)` of the entity `id`; false when it isn't in the level.
    pub(in crate::game) fn level_remove(&mut self, kind: Kind, id: u32, why: Removal) -> bool { … }
}
```
Add `mod entity_life;` to `game.rs`.

**`level_add`:**
1. `if !self.java_hook_entity_joining(&e, origin, saved) { self.forget_entity(e.id()); return None; }`. `spawn_loaded_entities` passes the tag it read; every other site passes `None`.
2. Push `e` into its vector (`self.mobs`, `self.dropped`, `self.orbs`, `self.creatures.projectiles`, `self.armor_stands`, `self.item_frames`, `self.paintings`, `self.boats`, `self.carts`, `self.others`, `self.crystals`, `self.cushions`; `self.dragon = Some`).
3. `self.note_touched(Some(id))` (SPEC-1a). Return `Some(id)`.

During a crossing, a lent vector is the one in `Game` (SPEC-1a `lend`), so the push appends to it, which is what SPEC-1a's fixed bound expects.

**`level_remove`:**
1. Find the entity by id in its vector; return `false` if absent or already `removed`.
2. If `self.lent()` has its kind (we are inside a crossing of that loop), set `removed = true` and leave it in place; the loop drops it. Otherwise remove it from the vector now (`retain` on id, or `swap_remove` where the site did).
3. `self.note_touched(Some(id))`.
4. `self.java_hook_entity_left(kind, id, why)`.
5. Return `true`.

Bookkeeping a site did around its removal (`forget_entity`, passengers, leashes, `died` sets) stays at the site, in the same order relative to the removal as now. The hook comes at the end of `level_remove`, so it sees the level after removal, as `onTrackingEnd` does.

### 2. `game/java_hooks.rs`
```rust
pub(super) fn java_hook_entity_joining(&mut self, e: &entity_life::NewEntity, origin: entity_life::Origin, saved: Option<&Tag>) -> bool
pub(super) fn java_hook_entity_left(&mut self, kind: entity_life::Kind, id: u32, why: entity_life::Removal)
```
- Both arms are trivial now: `let _ = (e, origin, saved); true` and `let _ = (kind, id, why);`.
- A `#[cfg(test)]` field `test_life: Option<Box<dyn FnMut(&mut Game, LifeEvent) -> bool>>` lets tests observe and refuse joins. The hook calls it when set.

### 3. Converting the sites
Every line of `lifecycle-sites.txt` is converted. The test in §4 enforces it.

**Adds** (`.push`, `.extend`, `.append`, `.insert` on an entity vector; `self.dragon = Some`) become `self.level_add(NewEntity::…(e), origin)`.
- `origin = FromDisk` only inside `persist.rs` `spawn_loaded_entities` when it reads a chunk's entities. Give it a parameter `origin: Origin`:
  - `persist.rs` 712, 1228, 1261, 1270 and `saving.rs:170` pass `FromDisk`;
  - `commands/data.rs:720` (`spawn_entity_from_nbt`: summon, Java spawns) passes `Fresh`;
  - `command_harness.rs:142` passes `FromDisk`.
- Everything else is `Fresh`: natural spawning, breeding, conversions, projectiles, drops, orbs, vehicles placed, dimension arrivals. Vanilla's arrival is a fresh copy: `Entity.teleport` → `addDuplicateFreshEntity`.
- A site that read the id or index after pushing uses the returned `Option<u32>`. `None` means the entity doesn't exist; skip what followed (its sound, its passengers) exactly as if the spawn had failed.

**Removes** (`.retain`, `.remove`, `.swap_remove`, `.drain`, `.clear`; `self.dragon = None`/`take()`) become a `level_remove` per removed entity, at the point the site decides it.
- A `retain` with a predicate becomes: collect the ids whose predicate is false, in vector order, then `level_remove` each. This happens right where the `retain` was, so the order against the code before and after it is the same.
- A removal decided **inside one of SPEC-1a's loops**, on the taken vector (pickup in `tick_items`, a projectile's hit in `tick_projectiles`, a mob's despawn and death-body removal in `tick_creature_entities`, an orb merge…): set the entity's `removed = true` and its `removal: Option<Removal>` at the decision. End the iteration's borrows, then call `self.lend(&mut [Lent::…], |g| g.java_hook_entity_left(kind, id, why))`, with `note_touched` like `level_remove`'s steps 3 and 4. Do it before any later step of the same entity's turn that has effects outside the entity: stats, sounds, events. If a later step would need the entity's borrow again, re-take it by index after the crossing. Add a `pub removal: Option<Removal>` field next to SPEC-1a's `removed` on the kinds that need it.

**Reasons:** the vanilla method the native site ports decides the reason. Put the vanilla line in the PR description, never in a comment.

| Native removal | Vanilla | Reason |
|---|---|---|
| A living entity's body after `tickDeath` (death_time ≥ 20) | `LivingEntity.tickDeath` → `remove(KILLED)` | `Killed` |
| `/kill`, the void, `kill()` on a non-living entity (boats, carts and stands broken by damage that ports `kill`) | `Entity.kill` → `remove(KILLED)` | `Killed` |
| Despawn | `Mob.checkDespawn` → `discard()` | `Discarded` |
| Item or orb picked up, merged, expired; a projectile hitting or expiring; a vehicle broken by a player (`destroy` → `discard`), a frame or painting dropped | `discard()` | `Discarded` |
| A conversion's old entity (zombie villager cure, piglin zombification) | `Mob.convertTo` → `discard()` | `Discarded` |
| A chunk's entities when it unloads or saves on unload; every entity at world close | `PersistentEntitySectionManager` unload → `UNLOADED_TO_CHUNK` | `UnloadedToChunk` |
| A vehicle leaving with its logging-out player | `PlayerList.remove` → `UNLOADED_WITH_PLAYER` | `UnloadedWithPlayer` |
| An entity leaving through a portal (the copy is added in the target level) | `Entity.teleport` → `removeAfterChangingDimensions` → `CHANGED_DIMENSION` | `ChangedDimension` |

A site that matches none of these rows is listed in the PR with its vanilla counterpart, for review.

**Not removals:** `levels.rs` swapping whole levels in and out (`enter_level`), the mob loop's `mem::take`/`append`, and SPEC-1a's `lend`. Those move vectors, not entities. Bulk `clear()` at world close is a removal (`UnloadedToChunk` for each, in vector order, kinds in the `Kind` order above).

### 4. Tests
- **`tests/test_entity_lifecycle_sites.py` (Python unittest, no game):** scans `crates/horizon-client/src/game/**/*.rs` and `game.rs` for direct mutations of the 12 vectors and `self.dragon = Some/None/take()`, with the regexes `lifecycle-sites.txt` was made with. It fails on any match outside an allowlist:
  - `entity_life.rs`;
  - `crossing.rs`;
  - the `mem::take`/`append`/swap lines of SPEC-1a's loops;
  - `levels.rs`' level swaps;
  - `game_tests.rs`, `*_fixture.rs`, `command_harness.rs`.

  Each allowlist entry has a file:function and a reason.
- **Rust, `game_tests.rs`:**
  - `a_refused_join_never_reaches_the_level`: `test_life` refuses the next join, then `/summon pig`; no pig, and nothing paired by the hub.
  - `joins_come_before_the_entity_is_in_the_level`: in `test_life` on join, `self.mobs` lacks the id.
  - `leave_comes_after_removal_with_its_reason`: on leave, the id is absent (or `removed`). Reasons: a pig killed → `Killed` when its body goes (tick 20); a despawned zombie → `Discarded`; an item picked up → `Discarded`; a chunk unloaded → `UnloadedToChunk`.
  - `loaded_entities_join_from_disk`: save and reload a chunk with a cow; its join has `FromDisk`; `/summon` has `Fresh`.
  - `a_pickup_leaves_before_the_stat`: the leave hook of an item picked up runs before the pickup's stat award. Read the order from a log the test hook and a test-only stat observer append to.
- **Harnesses:** `python3 tools/parity.py mob_ai server_entities server_player_damage dedicated_server` unchanged; `python3 tools/lan_e2e.py --check chest`.
- `cargo clippy -p horizon-client -- -D warnings` (and with `--features java-mods`); `cargo test -p horizon-client --lib --bins game_tests`.

## How the parent tests it
- The site test passes; there is no direct vector mutation left.
- The harnesses pass unchanged.
- No user steps: behaviour is unchanged until SPEC-1d gives the hooks bodies.
