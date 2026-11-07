# SPEC-1c2b: the writers that need native logic, cross-level teleport, riding

> Order superseded: §1.1, §1.2, §1.4–§1.6 are done (454aa7b3a). §1.3, §1.7, §2 and §3 are implemented as `SPEC-1c2c-12.md` parts 12a–12c, after `SPEC-1c2c-1…11` and `SPEC-1c2d`. Where this file says `SPEC-1c2c.md`, read the step files.

Design: `DESIGN.md` §2, §0 decision 1. Base: SPEC-1c, plus `SPEC-1c2a.md`, `SPEC-1c2c.md` (passengers) and `SPEC-1c2d.md` (`move` for every kind, owners). Implement in the order 1c2a, 1c2c, 1c2d, 1c2b. From 1c2a this uses:
- the `species` group (bit 13);
- `Source` (the flattened `DamageSource`);
- `Target::Tnt`;
- the null-safe `Flat.raw(Entity)` below.

Closes `Entity.move`, `LivingEntity.actuallyHurt`, `LivingEntity.releaseUsingItem`, `Mob.setPathfindingMalus`, `Projectile.setOwner`, `Projectile.shootFromRotation`, `AbstractArrow.setBaseDamage`, cross-level `Entity.teleport`, and riding beyond mob-on-mob.

Who calls them (bytecode, `/tmp/hz-poc/agents/spec1c2/callers.py`):
- `move`: `TinyTNTPrimedEntity.tick`, `EntityBalloon.tick`.
- `actuallyHurt`: `EntityRobit.actuallyHurt` calls `super`.
- `releaseUsingItem`: `ItemFlamethrower.onUseTickNoChemical`, on its user (a player).
- `setPathfindingMalus`: `EntityRobit.<init>`, `RobitAIBase.start/stop`.
- `setOwner`, `shootFromRotation`: `EntityFlame.create`, before its spawn.
- `setBaseDamage`: Mekanism's baby skeletons' `getArrow`, before the spawn.
- `teleport`: `SpatialStorageHelper.teleportEntity` (ae2), with its own `PostTeleportTransition` lambda that re-mounts riders; `TileEntityTeleporter.teleportEntityTo` (mekanism, `DO_NOTHING`).
- `startRiding`/`stopRiding`: AE2's spatial IO, on any entity, players included.

Unspawned receivers run the Java body on a plain Java object, and the result travels in the saved form at `addFreshEntity`. That is 1c's spawn and is correct: nothing native exists yet. The bridges below serve **backed** views.

Common rules for every entry:
- guard `@dev/horizonlab/javamods/boot/EntityViews.backed`, receiver `id: Ent`;
- an absent native entity means do nothing and return vanilla's answer for a removed entity;
- end with `after(game, id, GROUPS)`.

`Flat.raw(Entity)` must return 0 for `null` (`setOwner(null)`, `startRiding` guards); fix it in `Flat.java` line 34.

## 1. Writers

