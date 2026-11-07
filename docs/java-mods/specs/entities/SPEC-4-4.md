# SPEC-4-4: shield block, reductions, Damage.Pre and Post, armour hurt

Design: `DESIGN.md` §4 row `ShieldBlock`, `Damage.Pre/Post` (`DamageContainer` reductions). Index: `SPEC-2-8.md`. Size: ~280 lines. Needs 4-3 (sessions).

## Vanilla, patched (javap)
Inside a session (4-3), in this order (confirm with `javap -c -p` of the patched `LivingEntity`: `hurtServer`, `applyItemBlocking`, `actuallyHurt`, `getDamageAfterArmorAbsorb`, `getDamageAfterMagicAbsorb`, `doHurtEquipment`):
1. `applyItemBlocking` → `CommonHooks.onDamageBlock(this, container, blocked, blocking)` → `LivingShieldBlockEvent`; `container.setBlockedDamage(event)`.
2. Reductions: each `setReduction(Reduction.X, value)` runs the container's modifier functions for `X` (`IReductionFunction`s that mods added in `LivingIncomingDamageEvent`) and stores the result; vanilla code then uses `getNewDamage()`/`getReduction(X)`. Types: `javap -p 'net.neoforged.neoforge.common.damagesource.DamageContainer$Reduction'` (expect INVULNERABILITY, ARMOR, ENCHANTMENTS, MOB_EFFECTS, ABSORPTION).
3. `actuallyHurt`: `CommonHooks.onLivingDamagePre(this, container)` → `float` (the damage to apply), absorption, `captureInflictedDamage`, health.
4. `doHurtEquipment` → `CommonHooks.onArmorHurt(source, slots, damage, this)`.
5. `hurtServer`'s end: `CommonHooks.onLivingDamagePost(this, container)`.

## Change
- Native's ports of these methods call Java at each step **only when the session exists and the step's hook is admitted or the reduction type is in `reductionMask`** (4-3's reply):
  - `damage_block` (`float blocked`, `bool blocking`) → reply `bool blocked`, `float blockedDamage`, `float shieldDamage`, `bool shieldTakesDamage`;
  - `damage_reduction` (`byte type`, `float vanilla`) → `float` reduction after modifiers;
  - `damage_pre` → `float`;
  - `armor_hurt` (slots, `float`) → per-slot damage after the event (javap `ArmorHurtEvent` for its fields);
  - `damage_post` → nothing.
- Native reads each reply where vanilla reads the container. Where native computes a reduction vanilla records with `setReduction` (armor, enchantments, effects, absorption), name the native function and the vanilla line in the report.
- Lib-side reductions (inside a mob's body) go through 4-2's shared crossing (`Site::Reduction`).

## Test mod
- In `LivingIncomingDamageEvent`, for tag `horizon_armor_x2`: `addReductionModifier(ARMOR, (c, r) -> r * 2)` (check the API name with `javap -p LivingIncomingDamageEvent`).
- `LivingDamageEvent.Pre`: for tag `horizon_pre_one`, `setNewDamage(1)`. `Post`: record `getNewDamage()`.
- `LivingShieldBlockEvent`: cancel blocking for tag `horizon_no_shield`.
- Checks: `armor_reduction_modifier_applies` (a zombie in iron armour with the tag takes less than one without, by vanilla's armour formula with the doubled reduction); `damage_pre_sets_the_damage` (health − 1 whatever the hit); `damage_post_sees_the_dealt_damage`; `shield_block_cancel` (a shield-holding, blocking test mob takes the hit).

## Tests to run
- `java_mods.py build`, `check --analysis`, `bridges`; `python3 tools/parity.py server_player_damage armor_damage shield` (unchanged).
