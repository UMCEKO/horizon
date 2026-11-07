# SPEC-7-2: native's selector drives Java goals, with write-through

Design: `DESIGN.md` §8. Index: `SPEC-2-8.md`. Size: ~300 lines. Needs 7-1.

## The problem
`GoalSelector.tick` runs inside the lib's `Mob::tick` (`serverAiStep`), with `&MobWorld` borrowing `Game`: a Java goal's `start`/`tick` calls `getNavigation().moveTo(…)`, `setTarget`, `level.addFreshEntity`, which must write through at once (the navigation ticks right after the selectors in `Mob.serverAiStep`). 4-2's deferred writes would delay them past the body: wrong for goals.

## Change: the selector phase as its own part of the turn, for mobs with Java goals
- `Mob.serverAiStep`'s order (vanilla): `sensing.tick`, `targetSelector.tick`, `goalSelector.tick`, `navigation.tick`, `customServerAiStep`, `moveControl.tick`, `lookControl.tick`, `jumpControl.tick`.
- Split the lib tick into `tick_until_selectors` (everything before `targetSelector.tick`), `selectors` (both selectors) and `tick_after_selectors` (the rest), as 2-17 split around the air block. A mob with no Java goal runs the three in one call, as today.
- For a mob with a Java goal, `turn_mob` runs `selectors` itself with the mob put back in `Game` between goal calls: `GoalSelector::tick` gets a callback for Java goals, `&mut dyn FnMut(JavaCall) -> JavaAnswer`, which the bin implements by crossing (normal mode, `&mut Game`): native goals of the same selector get a fresh `Cx` after each crossing. The selector's own logic (priorities, flags, interruption, `requiresUpdateEveryTick`, the tick-delay halving) stays native, vanilla's port.
- **`JavaGoal` (trait `Goal` impl):** `can_use`, `can_continue_to_use`, `is_interruptable`, `start`, `stop`, `requires_update_every_tick`, `tick`, `flags` (read once at export and again after `setFlags` through a bridge). Each call: op `goal` (`int mob`, `byte selector`, `int goal`, `byte call`, groups of the mob (`reads`)), reply: the bool or nothing, effects. Java invokes the method on the `Goal` object of the view's selector.
- Writes the goal makes reach native through bridges at once (`setTarget`, navigation and control bridges in 7-3).

## Tests
- `cargo test -p horizon-client --lib mobs` (`tick_split_at_selectors_equals_tick` for a vanilla zombie).
- Test mod `FollowNearestPlayerGoal` (mod code): `canUse` true when a player is within 10 blocks; `tick` calls `getNavigation().moveTo(player, 1.0)`. Check `java_goal_moves_the_mob`: a `TestMob` 8 blocks from the player is within 3 blocks after 100 ticks (the native navigation followed the Java goal's path request the same tick).
- `python3 tools/parity.py mob_ai` unchanged.
