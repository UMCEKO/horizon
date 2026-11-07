# SPEC-7-3: navigation, controls, target and sensing bridges; mod mobs that think

Design: `DESIGN.md` §8 (`PathfinderMob` kind), §2 (writers bridged). Index: `SPEC-2-8.md`. Size: ~280 lines. Needs 7-2.

## Bridges (`G/java_bridges/entities.rs`, guard: the owning mob is backed)
The receiver of these is not the entity but its `PathNavigation`, `MoveControl`, `LookControl`, `JumpControl`, `Sensing`: add a `mob` back-reference accessor on each (`MobNavigationOwnerMixin` etc.) and a predicate guard `@EntityViews.backedOwner` that checks the owner's flag; the `Ent` argument is the owner.

| Vanilla | Native |
|---|---|
| `PathNavigation.moveTo(DDDD)Z`, `moveTo(Entity, D)Z`, `createPath(BlockPos, I)Path`, `moveTo(Path, D)Z`, `stop()V`, `isDone()Z`, `isInProgress()Z`, `getPath()`, `setSpeedModifier(D)V` | the mob's `nav` (`mobs/nav.rs`, `path.rs`); a `Path` crosses as its node list (`Flat.raw(Path)`, `toPath`) |
| `MoveControl.setWantedPosition(DDDD)V`, `strafe(FF)V`, `hasWanted()Z` | the mob's move control state |
| `LookControl.setLookAt(Entity, FF)V`, `setLookAt(DDD)V`, `setLookAt(Vec3)V` | look control |
| `JumpControl.jump()V` | jump flag |
| `Mob.setTarget(LivingEntity)V` (with 4-8's `LivingChangeTargetEvent` inside native's port), `getTarget()` (read: `riding`-like `target` group: add bit `target` with the target's id) | `m.target` |
| `Sensing.hasLineOfSight(Entity)Z` | the native sensing cache |
| `Mob.getRandom()` / `Entity.random` reads by mod goals | **the native mob's random**: a `RandomSource` view whose `nextInt`/`nextFloat`/`nextDouble`/`nextGaussian`/`nextBoolean` are bridged to the native mob's `LegacyRandom`, so draws happen in vanilla's order on one generator (`horizon_core::random`) |
| `PathfinderMob.getWalkTargetValue(BlockPos, LevelReader)F` overridden by a mod | an override route (6-3) at native's stroll target scoring |

Every one of these the analysis lists for the pack is required by `check --analysis` (they are writers or state reads of native-owned state; add their `GROUPS` rows).

## Kind
A `PathfinderMob`/`Mob` base mod type is a native `Mob` with species `Modded` (6-1's `Living` gains AI): its native tick runs vanilla `Mob` and `PathfinderMob` behaviour (physics, navigation, controls, despawn rules from its category) with its exported goals (7-1), and its overrides route (6-3). `customServerAiStep` overridden by the mod is an override route at its native site in `tick_after_selectors`.

## Tests
- Test mod: `TestMob` with the 7-1 goals attacks a pig (`NearestAttackableTargetGoal` + `MeleeAttackGoal`, native) and follows the player (Java goal). Checks `mod_mob_attacks_pig` (the pig is hurt within 200 ticks), `mod_goal_random_is_the_mobs` (a goal drawing `getRandom().nextInt(100)` twice gets the values the native mob's generator yields next: compare with a native debug read).
- `java_mods.py check --analysis` on the pack: no missing bridge for robit's goals' calls.

## User steps
Mekanism: place a robit, give it a chunk of food in its inventory: it follows you, picks up items, and stays on its chargepad as on Forge.
