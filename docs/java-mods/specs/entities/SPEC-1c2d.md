# SPEC-1c2d: `Entity.move` for every kind, projectile owners as any entity

Design: `DESIGN.md` §0 decisions 1 and 4. User decision (2026-11-04): `Entity.move` wired for every kind now; projectile owners widened to any entity, guests included.

**Order:** after `SPEC-1c2c-11` (the steps in `SPEC-1c2c.md`), before `SPEC-1c2c-12` (12b's `setOwner` and `move` bridges call what this spec adds). §2 (owners) depends on no riding step; §1 needs step 9.

## 1. `Entity.move` for every kind
**Native has the port:** `horizon_world::entity_move::move_entity<W: EntityMoveWorld>` (`entity_move.rs:123`), vanilla `Entity.move`'s order: no-physics, piston limit, stuck multiplier, `maybeBackOffFromEdge`, collide, history, collision flags, `setOnGroundWithMovement`, fall-reset clip, `checkFallDamage`, restitution, movement emission, block speed factor. Its harness is `entity_move`. Only the player implements the 40-method `EntityMoveWorld` (`motion.rs:2055`). 1c2b §1.7 serves TNT, mobs, carts, items, frames and paintings through their own ports. This spec adds every other kind.

### 1.1 The state vanilla's `Entity` keeps for `move`
New `horizon_world::entity_move::MoveState`:
```rust
#[derive(Clone, Debug, Default)]
pub struct MoveState {
    pub fall_distance: f64,
    pub stuck: Vec3,                 // stuckSpeedMultiplier
    pub collision: CollisionFlags,   // horizontalCollision, verticalCollision, verticalCollisionBelow, minorHorizontalCollision
    pub history: MovementHistory,
    pub on_ground: bool,
}
```
- Embed `pub moving: MoveState` in `ExperienceOrb`, `Projectile`, `boats::Boat`, `ArmorStand`, `dragon::Crystal`, `Cushion`, `lightning::Bolt`, `commands::OtherEntity`.
- Where the kind already has `on_ground` or `fall_distance`, move those fields into `moving` (the compiler lists the readers).
- Save and load `fall_distance` and `OnGround` as the base tag already names them (`entity_nbt.rs:71–72`).

### 1.2 `game/entity_move.rs` (new): one adapter for those kinds
`struct KindMoveWorld<'g, 'w> { game: &'g mut Game<'w>, target: Target, body: BodyRef }` implements `EntityMoveWorld`. `BodyRef` gives `&mut` access to the entity's position, velocity, `moving`, box size (`entity-types.json` width and height) and `removed`.

Take the entity out of its vector by index around the call (`with_entity_outside`, modelled on `entity_life.rs` `with_mob_outside`), so `collide`'s world never sees the entity itself.

| Trait method | Source (each is native's existing port, or vanilla's default for the kind) |
|---|---|
| `no_physics` | the kind's vanilla `noPhysics` field: true for the dragon; for each other class, the value its constructor sets (`grep -n noPhysics` in its vanilla source), else false |
| `position`, `set_position` | the struct (`setPos`: position + box) |
| `record_movement`, `movement_history` | `moving.history` |
| `collision_flags`, `set_collision_flags` | `moving.collision` |
| `limit_piston_movement` | `piston_push.rs` `pushed_movement` (`limitPistonMovement`) |
| `stuck_multiplier`, `set_stuck_multiplier` | `moving.stuck` |
| `velocity`, `set_velocity` | the struct |
| `back_off_edge` | vanilla `maybeBackOffFromEdge` is a no-op for non-players: return the movement |
| `collide` | `horizon_world::world_movement::resolve_world_movement` with the kind's step height (0 except boats' vanilla value), as `SceneCollisions::resolve` drives it for mobs |
| `fall_distance`, `reset_fall_distance`, `fall_reset_clip` | `moving.fall_distance`; the clip as `motion.rs`'s impl does it |
| `locally_authoritative`, `client_side`, `can_simulate` | server: `false`, `self.remote.is_some()`, `true` |
| `set_on_ground_with_movement`, `on_ground`, `minor_horizontal_collision` | `moving` (vanilla bodies, copied from `motion.rs`'s impl) |
| `legacy_on_position`, `block_state` | the level |
| `check_fall_damage` | vanilla `Entity.checkFallDamage`: block `fallOn` (`horizon_world` `level` hooks), fall distance bookkeeping. Kind overrides: `AbstractBoat.checkFallDamage` (breaks into planks above 3 blocks: native `boats.rs` if ported, else port it); `ArmorStand` is living (`LivingEntity.checkFallDamage` particles, `causeFallDamage`: armor stands take none, vanilla `ArmorStand.causeFallDamage`) |
| `restitute` | `horizon_world::movement_restitution::restitute_movement` (slime and bed bounce) |
| `movement_emission`, `apply_movement_emission` | `getMovementEmission` per class (`rea_vanilla.py method <Class> getMovementEmission`: projectiles, TNT, orbs, bolts override it); `horizon_world::movement_emission::apply_movement_emission` |
| `passenger` | `self.is_passenger(Ent::Id(id))`, the server table (`SPEC-1c2c-6`) |
| `removed` | the struct's `removed` |
| `block_speed_factor` | the block-speed-factor query `motion.rs` uses |

**Kind overrides of `move` itself** (vanilla `grep -rln "public void move(final MoverType"`):
- **`AbstractArrow.move`:** `super.move`, then `if (moverType != SELF && shouldFall()) startFalling()`. Port it after the generic call for arrow projectiles, with native's `arrows.rs` stuck state.
- **`AbstractMinecart`, `BlockAttachedEntity`/`ItemFrame`, `Shulker`:** already in 1c2b §1.7 (carts, frames) and mobs.

`Game::entity_move` (1c2b §1.7) has the arms `Orb | Projectile | Boat | Stand | Crystal | Cushion | Bolt | Other => KindMoveWorld`, so every kind has one.

**Not changed:** each kind's own tick keeps its own move code. Moving the tick loops of orbs, boats and projectiles onto `KindMoveWorld` is a parity-affecting change for another spec; the bridge and the ticks then share one vanilla method through two ports. That is listed for the parent.

### 1.3 Tests
- `cargo test -p horizon-client --bins entity_move`:
  - `an_orb_moved_into_a_wall_stops_and_flags_horizontal_collision`;
  - `a_boat_moved_down_onto_slime_bounces` (restitution);
  - `an_arrow_moved_by_a_piston_starts_falling`;
  - `the_dragon_moves_through_blocks` (no physics).
- Harnesses: `python3 tools/parity.py entity_move movement_emission movement_support world_movement piston_movement arrow`.
- Test mod: `move_every_kind` moves one each of orb, armor stand, boat, snowball, end crystal by `(1, 0, 0)` on a floor, and reads back `getX()` +1 and `onGround()`. A boat pushed into a wall reads back `horizontalCollision`.

## 2. Projectile owners: any entity, guests included
**Native today** (`projectiles.rs:110`):
- `owner: Option<Who>`, where `Who` is `Player` (the acting player) or `Mob(id)`;
- plus `guest: Option<u32>` meaning "`Who::Player` is that guest".

Guest owners are representable now, through the pair. The `pvp` swap at `creatures.rs:3143–3180` and `guest_projectile_hit` run hits as the guest, so guest kill credit already works for arrows. What can't be named:
- an owner of any other kind (an armor stand, a TNT, a boat), which vanilla allows for any `Entity`;
- an owner that isn't loaded. Vanilla keeps the `EntityReference` UUID, so after a relog the arrow is still yours (pickup, kill credit).

### 2.1 `Owner` (in `projectiles.rs`)
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Player { guest: Option<u32> },  // the host (None) or a guest by connection
    Entity(u32),                    // any non-player entity of the level, by id
    Unloaded([i32; 4]),             // EntityReference with no entity found yet: its UUID
}
impl Owner {
    /// The `Who` the hit and damage code takes: the player whose turn it is, or a mob.
    pub fn who(self, acting: Option<u32>, is_mob: impl Fn(u32) -> bool) -> Option<Who>
}
```
- `Projectile.owner: Option<Owner>`; delete `Projectile.guest`.
- `who()`: `Player { guest }` → `Who::Player` when `guest == acting`, else `None`. `Entity(id)` → `Who::Mob(id)` if a mob, else `None`, since the hit code only targets players and mobs.
- Every `p.owner == Some(Who::…)` and every `p.guest` use (34 lines across 30 files) goes through `owner`. The compiler lists them.
- **Shot as a guest:** a site that wrote `Some(Who::Player)` during a guest's turn (`as_guest`) now writes `Owner::Player { guest: self.lan_acting }`. The `pvp` swap that set `p.owner = Some(Who::Mob(guest entity))` around the tick goes through `who()` with the guest's entity id. Drop the restore line (`creatures.rs:3180`).
- **Kill credit** (`player_hurt_mob`, `creatures.rs:3330`): `Owner::Player { guest }` → `last_hurt_by_player.insert(id, guest)`, not `self.lan_acting`. It then credits the right player even when the hit runs outside the shooter's turn: an arrow landing after the guest's turn ended, or a guest's fireball deflected.
- `ArrowState` pickup (`AbstractArrow.tryPickup`: owner check) and `arrows.rs:960` (`Mob` owner and mob griefing) read `who()` / `Owner::Entity` of a mob.

### 2.2 Resolving and saving
- **`Owner::Unloaded(uuid)`:** vanilla `EntityReference.getEntity` looks the UUID up each time it is asked. Add `fn resolve_owner(&self, o: Owner) -> Owner`:
  - `Unloaded(u)` → `Player { guest }` when it is the host's or a connected guest's UUID (`player_with_uuid`);
  - `Entity(id)` when `persist.uuids` maps an id to `u` in the level;
  - else unchanged.

  Call it at each read site, where vanilla calls `getOwner()`: the hit, pickup, kill credit, deflect (`owner_look`).
- **Save:** `Owner` → `Owner` UUID (`uuid_for(id)` for `Entity`, the player's UUID, or the stored one). The four `arrow_to_nbt` / `fireball_to_nbt` sites in `entity_nbt.rs` (496, 505, 562, 691) take `Option<[i32; 4]>` from `owner_uuid(o)`.
- **Load:** `Owner::Unloaded(uuid)`; the first `resolve_owner` turns it live.

### 2.3 The `setOwner` bridge (1c2b §1.3, final form)
Vanilla stores only the owner's UUID (`EntityReference.of(owner)`), so the bridge passes exactly that.
- Add `Flat.raw(Entity)`'s sibling for this one entry: the descriptor's `Entity` argument is flattened by a new `Flat.rawUuid(Entity) -> String` (`""` for null). In Rust it is a new `Arg` type `EntUuid(Option<[i32; 4]>)`, `RAW "Ljava/lang/String;"`, whose `Flat` name `BridgeProcessor` picks from the `Arg`'s declared Java method. If `BridgeProcessor` only knows `raw`, give the `Arg` a `FLAT: &str = "rawUuid"` constant and use it there.
- Body: `None` for empty, else `resolve_owner(Owner::Unloaded(uuid))`. This yields `Player { guest }`, `Entity(id)`, or stays `Unloaded` for a Java-only object, the same cases vanilla's reference covers.

The `Projectile` species bytes (1c2b §1.3) send `bool hasOwner`, then 2 `long` UUID. Java `apply` sets `owner = EntityReference.of(uuid)` raw, which is exactly vanilla's field.

### 2.4 Tests
- `cargo test -p horizon-client --lib projectiles`:
  - `an_owner_round_trips_through_save_as_its_uuid`;
  - `a_guests_arrow_credits_the_guest_after_its_turn` (owner `Player { guest: Some(2) }`, hit run with `lan_acting = None` → `last_hurt_by_player[id] == Some(2)`);
  - `an_unloaded_owner_resolves_when_it_loads`.
- `python3 tools/parity.py arrow trident server_player_damage dedicated_server`; `python3 tools/lan_e2e.py --list` and run each check involving bows or arrows.
- Test mod: `projectile_owner_any_entity` (`snowball.setOwner(armorStand)` → read back `getOwner() == armorStand`); `projectile_owner_survives_reload` (`afterChange`: the arrow's `getOwner()` is the player).

## Scenario (parent)
**`tools/scenarios/owners.json`:**
1. Floor, `tp`, `near`.
2. `/summon arrow ~ ~2 ~ {Owner:[I;1,2,3,4]}`, then `/data get entity @e[type=arrow,limit=1] Owner` returns `[I;1,2,3,4]` after a save and reload.

**User steps (LAN, two players):**
1. The guest shoots a zombie with a bow from far away and walks off before the arrow lands.
   - Expected: the kill counts for the guest (statistics, `player_killed_entity` advancement), not the host.
2. The guest relogs, and picks up an arrow they shot before.
   - Expected: it is pickable as theirs (survival).
