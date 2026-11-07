# SPEC-6-3: a mod type's overrides run in Java at native's sites

Design: `DESIGN.md` §8 ("Overrides are routes"), §3 table (`PrimedTnt.tick`/`explode` override; `super.tick()` → native), W12 `DESIGN.md` §2.3 ("a mod class overriding the method isn't admitted: its own override routes separately"). Index: `SPEC-2-8.md`. Size: ~280 lines. Needs 6-2, 3-1 (W12 entity receivers).

## Mechanism (on W12's gate, no new hook)
- **Override routes** are W12 routes of kind `body` whose admitted set is the mod types whose class (or a mod superclass) declares the vanilla method (6-1's `overrides`, by class chain). `Routing.confirm` adds them at the first server start with the entity types: key = the vanilla method key, family `entity`, `override` flag. They live in `routes.bin` beside the mixin routes; one route can have both admitted sets (a mixin route admits vanilla types; the override route admits mod types).
- At the gated port (`route!` already there for a mixin route, or added now for an overridden method with a native port), `Recv::Entity` of a mod type admitted by the override route crosses; Java invokes the method **virtually** on the view: the mod's override runs.
- **`super.m()`** inside the override reaches the vanilla method's Java body, which for a gated, bridged method is the route stub / body bridge into native's body fn (W12 §2.2): native runs vanilla's body on the native entity. A method native ports but doesn't gate yet must be gated in this step for each override in the pack (below); the W12 lint's reference-pack list gains the pack's override keys so a missing gate fails CI.
- A mod override of a method **native doesn't port** as a site (a getter native never calls) needs nothing: no native caller, no effect (W17 P2: if a native caller should exist, it is a port to add with evidence).

## The pack's overrides (DESIGN §1.4) and their gates
| Override | Native port to gate |
|---|---|
| `PrimedTnt.tick` (ae2 tiny TNT, Mekanism obsidian TNT) | `tnt_tick(id)` (the tick part of `turn_tnt`) |
| `PrimedTnt.explode` | `tnt_explode(id)` (`tnt.rs`, before `server_explode`) |
| `Projectile.tick` (Mekanism flame) | `projectile_tick(id)` |
| `LivingEntity.isBaby` (Mekanism baby mobs) | `is_baby(e)`: every native read of `m.baby` on a mob that can be modded becomes this seam; boot-fact answer when the override is pure (§ below) |
| `Creeper.explodeCreeper`, `getDefaultDimensions` | `creeper_explode(id)`; dimensions: boot fact |
| `Entity.*` (balloon: list from `overrides`) | each method's port; name each in the report |

**Pure overrides as boot facts:** `getDefaultDimensions(Pose)` and `isBaby()` overridden with a body that reads no instance state (the analysis can tell: the method's bytecode has no `getfield`/virtual call on `this` except constants) are evaluated once per pose at boot and stored in the type fact, checked twice (DESIGN §3 table: "dimensions are a boot fact when pure (checked twice at boot)").

## Tests
- Test mod `TestTnt.tick` override: `super.tick()` then, at fuse 40, `setFuse(80)` once. Check `override_tick_runs_and_super_is_native`: the fuse goes 80→… →40→80→…→0 (native ticked it through `super.tick`, the override reset it once), and the explosion happens at the expected tick; `explode` override cancels the block damage (empty override): no blocks broken.
- `cargo test … java_routing` (`an_override_route_admits_only_its_mod_types`).
- `python3 tools/java_routing.py lint`.

## User steps
- AE2: place and prime a tiny TNT: small blast, as on Forge.
- Mekanism Additions: obsidian TNT and a baby creeper (spawn egg or natural): small creeper box, small blast.
