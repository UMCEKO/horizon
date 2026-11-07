# SPEC-3-2: the pack's entity routes gated (ae2 ×2, Jade ×1)

Index: `SPEC-2-8.md`. Size: ~220 lines. Needs 3-1. Mechanism: W12 `DESIGN.md` §3 (a gated port and its body fn, the body bridge).

## Gates
| Key | Kind (W12 SPEC-1) | Port fn (gate first) → body fn | Notes |
|---|---|---|---|
| `net/minecraft/world/entity/item/ItemEntity#tick()V` (ae2 RETURN) | edge | `item_tick(id)` → `item_tick_body(id)`: the tick part of `turn_item` (2-9), i.e. `ItemEntity.tick`'s port, without `commonTick` | `ItemEntity.tick` has the empty-item `discard()` branch and the normal end: count the `return`s of the patched method with `javap -c -p`; mark each non-last one `route::early` in the body fn (W12 §3) so TAIL changes stay exact |
| `net/minecraft/world/entity/item/ItemEntity#hurtServer(Lnet/minecraft/server/level/ServerLevel;Lnet/minecraft/world/damagesource/DamageSource;F)Z` (ae2 HEAD, cancellable) | edge | `hurt_item_entity(id, source, amount)` (1c2a §4) → `hurt_item_entity_body` | every native item damage goes through it (explosions, fire, lava, cactus, lightning, `/damage`, the `hurtServer` bridge) |
| `net/minecraft/world/entity/LivingEntity#addEffect(Lnet/minecraft/world/effect/MobEffectInstance;Lnet/minecraft/world/entity/Entity;)Z` (Jade HEAD) | edge | `add_effect(target, effect, source)` → `add_effect_body`: one port of `LivingEntity.addEffect` for mobs, armor stands and players | today the adds are spread (`effects.rs:186` callers, mob effect adds): make every caller go through this seam first (the seam is the larger part of this step); lib adds arrive as `MobEvent`s (3-1 §2) |

- Body bridges (`route … if backed`, W12 §3) for the three, in `G/java_bridges/entities.rs`, calling the body fns. New `RouteArg`/`RouteRet`/`Flat` pairs as needed: `Source` (1c2a's flattened `DamageSource`), `Effect` (`text id`, `int amplifier`, `int duration`, `bool ambient`, `bool visible`, `bool icon`), entity refs (`Ent`, 0 = null).
- Remove the three keys from `tools/java_routing/deferred.tsv` (`view: entities 1c`).

## Test mod mixins
Add `horizon_java_test.mixins.json` (`neoforge.mods.toml` `[[mixins]]`), package `dev.horizonlab.javatest.mixin`:
- `ItemEntityTickMixin`: `@Inject(method = "tick", at = @At("RETURN"))` counts calls per item id; plus `@At("TAIL")` counting separately;
- `ItemEntityHurtMixin`: `@Inject(method = "hurtServer", at = @At("HEAD"), cancellable = true)` → `cir.setReturnValue(false)` for stacks named `armored`;
- `AddEffectMixin`: `@Inject(method = "addEffect(…)Z", at = @At("HEAD"), cancellable = true)` → false for `minecraft:poison` on entities tagged `horizon_immune`.

`ChecksEntities` (`later`):
- `item_tick_route_every_tick`: RETURN count = ticks elapsed (±1); an item that expired in the empty branch got RETURN but not TAIL on that call;
- `item_hurt_route_cancels`: an `armored` item survives `hurtServer(…, 10)` and 20 ticks in lava;
- `add_effect_route_cancels`: `/effect give @e[tag=horizon_immune] poison` leaves it without poison; speed applies.

## Checks
- `java_mods.py build`, `check --analysis`, `bridges`; `python3 tools/java_routing.py lint`.
- `python3 tools/parity.py item_entity potion` (no route without mods: unchanged).
- M1 (parent): `mod_parity.py run` on the pack: the ae2 and Jade handler counts are no longer zero on Horizon (DESIGN §1.4).

## User step
AE2 + Jade pack: AE2's item behaviours work as on Forge; Jade's effect tooltip updates when a mob gets an effect.
