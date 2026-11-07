# SPEC-4-6: fall, jump, knockback, size and ender teleport events

Design: `DESIGN.md` §4 rows `LivingFallEvent`, `LivingJumpEvent`, `LivingKnockBack`, `EntityEvent.Size`, `EntityTeleport.EnderEntity`. Index: `SPEC-2-8.md`. Size: ~260 lines. Needs 4-2.

## Vanilla, patched (javap)
| Method | Hook | Result |
|---|---|---|
| `LivingEntity.causeFallDamage(fallDistance, multiplier, source)` | `CommonHooks.onLivingFall(this, distance, multiplier)` → `LivingFallEvent` | canceled → return false; else distance and multiplier from the event |
| `Player.causeFallDamage` | `EventHooks.onPlayerFall(this, distance, multiplier)` (flying players) | none |
| `LivingEntity.jumpFromGround()` | `CommonHooks.onLivingJump(this)` at the end | none |
| `LivingEntity.knockback(strength, x, z, source, …)` | `CommonHooks.onLivingKnockBack(this, strength, x, z)` → event | canceled → no knockback; strength, ratios from the event |
| `Entity.<init>` and `refreshDimensions()` | `EventHooks.getEntitySizeForge(this, pose, old, new)` → `EntityEvent.Size` | the new dimensions and eye height |
| `LivingEntity.checkPositionAndTeleport` (chorus, enderman), `Shulker.teleportSomewhere` | `EventHooks.onEnderTeleport(this, x, y, z)` → event | canceled → no teleport; target from the event |

Find the exact positions with `javap -c -p` of the patched classes.

## Sites
- **Fall:** `mobs/living.rs:904` (lib, mob body: 4-2 `Site::Fall`), the player's (`motion.rs` fall damage, guest under `as_guest`: a normal crossing), armor stands (none: `ArmorStand.causeFallDamage` returns false before the hook? check).
- **Jump:** the mob jump in the lib (`Site::Jump`), the player's `motion.rs:4605`.
- **Knockback:** `creatures.rs:2716` knockback and the lib's mob-on-mob knockback (`Site::Knockback`), explosion and wind knockback are `Entity.push`, not `knockback` (no event).
- **Size:** where native changes a mob's box (`refresh_dimensions`, pose and baby changes; grep `refresh_dimensions\|set_pose`), and once at construction for types the facts admit. The answer replaces native's dimensions for that entity until the next refresh. A mod overriding `getDefaultDimensions` is phase 6.
- **Ender teleport:** the enderman's and shulker's teleport ports and chorus fruit (`consume.rs`), through `Site::Teleport` for the lib ones.

## Test mod
Listeners keyed on tags: `horizon_feather` cancels falls; `horizon_spring` records jumps; `horizon_heavy` cancels knockback; `horizon_tall` sets the size to 1×3; `horizon_anchor` cancels ender teleports.
Checks: `fall_canceled`, `jump_seen` (a `horizon_spring` slime's jumps counted > 0 in 100 ticks), `knockback_canceled` (a tagged zombie hit by the wand doesn't move), `size_event` (a tagged pig's box height reads 3 through `getBbHeight` and native's `/data get` of nothing… use `level.getEntities` with a 1×3 box query), `ender_teleport_canceled` (a tagged enderman hit by an arrow stays put).

## Tests to run
- `java_mods.py build`, `check --analysis`, `bridges`; harnesses `mob_ai`, `fall_damage`, `knockback`, `enderman` (or what `--changed-files` names): unchanged.
