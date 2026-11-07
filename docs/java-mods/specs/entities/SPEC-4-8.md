# SPEC-4-8: items, lightning, projectile impact, mounting, conversion, targets, heal, effects

Design: `DESIGN.md` §4 rows `ItemExpireEvent`, `ItemEntityPickupEvent`, `StruckByLightning`, `ProjectileImpact`, `EntityMount`, `LivingConversion`, `LivingChangeTargetEvent`, plus the effect and heal hooks the patched `LivingEntity` has. Index: `SPEC-2-8.md`. Size: ~300 lines (split 4-8a items/lightning/impact/mount, 4-8b the rest, if over). Needs 4-2.

| Hook (patched caller) | Native site | Result |
|---|---|---|
| `EventHooks.onItemExpire(item)` → `int` extra life (`ItemEntity.tick`, before the 6000-tick discard) | `turn_item` | life added, else discard |
| `EventHooks.fireItemPickupPre(item, player)` → event (`canPickup` tri-state), `fireItemPickupPost(item, player, copy)` (`ItemEntity.playerTouch`) | the item pickup in the player's `aiStep` touch (`game.rs` pickup, guest under `as_guest`) | allow, deny or default; post after the take |
| `EventHooks.onEntityStruckByLightning(entity, bolt)` → bool canceled (`LightningBolt.tick`, per entity, before `thunderHit`) | `thunder_hit` per entity (1c2a §3.1) | skip that entity |
| `EventHooks.onProjectileImpact(projectile, hit)` → bool canceled (`AbstractArrow.stepMoveAndHit`, `AbstractHurtingProjectile.tick`, `ThrowableProjectile.tick`, `FireworkRocketEntity.onHit`, `FishingHook.checkCollision`, `LlamaSpit.tick`, `ShulkerBullet.tick`) | each native hit port in `turn_projectile` and the other kinds' turns | canceled → no hit this tick (the projectile goes on) |
| `EventHooks.canMountEntity(rider, vehicle, mounting)` → bool (`Entity.startRiding`, `removeVehicle`) | `start_riding`/`stop_riding` (`passengers.rs`) | false → no mount / no dismount |
| `EventHooks.canLivingConvert`… and `onLivingConvert(old, new)` (the conversion lambdas: piglin, hoglin, zombie villager, villager, pig, mooshroom, tadpole, `ConversionTracker`) | `conversion.rs` and the bin's conversion handler (1c2c-4) | canceled → no conversion; post after |
| `CommonHooks.onLivingChangeTarget(mob, target, type)` → event (`Mob.setTarget`, `StartAttacking`) | goal and brain target setters (`monster_brain.rs:571`, goals): 4-2 `Site::ChangeTarget` | new target or cancel |
| `EventHooks.onLivingHeal(entity, amount)` → `float` (`LivingEntity.heal`) | native heal ports (mobs lib: `Site::Heal`; players) | amount; ≤ 0 → no heal |
| `CommonHooks.canMobEffectBeApplied(entity, effect, source)` (`addEffect`, `forceAddEffect`), `EventHooks.onEffectRemoved` (`removeEffect`, `removeAllEffects`), plus `MobEffectEvent.Added`/`Expired` (find their callers with `callers.sh`) | the effect add/remove/expire ports | applicable bool; remove canceled |

Use `callers.sh` and `javap -c -p` of each patched caller for exact positions. Add each hook to 2-1's `HOOKS`.

## Test mod
One listener per row keyed on a tag (expire: +100 life; pickup: deny for `horizon_sticky` items; lightning: cancel for `horizon_rod`; impact: cancel arrows against `horizon_bulletproof`; mount: deny `horizon_unrideable`; conversion: cancel villager→witch; target: cancel any target for `horizon_pacifist` zombies; heal: double for `horizon_regen`; effect applicable: deny poison for `horizon_immune2`), and one check each.

## Tests to run
`java_mods.py build`, `check --analysis`, `bridges`; the harnesses `--changed-files` names (unchanged without listeners).
