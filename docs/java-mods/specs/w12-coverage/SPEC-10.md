# SPEC-10: living damage and effects (Balm, Jade)

Design: `DESIGN.md` §2.6 row 10. Needs SPEC-3, SPEC-5 (`Value` receivers), SPEC-8 §1 (field bridges), W9 3-2 (`add_effect` seam), W9 4-3, 4-4, 4-5 (damage sessions, reductions, death: the bridges a Java `hurtServer` body needs). Size: ~400 changed lines.

Paths as SPEC-2. Read first: vanilla `LivingEntity.java` (`hurtServer`, `causeFallDamage`, `calculateFallDamage`, `addEffect`, `onEffectAdded`, `onEffectUpdated`, `forceAddEffect`), `MobEffectInstance.java` (`update`, `onEffectAdded`); W9 `SPEC-3-2.md` (`add_effect`/`add_effect_body`), W9 4-3…4-6 specs; `G/potions.rs` (`apply_effect_action`), `crates/horizon-client-render/src/effects.rs` (~40, the merge), `B/PlayerSync.java` (`fillEffects` after SPEC-2), `B/EntityViews.java` (`apply` for the `EFFECTS` group); `java_mods.py check --analysis` output for these keys' closures.

Comments in code: one line at most.

## Keys
| Key | Kind | Mod | Seam |
|---|---|---|---|
| `LivingEntity#causeFallDamage(DFLnet/minecraft/world/damagesource/DamageSource;)Z` | body (wrap `calculateFallDamage`) | balm | native's fall-damage step for living entities and players: one `Game` fn `living_cause_fall_damage(id, distance, multiplier, source) -> bool` (W9 4-6 places the fall event; this is the same site) |
| `LivingEntity#hurtServer(Lnet/minecraft/server/level/ServerLevel;Lnet/minecraft/world/damagesource/DamageSource;F)Z` | body (wrap `isDeadOrDying`) | balm | W9 4-3's damage session entry: `living_hurt_server(id, source, amount) -> bool` |
| `MobEffectInstance#update(Lnet/minecraft/world/effect/MobEffectInstance;)Z` | edge (`wrap_method`) | jade | inside `add_effect_body` where vanilla's `addEffect` calls `existing.update(new)` |
| `MobEffectInstance#onEffectAdded(Lnet/minecraft/world/entity/LivingEntity;)V` | inject (SPEC-1's analysis gives where) | jade | where vanilla's `LivingEntity.onEffectAdded` calls `effect.onEffectAdded(this)` |

## 1. Effect instances keep their identity in views
Jade's mixin adds fields to `MobEffectInstance` (`jade$addTime`, `jade$updateTime`, javap of `snownee.jade.mixin.MobEffectInstanceMixin`): state that lives on the instance in the entity's effect map. Today `PlayerSync.fill` clears and rebuilds the player's map, and W9's `EFFECTS` group rebuilds a mob's, so any such field is lost at the next refresh.
- `PlayerSync.fillEffects` and `EntityViews.apply` (`EFFECTS`): update in place. For an effect present before and after, set duration, amplifier, ambient and visibility on the existing instance through an accessor mixin (`MobEffectInstanceAccess`, raw: no `update`, no callbacks); add new ones; remove gone ones. `sendEffectChanges` keeps its packets.
- Test: `effect_instance_survives_refresh` (below).

## 2. Receivers on an effect
`Recv::Effect { entity: u32 or the acting player, effect: &str }` (byte 8): Java's receiver is the live instance `((LivingEntity) view).getEffect(holder)` (for the acting player, the player view's). For `update`, whose receiver is the existing instance and whose argument is the new one: the receiver is that live instance, the argument an `EffectArg`. The handler and body mutate the receiver in place; the body bridge (native merge) writes native's result back raw to the same instance before RETURN handlers read it (a `refresh` of the effect group for that one effect).