### 1.1 `Mob.setPathfindingMalus(PathType, F)V`
- Vanilla `Mob.java:191`: `pathfindingMalus.put(type, cost)`.
- Native: `Mob::set_malus` (`mobs/ai.rs`; `PathType` in `mobs/path.rs` is in vanilla's declaration order, checked).
- Bridge: `Flat.raw(PathType) -> int` (ordinal); `PathType::from_ordinal(i)` (new, a `[PathType; 27]` table); `m.set_malus(t, cost)`.
- Refresh `AI` (bit 15, 32768), a new wire group: `int n` × (`byte ordinal`, `float malus`) over the set entries. Java `apply` writes `MobAccess.pathfindingMalus()` raw (clear, then put).
- Mobs only; another kind is absent.

### 1.2 `AbstractArrow.setBaseDamage(D)V`
- Vanilla `AbstractArrow.java:673`.
- Native: `ProjectileKind::Arrow { base_damage, .. }` (`projectiles.rs:27`), set in place. A trident (`ProjectileKind::Trident`): follow vanilla `ThrownTrident`; if it reads `baseDamage`, add the field, else store nothing.
- Refresh `SPECIES`. Extend 1c2a's species shapes with `Arrow`: `double baseDamage`, then the `Projectile` bytes (§1.3). Java `apply` uses an `AbstractArrowAccess.baseDamage` accessor.

### 1.3 `Projectile.setOwner(Entity)V`
- Vanilla `Projectile.java:83`: `owner = EntityReference.of(owner)`.
- Native: `Projectile.owner: Option<Owner>`, from SPEC-1c2d §2: any entity, guests and unloaded owners included.
- The bridge entry, its argument (`EntUuid`) and its body are SPEC-1c2d §2.3.
- Refresh `SPECIES` (the `Projectile` bytes: `bool hasOwner`, 2 `long` UUID).

### 1.4 `Projectile.shootFromRotation(Entity, FFFFF)V`
- Vanilla `Projectile.java:183`. Native: `Projectile::shoot_from_rotation` (`projectiles.rs:385`), which takes the source's known movement and `onGround`.
- Bridge `(id: Ent, source: Ent, x_rot, y_rot, y_offset, pow, uncertainty)`:
  - `(movement, on_ground)` of `source`:
    - the acting player: `known_player_movement()` and its `ground.on_ground` (the `creatures.rs:5180` pair);
    - a guest: its `known_movement` (`lan_actions.rs:194`);
    - a mob: `velocity` and `on_ground` (vanilla `getKnownMovement` is `getDeltaMovement` for non-players);
  - then `p.shoot_from_rotation(&mut self.world.random, …)`, the random native's own throw sites pass; `p.needs_sync = true`.
- Refresh `MOTION | POSITION` (rotation lives in position).

### 1.5 `LivingEntity.releaseUsingItem()V`
Vanilla `LivingEntity.java:3604`:
1. if the use item matches the hand's (`isSameItem`), `useItem.releaseUsing(level, this, remaining)`;
2. if `useOnRelease`, `updatingUsingItem`;
3. `stopUsingItem`.

Its receiver is the flamethrower's user, a player. Two entries:
- **`if "net/minecraft/server/level/ServerPlayer"`** (`player.rs`), on the acting player, ported from vanilla with native's pieces:
  - `let Some((slot, ticks)) = self.using`, plus the stack in that slot;
  - same item → `self.release_using(&stack, remaining, false)` (`game.rs:5826`: it reaches Java's `Item.releaseUsing` only for a mod item, through `java_hook_release_using`, a nested call);
  - `useOnRelease` (the item's `use_effects`/crossbow rule native already reads in `tick_used_item`) → `tick_used_item` once;
  - `stopUsingItem`: `self.using = None`, then the game event `minecraft:item_interact_finish` when it was using (the `causeUseVibration` native already sends at a use's end).
  - Then `java_refresh_player()`. The player refill must also write the view's use state raw: `useItem = EMPTY`, `useItemRemaining = 0`, living flag 1 cleared (`LivingEntityAccess`). Otherwise `java_tick_using`'s reply (`java_mods.rs:76`) reads `using` true and resumes it.
- **Backed (mobs):** native mob item use (`mobs/spear_use.rs` `stop_using_item`, and whatever `port_ledger.py where LivingEntity.startUsingItem` lists for mobs) releases in the same three steps. Refresh `EQUIPMENT | FLAGS`.

### 1.6 `LivingEntity.actuallyHurt(ServerLevel, DamageSource, F)V`
Vanilla `LivingEntity.java:1959`:
1. `!isInvulnerableTo` →
2. armor absorb, magic absorb;
3. absorption amount;
4. `DAMAGE_DEALT_ABSORBED` stat for a player source;
5. `recordDamage`, `setHealth(health - dmg)`, absorption again, `gameEvent(ENTITY_DAMAGE)`.

Overrides native already ports inline: `Wolf` (armor), `Animal` (`resetLove`), `Armadillo`.

**Native today:** `Mob::damage_with` (`mobs.rs:2426`) fuses `hurtServer` and `actuallyHurt`. It applies armor **before** the cooldown subtraction, where vanilla (`LivingEntity.java:1220–1231`) does `actuallyHurt(damage - lastHurt)` on the pre-armor amount and stores `lastHurt = damage` pre-armor. Fix that while splitting:
- New `pub fn actually_hurt(&mut self, source: Damage, dmg: f32) -> f32` (the health it took), with, in this order:
  - the invulnerability test (`invulnerable_flag` and `bypasses_invulnerability`, as now);
  - Armadillo's reduction (if vanilla has it in `actuallyHurt`; else it stays in `hurtServer`);
  - armor (`CombatRules.getDamageAfterAbsorb`, the `armor_effectiveness` hook passed through);
  - protection, Witch's magic rule;
  - absorption (add `Mob.absorption: f32` if absent; the 1c health group already reads `AbsorptionAmount`);
  - Wolf armor;
  - `health -=`; `record_damage`; Animal's `in_love = 0`;
  - the `minecraft:entity_damage` game event through `MobEvent` (what `sculk.rs:576` `mob_damage_event` emits today).
- `damage_with` keeps the `hurtServer` part:
  - immunities, freeze ×5, helmet ×0.75, crackiness;
  - the cooldown branch: `if invulnerable > 10 { if amount <= last_hurt { return false }; actually_hurt(amount - last_hurt); last_hurt = amount; knock = false } else { last_hurt = amount; invulnerable = 20; actually_hurt(amount); hurt_time = 10 }`;
  - then the rest as now.
- The bridge: `java_target` → mob → `m.actually_hurt(damage_of(source), dmg)`, `damage_of` from 1c2a's `Source`. Refresh `HEALTH`.

This changes native damage inside the cooldown window toward vanilla. Run the harnesses in Tests.

### 1.7 `Entity.move(MoverType, Vec3)V`
- Vanilla `Entity.java:731`. Native has one generic port, `horizon_world::entity_move::move_entity<W: EntityMoveWorld>`, implemented only for the player (`motion.rs:2055`), plus per-kind ports.
- Bridge `(id: Ent, mover: i32, delta: Point)`, with `Flat.raw(MoverType)` as the ordinal (`SELF, PLAYER, PISTON, SHULKER_BOX, SHULKER`, the order of `entity_move::MoverType`).
- New `Game::entity_move(t, mover, delta) -> bool` dispatches to the kind's existing port:

| Kind | Native port |
|---|---|
| `Tnt` | new `PrimedTnt::move_by(world, collision, delta)`, factored out of `PrimedTnt::tick` (the `resolve`, `on_ground` and per-axis zeroing); `tick` calls it with `self.velocity` |
| `Mob` | new `Mob::do_move_by(w, delta, events)`, factored out of `do_move` (`mobs/living.rs:537`, `wanted = delta`); `do_move` calls it with `self.velocity`. `MobWorld` built as `creatures.rs:908` does; events go through `handle_mob_events` |
| `Cart` | `move_cart(&mut c, delta)` (`minecarts.rs:379`) |
| `Item` | new `ItemEntity::move_by`, factored from `ItemEntity::tick` (`entities.rs:219–312`) |
| `Frame`, `Painting` | vanilla `BlockAttachedEntity.move`: `!removed && delta.lengthSqr() > 0` → `kill` + `dropItem(null)`: native's break of that frame or painting with no breaker |

- Refresh `POSITION | MOTION | FLAGS`.
- Every other kind (orbs, projectiles, boats, stands, crystals, cushions, bolts, `Other`) goes through SPEC-1c2d §1's `KindMoveWorld`, the generic `horizon_world::entity_move::move_entity`. No kind is left out.

## 2. Cross-level `Entity.teleport`
`java_teleport` (`java_entities.rs:941`) warns and does nothing for another dimension. Native has no cross-level teleport for non-players at all: `/tp` of a mob into another dimension isn't ported either (`commands.rs` `teleport_across` is players only). Port vanilla's `teleport` / `teleportSameDimension` / `teleportCrossDimension` (`Entity.java:3149–3215`) once, and use it from the bridge and from `/tp`.

**`horizon-core`, new `teleport.rs`:**
- `PositionMoveRotation { pos, delta, yaw, pitch }`;
- `calculate_absolute(current, change, relatives: u16) -> PositionMoveRotation`, ported from `PositionMoveRotation.calculateAbsolute` (`net/minecraft/world/entity/PositionMoveRotation.java`), `Relative` bits in vanilla's ordinal order;
- unit tests on each relative bit.
- `nether_portal.rs:132`'s own copy calls it.

**New `game/entity_travel.rs`:**
```rust
pub(super) struct Teleport { pub dim: Dim, pub change: PositionMoveRotation, pub relatives: u16, pub as_passenger: bool, pub post: Post }
pub(super) enum Post { Nothing, PortalSound, PortalTicket, Java }
impl Game<'_> {
    /// `Entity.teleport`: the entity after it, `None` when it is removed or can't travel.
    pub(super) fn teleport_entity(&mut self, t: Target, to: &Teleport) -> Option<Target>
}
```
1. Not in the level, or removed → `None`. If `!as_passenger`, `stop_riding(t)` (§3).
2. **Same level:**
   - for each passenger (`passengers_of(t)`), `teleport_entity(p, passenger_transition(to, p, t))`. `calculatePassengerTransition` is ported beside it: offset from the vehicle, rotation deltas unless relative, `as_passenger = true`.
   - Then `calculate_absolute(pose(t), to.change, relatives)` is written with `java_set_position` + `java_set_motion` (position, previous position, rotation, motion).
   - Unless `as_passenger`, mark it for an immediate teleport sync the way native `/tp` of a mob does (`teleport_to_in_level`).
   - Then `post`. Return `Some(t)`.
3. **Another level:**
   - `let old = passengers_of(t)`, eject them (`stop_riding` each, last first, as `ejectPassengers`);
   - `teleport_entity` each with its passenger transition, collecting the new targets;
   - `let tag = entity_nbt(t)` (one entity, no passengers), with `Pos`, `Motion`, `Rotation` set to `calculate_absolute(pose(t), to.change, relatives)`;
   - `level_remove(kind, id, ChangedDimension)`, `forget_entity(id)` (vanilla `removeAfterChangingDimensions`);
   - `let saved_lent = std::mem::take(&mut self.lent)`;
   - `let back = self.enter_command_level(to.dim)`;
   - `let new = self.spawn_entity_from_nbt(tag)` (Fresh: vanilla `addDuringTeleport`, a new id);
   - for each new passenger, `start_riding(p, new, force = true, events = false)`;
   - `if let Some(b) = back { self.enter_command_level(b) }`; `self.lent = saved_lent`; `self.note_touched(None)`.

     `enter_command_level` swaps whole levels, so the lent vectors ride along and come back. The lent marks must not apply to the target level's vectors, which is why `lent` is put aside.
   - Then `post`. Return `new`.
4. **`post`:**
   - `PortalSound`: level event 1032 to the entity only when it is a player (vanilla `PLAY_PORTAL_SOUND`); for others nothing.
   - `PortalTicket`: the portal ticket native keeps for arrivals (`nether_portal.rs` `arrival_chunks`) at the entity's chunk.
   - `Java`: nothing here (below).

**Bridge** (`entity_teleport`, `item_teleport` in `entities.rs`; the player entries stay as they are):
- `Flat.raw(TeleportTransition)` appends `int relatives` (bits by `Relative.ordinal()`), `bool asPassenger`, `byte post`: 0 `DO_NOTHING`, 1 `PLAY_PORTAL_SOUND`, 2 `PLACE_PORTAL_TICKET` by identity, 3 anything else.
- On 3 it keeps the callback in `EntityViews.PENDING_POST` (thread-local).
- The body calls `teleport_entity` and returns the new id (0 for `None`).
- `Flat.toEntity(int id)` runs after the native returns. It resolves the view in any level (`EntityViews.byId` over all of `LIVE`: ids are game-wide, `next_entity` isn't per level). If `PENDING_POST` is set, it clears it and calls `post.onTransition(view)`. This runs AE2's lambda, which re-mounts through the bridged `startRiding`, on the new entity, as vanilla does right after the add.
- `/tp` of a non-player into another level (`cmd_teleport`) calls `teleport_entity` too, in place of keeping it in its level.

## 3. Riding
`java_ride` / `java_dismount` (`java_entities.rs:887`, 898) handle only mob on mob, without vanilla's checks. Delete them.

`start_riding(r, v, force, events)` and `stop_riding(r)` are SPEC-1c2c's passenger model:
- every kind as rider and as vehicle, players and guests included;
- vanilla's seven `startRiding` steps;
- `addPassenger`'s player-first rule;
- `boardingCooldown`, per-kind `canAddPassenger`, positions from exported attachments, dismount locations;
- saving `Passengers` for any kind;
- `SetPassengers` tracking;
- `/ride`.

This section only wires the bridges to it, with `ent_of(java_target(id))` for backed views and the player's `Ent` for player entries.

**Bridges:**
- `entity_start_riding`, `entity_stop_riding`, `living_stop_riding` call `start_riding` / `stop_riding`, with `force` and `events` from the arguments. Refresh both entities' `RIDING | POSITION`.
- New player entries with `if "net/minecraft/server/level/ServerPlayer"` on `Entity.startRiding(Entity, ZZ)Z` and `stopRiding()V`. Check with `javap -p` of the patched server jar whether `ServerPlayer` or `Player` declares `stopRiding`; bridge each declaration.
  - Their receiver `Ent` is the player's network id. Map it to `Target::Player` or `Target::Guest(conn)` with the inverse of `network_entity_id`, then call the same functions, followed by `java_refresh_player()`.
  - Verify in `GameBridge.fill` that a player view's `getId()` is the hub's entity id. If it isn't, set it there.

## Test mod (`ChecksEntities`)
Read-backs go through `((ServerLevel) level).getEntity(id)` (1c refresh). Spawn what is needed in `run`; check in `later` where native must have ticked.
- `malus_writes_through`: a zombie; `setPathfindingMalus(PathType.WATER, 0F)` → read back `getPathfindingMalus(WATER) == 0F`.
- `arrow_base_damage_writes_through`: a spawned `Arrow`; `setBaseDamage(7.0)` → read back `getBaseDamage() == 7.0`.
- `projectile_owner_writes_through`: a spawned snowball; `setOwner(chicken)` → read back `getOwner() == chicken`.
- `shoot_from_rotation_writes_through`: a spawned snowball; `shootFromRotation(chicken, 0, 90, 0, 1.5F, 0)` → read back `getDeltaMovement().x < -1.4`, with yaw 90 aiming −x.
- `actually_hurt_writes_through`: `actuallyHurt` is protected, so the test mod calls it through an `@Invoker` mixin (`LivingEntityInvoker.callActuallyHurt`) on a full-health zombie with `generic()` and 4 → read back `health == max - 4` and `hurtTime == 0` (`actuallyHurt` sets no hurt time).
- `release_using_writes_through`: the test wand gets a held use (`ItemUtils.startUsingInstantly` in its `use`); on the next `use_tick`, the wand calls `player.releaseUsingItem()` → `!player.isUsingItem()` there and on the following tick (native didn't resume it).
- `move_writes_through`: a spawned `PrimedTnt` on the floor; `tnt.move(MoverType.SELF, new Vec3(1, 0, 0))` → read back `getX()` 1 more. And a zombie moved into a wall stops at it (`horizontalCollision`).
- `teleport_across_levels`: a chicken; `chicken.teleport(new TeleportTransition(nether, pos, Vec3.ZERO, 0, 0, Relative.NONE, TeleportTransition.DO_NOTHING))` → result `r != null && r != chicken && r.level() == nether && chicken.isRemoved()`, and `nether.getEntity(r.getId()) == r`.
- `teleport_runs_mod_callback`: the same with a lambda setting a static flag → the flag is set, and the lambda got `r`.
- `ride_boat_and_back`: a boat and a pig; `pig.startRiding(boat, false, true)` → true; `pig.getVehicle() == boat` read back; `pig.stopRiding()` → `getVehicle() == null`.
- `player_rides_boat`: `player.startRiding(boat)` → true, and native's player is in the boat (`player.getVehicle() == boat` after `java_refresh_player`).

## Tests to run
- `cargo test -p horizon-core teleport` (`calculate_absolute`).
- `cargo test -p horizon-client --lib mobs` (new: `actually_hurt_in_the_cooldown_takes_the_pre_armor_difference`, against hand-computed vanilla numbers).
- `cargo test -p horizon-client --features java-mods --bins java_entities` (the `AI` and `Arrow` species groups round-trip).
- `cargo test -p horizon-client --lib --bins game_tests` (new: `a_mob_teleported_across_levels_leaves_with_changed_dimension_and_joins_there`; `passengers_travel_first_and_remount`).
- `python3 tools/java_mods.py build && HORIZON_NEOFORGE=26.3.0.101 python3 tools/java_mods.py check --analysis /tmp/hz-poc/agents/spec0/out/entity-facts.json` → `0 problems`.
- `python3 tools/java_mods.py bridges`.
- Harnesses: `python3 tools/parity.py living_damage damage_reduction damage_immunity mob_ai server_player_damage entity_move arrow trident item_use_lifecycle nether_portal server_entities dedicated_server`.

## Scenario (parent, `hz_scenario`)
`tools/scenarios/entity_travel.json`:
1. Floor at y=100 (forceload), `tp`, `near`.
2. `/summon pig ~2 ~1 ~ {Passengers:[{id:chicken}]}`, then `/execute in the_nether run tp @e[type=pig,limit=1] 0 100 0`.
3. `/execute in the_nether positioned 0 100 0 if entity @e[type=pig,distance=..3]` succeeds, and so does `/execute in the_nether as @e[type=chicken] if entity @s[nbt={}] on vehicle if entity @s[type=pig]` (the chicken rides the pig there).
4. In the overworld, `/execute unless entity @e[type=pig]` succeeds.

**User steps** (Mekanism + AE2 world):
1. Build an AE2 spatial IO with a 3×3 cell containing a pig on a boat. Store, then restore.
   - Expected: the pig comes back sitting in the boat, and nothing is left behind or duplicated.
2. Hold a fuelled Mekanism flamethrower, fire, and release.
   - Expected: the flame stops when released, and no flame keeps coming the next tick.
