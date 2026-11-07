# SPEC-7-1: a mod mob's goals, listed at join, vanilla ones made native

Design: `DESIGN.md` §8 ("Goals: vanilla goals become their native goal, built from the goal object's fields; mod goals run in Java and are driven by native's selector"). Index: `SPEC-2-8.md`. Size: ~280 lines. Needs phase 6.

## Why
A mod mob's `registerGoals()` runs in its Java constructor, which builds the view (`EntityType.create` at join). Its `goalSelector` and `targetSelector` then hold `WrappedGoal`s: vanilla goal classes (`FloatGoal`, `MeleeAttackGoal`, `NearestAttackableTargetGoal`…) configured by constructor arguments, and the mod's own goal classes. Native's `GoalSelector` (`mobs/goals.rs:80`, trait `Goal`) ticks native goals.

## Change
- **Export at join** (op `entity_view`'s reply, for a mob view whose class isn't vanilla): `int n` × (`byte selector` 0 goals / 1 targets, `int priority`, `text goalClass`, and either the vanilla goal's fields or `int javaGoal` (an index into the view's goal list)).
- **Vanilla goal → native goal:** a table in Java (`B/GoalExport.java`) for each vanilla goal class native ports (`goals.rs` structs: `Float`, `Panic`, `Stroll`, `LookAtPlayer`, `RandomLookAround`, `MeleeAttack`, `LeapAtTarget`, `NearestTarget`, `HurtByTarget`, `AvoidEntity`, `Tempt`, `Breed`, `FollowParent`, `FollowMob`, `RestrictSun`, `FleeSun`, `EatBlock`, `BreathAir`, `TryFindWater`, and the rest `grep -n "^pub struct" goals.rs` lists): the constructor arguments vanilla stores in fields, read by reflection with field names from the vanilla source (`speedModifier`, `lookDistance`, `targetType`, `mustSee`…), encoded per class. The Rust side builds the matching native goal with the same fields. A field holding a predicate or a mod lambda (`NearestAttackableTargetGoal`'s `selector`) makes that goal a Java goal instead (its predicate is mod code).
- A **subclass of a vanilla goal** defined by the mod (overrides `canUse`…) is a Java goal.
- A vanilla goal class native doesn't port: a Java goal too, and the report lists it (W17 evidence for a port).
- The mob's native `goals`/`targets` selectors are filled in the exported order, with the same priorities. `Mob.registerGoals` doesn't run natively for mod types.
- Goals added or removed later by mod code (`goalSelector.addGoal`/`removeGoal`, e.g. on taming): a bridge on `GoalSelector.addGoal(ILnet/minecraft/world/entity/ai/goal/Goal;)V` and `removeGoal` with a backed-owner guard (the selector's mob: add an accessor `GoalSelector` → owner set at export) re-exports that one goal; `removeAllGoals` likewise.

## Tests
- `cargo test -p horizon-client --features java-mods --bins java_goals`: each exported vanilla goal's bytes build the same native goal as native's own `register_goals` for a vanilla mob with the same arguments (compare to a zombie's `MeleeAttack(1.0, false)`).
- Test mod `TestMob.registerGoals`: `FloatGoal`, `MeleeAttackGoal(this, 1.0, false)`, `WaterAvoidingRandomStrollGoal(this, 0.8)`, `LookAtPlayerGoal(this, Player.class, 8)`, a mod `FollowNearestPlayerGoal`, and `NearestAttackableTargetGoal(this, Pig.class, true)`. Check `goals_exported`: the debug log lists 5 native and 1 Java goal with their priorities.
