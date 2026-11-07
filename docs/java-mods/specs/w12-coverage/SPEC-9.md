# SPEC-9: guarded entity hooks (Travelers Backpack, Waystones, Mekanism Additions)

Design: `DESIGN.md` §2.3, §2.6 row 9. Needs SPEC-4 (guards, item admission), SPEC-5 (lib gates, for the reads inside lib physics), W9 3-1 and 3-2 (`Recv::Entity`; `item_tick` already gated for ae2), W9 2-9/2-10 (one turn function per kind). Size: ~400 changed lines.

Paths as SPEC-2; `L/` = `crates/horizon-client/src/`. Read first: vanilla `Entity.java` (`tick`, `baseTick`, `isInWater`, `isInvulnerable`, `onBelowWorld`, `checkBelowWorld`), `ItemEntity.java` (`tick`, `fireImmune`, `ignoreExplosion`, `setItem`, `hurtServer`), `ServerExplosion.java` (where `ignoreExplosion` is asked); `L/entities.rs` (`tick_item`, the item's fire and lava handling), `L/motion.rs` (`is_in_water` reads, `check_below_world`), `L/mobs/living.rs`, `G/explosion.rs` (`hurt_item_entity`, the per-entity explosion loop), W9's turn functions (`turn_item` and the other kinds), SPEC-4's guard output for these keys.

Comments in code: one line at most.

## Keys
| Key | Kind | Mods | Expected admission (SPEC-4) |
|---|---|---|---|
| `Entity#isInWater()Z` | edge HEAD | travelersbackpack | type `minecraft:item`, items = backpacks |
| `Entity#isInvulnerable()Z` | edge HEAD | travelersbackpack | same |
| `Entity#onBelowWorld()V` | edge HEAD | travelersbackpack | same |
| `ItemEntity#fireImmune()Z` | edge HEAD | travelersbackpack | items = backpacks |
| `ItemEntity#ignoreExplosion(Lnet/minecraft/world/level/Explosion;)Z` | edge HEAD | travelersbackpack | items = backpacks |
| `ItemEntity#setItem(Lnet/minecraft/world/item/ItemStack;)V` | edge HEAD | travelersbackpack | arg item = backpacks (SPEC-4 `arg_item`) or receiver item |
| `ItemEntity#tick()V` | edge HEAD (+ ae2 RETURN) | travelersbackpack, ae2 | all items (ae2's change has no guard) |
| `Entity#tick()V` | edge TAIL + inject (SPEC-1's analysis gives mekanismadditions' kind) | waystones, mekanismadditions | from SPEC-4; likely all types |

If SPEC-4's walker returns `all` for a Travelers Backpack key, this spec still gates it, with the cost that implies; report the measured cost (§4) so the lead can decide.

## 1. `ItemEntity#tick` (shared with W9 3-2)
W9 3-2 gates the key in `item_tick(id)`. Travelers Backpack's HEAD rides the same gate; nothing to add but its check (§3). If 3-2 hasn't landed, this spec waits for it: one key, one gate.

## 2. Seams
**In `Game` code (`route!`):**
- `ItemEntity#ignoreExplosion`: in `G/explosion.rs`'s per-entity loop, where vanilla's `ServerExplosion.hurtEntities` asks `entity.ignoreExplosion(this)` (check the source for the exact place): `fn item_ignores_explosion(&mut self, id, explosion: &ExplosionArg) -> bool`, gate first (`Recv::Entity(id)`, `item = stack item`), body = native's answer today (`false` for items, unless native already has a rule).
  - `ExplosionArg` (new `RouteArg`, add its reader to SPEC-3's table): `text dim, double x, y, z, float radius, bool fire, int interaction (Level.ExplosionInteraction ordinal), int source entity id (0 none)`. Java builds `new ServerExplosion(level, source, null, null, center, radius, fire, blockInteraction)` (check the 26.3 constructor), used only as the argument.
- `ItemEntity#setItem`: one `fn item_entity_set_item(&mut self, id, stack: ItemStack)` with the gate first (`item = stack.item`), body = the native write plus whatever vanilla's `setItem` does (it also sets the despawn-related state in 26.3? check). Route every native write of an item entity's stack through it: grep `.stack =` and `stack:` constructions on `ItemEntity` in `L/entities.rs`, `G/` (merging, partial pickups, `/data`, hopper pickup leftovers). Construction of a new item entity is `ItemEntity`'s constructor in vanilla, which calls `setItem`: route the native spawn of an item entity through it too (after `level_add`, as vanilla's constructor runs before `addFreshEntity`: if the order matters for the view, construct, then `item_entity_set_item`, then add).
**In lib code (`lib_route!`, SPEC-5):**
- `Entity#isInWater`: every read of the in-water flag of an item entity in `L/entities.rs` `tick_item` and in `L/motion.rs` for items, through one helper `fn in_water(item: &ItemEntity) -> bool` that asks `lib_route!(…Entity#isInWater…, LibRecv::Entity(item.id), item = &item.stack.item)` first. Other entity kinds aren't admitted, so their reads stay plain field reads; don't touch them unless SPEC-4's admission includes their type.
- `Entity#isInvulnerable`, `ItemEntity#fireImmune`: same pattern at the item's reads (fire, lava, cactus, the damage-type checks in `tick_item` and `hurt_item_entity`'s inputs).
- `Entity#onBelowWorld`: at the item's below-world check (vanilla `checkBelowWorld` → `onBelowWorld`): the body is today's removal; a HEAD cancel keeps the item.
A lib gate on a type whose read sits in `Game` code instead takes `route!`; use whichever the call site allows, never both.

## 3. `Entity#tick` (Waystones TAIL, Mekanism Additions)
Vanilla's `Entity.tick()` is `baseTick()`; every subclass's `tick` calls `super.tick()` at its start (check each kind W9 turns: `ItemEntity`, `ExperienceOrb`, `LivingEntity`, projectiles, vehicles, `FallingBlockEntity`…).
- Add `fn entity_tick_base(&mut self, id) ` in `G/` (next to W9's turn functions), gate first (`Recv::Entity(id)`), body = the kind's `baseTick` port. Each turn function calls it where vanilla's subclass `tick` calls `super.tick()`, so the TAIL handlers run after `baseTick` and before the rest of the subclass tick, as in vanilla.
- TAIL with several returns: mark the body's early returns `route::early` (W12 §3; `java_routing.py lint` checks it).
- This is per entity per tick when admitted for all types; measure it (§4).

## 4. Tests
- Test mod mixins (`J/testmod/…/mixin/EntityHookMixin.java`), guarded the way SPEC-4 recognises (`if (!(this instanceof ItemEntity e) || !(e.getItem().getItem() == Items.NETHER_STAR)) return;` is **not** a class guard; use a test-mod item class `TestFloatItem extends Item` registered as `horizon_java_test:float_item`):
  - `isInWater` HEAD: `cir.setReturnValue(false)` for `TestFloatItem` stacks;
  - `fireImmune` HEAD: `true` for them; `ignoreExplosion` HEAD: `true`;
  - `onBelowWorld` HEAD: `ci.cancel()` and `setPos(x, level.getMinY() + 5, z)` for them;
  - `Entity#tick` TAIL: count calls per entity id.
- `ChecksEntities` (`later`):
  - `float_item_admitted_only`: the route call count for `isInWater` is non-zero, and the profile has no `route:…isInWater` call for a plain stone item the session also drops in water (admission, not a skip);
  - `float_item_ignores_fire`: thrown into lava, the item still exists after 100 ticks; a stone item thrown beside it is gone;
  - `float_item_survives_tnt`: a primed TNT next to it: it exists after; a stone item doesn't;
  - `float_item_below_world`: teleported to `y = minY - 10`: it reappears at `minY + 5`;
  - `entity_tick_tail_once_per_tick`: a pig's TAIL count equals ticks elapsed ±1.
- Cost: run W9's 360-mob scenario (`agents/entities/mobs.json`) with `HORIZON_PROFILE`; report `route:…Entity#tick` calls per tick and mean, and `tick_entities` before/after. If `Entity#tick` is admitted for all types and costs more than 1 ms a tick there, report it with the numbers (no skipping).
- `python3 tools/parity.py item_entity explosion` (unchanged without mods); `cargo test` of the touched modules; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run.

## Done bar
Checks pass; the eight keys gated (one shared with W9 3-2); `java_routing.py lint` passes; the cost report is in the hand-off.

## For the lead and the user
- User (Travelers Backpack, showcase install): drop a backpack into lava: it floats and survives; into water: it floats up (doesn't sink and flow away); next to TNT: it survives; drop it into the void in the End: it comes back up. A normal item in the same places burns, sinks, explodes, falls. Same as on NeoForge.
- User (Waystones): step on a warp plate with a mob or item on it; the warp happens after the plate's delay, as on NeoForge.