## 3. Seams and bodies
- **`update`**: `add_effect_body` (W9 3-2) calls `self.effect_update(target, &existing, &new) -> bool` where vanilla calls `update`. Gate first (`Recv::Effect`), body = the native merge in `effects.rs` (a lib fn, called from this `Game` fn, so no lib gate is needed) applied to native's effect list, then the one-effect raw write to the view (§2).
- **`onEffectAdded`**: `add_effect_body`'s "new effect" branch calls `self.effect_on_added(target, effect)` where vanilla's `LivingEntity.onEffectAdded` calls `effect.onEffectAdded(this)`; gate first, body = native's attribute modifier application for that effect (whatever native does at that point today).
- **`causeFallDamage`** (body): Java runs vanilla's body on the view. Its closure (`needs_bridge`: `Entity#playSound`, `propagateFallToPassengers`, `calculateFallDamage`, `getFallDamageSound`, `playBlockFallSound`) is bridged: `calculateFallDamage` and the sounds to their native ports (W9 4-6's), `hurtServer` to the gated `living_hurt_server` (so a fall that hurts goes through the damage seam, and balm's other route).
- **`hurtServer`** (body): Java runs vanilla's `LivingEntity.hurtServer` on the view. This replaces native's damage path for admitted types (all living types, unless SPEC-4 narrows balm's change). Its closure is the larger part of this spec: list it from `check --analysis` (the report showed 30: `broadcastDamageEvent`, `gameEvent`, `playSound`, `applyItemBlocking`, `dealDefaultKnockback`, `die`, `getAttributes`…). Each gets a bridge to the native function W9 phase 4 ports (shield, reductions, knockback, death, drops). Field writes of the body (`invulnerableTime`, `hurtTime`, `lastHurt`, `lastDamageSource`…) go through SPEC-8's field bridges: add their rows. A field whose native counterpart doesn't exist yet is a W9 4-x gap: report it, don't stub it.
- Players: `hurtServer` on the player view runs `ServerPlayer.hurtServer` (its override calls `super`); admission follows W12's overrider rule. Keep players in if admitted; their health writes already go through `PlayerSyncMixin`.

## 4. Tests
- Test mod mixins (`J/testmod/…/mixin/LivingRouteMixin.java`):
  - `@WrapOperation(method = "causeFallDamage", at = @At(value = "INVOKE", target = "…calculateFallDamage(DF)I"))`: halves the result for entities tagged `horizon_soft`;
  - `@WrapOperation(method = "hurtServer", at = @At(value = "INVOKE", target = "…isDeadOrDying()Z"))`: records the value it saw;
  - `@WrapMethod(method = "update")` on `MobEffectInstance`: records the call and the old/new durations; `@Inject(method = "onEffectAdded", at = @At("HEAD"))`: counts.
- `ChecksEntities`:
  - `fall_damage_halved`: a `horizon_soft` zombie dropped from 20 blocks ends with the health vanilla's formula gives for half the damage (compute the expected number by hand from vanilla's `calculateFallDamage` for a 20-block fall with no effects: write the constant in the check with the arithmetic as its one-line comment);
  - `hurt_body_sees_death`: `/damage <zombie> 1000` → the wrap saw `true`, the zombie died once (one `LivingDeathEvent`, W9 4-5), drops spawned once;
  - `effect_update_wrapped`: `/effect give @e[tag=t] speed 10` then `… speed 30`: one `update` call with old ≤ 10 s, new 30 s;
  - `effect_instance_survives_refresh`: the identity hash of the zombie's speed instance is the same before and after 20 ticks of refreshes.
- `python3 tools/parity.py --changed-files <files> --list`, then the damage and potion harnesses it names (`potion`, `mob_ai` damage cases, `server_player_damage`): without mods nothing moves.
- `cargo test` of the touched modules; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run.

## Done bar
Checks pass; four gates; `check --analysis` lists no unbridged writer for these routes' closures; lint passes.

## For the lead and the user
- Lead: M1 on the showcase pack: `mod_parity.py` fight scenario; balm's and jade's handler counts are non-zero and match NeoForge's within variance.
- User (Jade): look at a zombie with Speed from a splash potion; Jade's tooltip shows the effect and its remaining time counting down; throw a second, longer potion: the time jumps up. Same as NeoForge.
- User: fall from 20 blocks in survival with and without feather falling boots: the damage taken matches NeoForge's (same hearts lost).
