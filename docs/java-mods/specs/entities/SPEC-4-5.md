# SPEC-4-5: death, totem, drops and experience events

Design: `DESIGN.md` §4 rows `LivingDeathEvent`, `LivingDropsEvent`, `LivingExperienceDropEvent`. Index: `SPEC-2-8.md`. Size: ~280 lines. Needs 4-1 (`die` at the blow), 4-3.

## Vanilla, patched (javap)
- `LivingEntity.die(source)`: `if (CommonHooks.onLivingDeath(this, source)) return;` first. `Player.die` and `ServerPlayer.die` call the same hook first.
- `checkTotemDeathProtection(source)`: `CommonHooks.onLivingUseTotem(this, source, totemStack, hand)` → `boolean` (false: the totem isn't used).
- `dropAllDeathLoot(level, source)`: the loot (`dropFromLootTable`, `dropCustomDeathLoot`, equipment) is **captured** into a collection of `ItemEntity`s (`captureDrops`), then `CommonHooks.onLivingDrops(this, source, drops, recentlyHit)` → `boolean` (true: none spawn), else each is added to the level.
- `dropExperience(level, killer)`: `EventHooks.getExperienceDrop(this, player, vanillaXp)` → `int`.

## Change
- `die` (4-1): first `java_living_death(victim, source) -> bool` (op `living_death`) if admitted; true → return (no death; health stays at 0 as vanilla leaves it).
- Totem: `java_use_totem(victim, source, stack, hand) -> bool` before native consumes it.
- **Drops captured, then the event, then spawned:** native's loot for a death (`mobs/drops.rs`, `drops.rs:401`: equipment, loot table, custom loot) is produced into a `Vec<ItemEntity>` (not yet `level_add`ed: ids assigned, `Fresh`). If `LivingDropsEvent` is admitted, op `living_drops`: victim + groups, `Source`, `bool recentlyHit`, the items (their saved forms). Java builds unbacked `ItemEntity`s from them (as `entity_view` would, without joining), posts through `CommonHooks.onLivingDrops(view, source, list, recentlyHit)`, and replies `bool canceled` + the final list as saved forms (mods add, remove and edit drops). Native then `level_add`s each, in list order. Not admitted: native adds its list directly.
- XP: `java_experience_drop(victim, player, xp) -> i32` (op `experience_drop`) where native computes the orb total.

## Test mod
- `LivingDeathEvent`: cancel for tag `horizon_undying`; `LivingDropsEvent`: add a diamond for tag `horizon_rich`, cancel for tag `horizon_poor`; `LivingExperienceDropEvent`: set 100 for tag `horizon_rich`.
- Checks: `death_cancel` (a tagged pig `/kill`ed: still in the level); `drops_added` (a diamond item entity at the rich zombie's death spot the same tick); `drops_canceled` (no items from the poor one); `xp_set` (orbs total 100).

## Tests to run
- `java_mods.py build`, `check --analysis`, `bridges`; `python3 tools/parity.py mob_drops server_player_damage` unchanged.

## User step
MekanismTools / Mekanism world: kill a zombie with a Meka-Tool; the drops match Forge (side by side if possible).
