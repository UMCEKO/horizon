# SPEC-4-9: mob griefing and visibility

Design: `DESIGN.md` §4 rows `EntityMobGriefingEvent`, `LivingVisibilityEvent`; Corrections (call Java at the site inside the mob AI loop). Index: `SPEC-2-8.md`. Size: ~220 lines. Needs 4-2.

## Vanilla, patched
- `EventHooks.canEntityGrief(level, entity)` replaces `getGameRules().get(MOB_GRIEFING)` at 24 sites (`callers.sh canEntityGrief`): `Allay.wantsToPickUp`, `CropBlock.entityInside`, `EatBlockGoal.tick`, the enderman leave/take goals, `Entity.canTrample`, `EvokerWololoSpellGoal.canUse`, `FoxEatBerriesGoal.onReachedTarget`, `HarvestFarmland.checkExtraStartConditions`, `LargeFireball.onHit`, `LivingEntity.createWitherRose`, `Mob.aiStep`, `Piglin.wantsToPickUp`, `Projectile.mayInteract`, `RaidGardenGoal.canUse`, `Ravager.aiStep`, `Ravager.roar`, `RemoveBlockGoal.canUse`, `ServerLevel.explode`, the silverfish goals, `SmallFireball.onHitBlock`, `SnowGolem.aiStep`, `TurtleEggBlock.canDestroyEgg`, `WitherBoss.customServerAiStep`.
- `CommonHooks.getEntityVisibilityMultiplier(entity, looker, vanilla)` at the end of `LivingEntity.getVisibilityPercent`.

## Change
- Native's mob-griefing reads: `grep -rn 'mob_griefing' crates/horizon-client/src` (14 `w.mob_griefing` in the lib, the rest in the bin). Map each to one row above (each native read must correspond to a vanilla site; a read with no vanilla counterpart is reported).
- Lib sites: `w.grief(entity)` → `w.java.map_or(w.mob_griefing, |j| j.ask(Site::Grief, …))` (4-2), only for types admitted for `EntityMobGriefingEvent` (subject = the entity passed to the hook: the mob, the projectile, the fireball's owner? check each caller's argument).
- Bin sites (explosions, crops, turtle eggs, `canTrample`): a normal crossing, op `mob_grief` → `EventHooks.canEntityGrief(level, view)`.
- Visibility: `mobs.rs:2125`, `goals.rs:712` (`TargetingConditions.test` → `getVisibilityPercent`): `Site::Visibility` with the vanilla multiplier and the looker; the reply replaces it.

## Test mod
`EntityMobGriefingEvent`: deny for creepers tagged `horizon_tame`, allow for endermen tagged `horizon_thief` while the rule is off. `LivingVisibilityEvent`: multiply by 0 for players holding the test wand (an invisibility cloak).
Checks: a tagged creeper exploding next to dirt with griefing on leaves the dirt; a tagged enderman with griefing off picks up a block within 400 ticks; zombies don't target a player holding the wand at 5 blocks.

## Tests to run
`java_mods.py build`, `check --analysis`, `bridges`; `python3 tools/parity.py mob_ai explosion enderman` unchanged.
