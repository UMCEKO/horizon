# SPEC-4-3: incoming damage and invulnerability

Design: `DESIGN.md` §4 rows `LivingIncomingDamage`, `EntityInvulnerabilityCheckEvent`. Index: `SPEC-2-8.md`. Size: ~280 lines. Needs 4-1, 2-1 (add the hooks below to `HOOKS`).

## Vanilla, patched (javap of `LivingEntity.hurtServer`, `Entity.isInvulnerableToBase`)
- `hurtServer` pushes `new DamageContainer(source, damage)` on `this.damageContainers` (a `Stack` field), then `CommonHooks.onEntityIncomingDamage(this, container)` → `boolean` (true: return false), then works on `container.getNewDamage()`; it sets reductions (`setReduction`), `setPostAttackInvulnerabilityTicks`, `setShouldCauseSideEffects`, and pops at the end, after `CommonHooks.onLivingDamagePost`. `applyItemBlocking`, `getDamageAfterMagicAbsorb` and `actuallyHurt` read and write the same container (4-4).
- `isInvulnerableToBase(source)` ends with `CommonHooks.isEntityInvulnerableTo(this, source, vanillaResult)` → `boolean`.

Use `javap -c -p` of the patched `LivingEntity` (`/home/umceko/hz-poc/agents/entities/p2/jar/`, extracted) to follow the exact order of the container calls; it is patched vanilla, which DESIGN uses for order.

## Damage sessions
A hurt of an admitted living entity opens a **session** in Java for its length:
- Rust `java_damage_begin(victim, source: &Source, amount) -> Option<Session>` at the top of the native hurt port (the `damage` / `actuallyHurt` path and `hurt_item_entity` is not living: no session), only if any of `Hook::IncomingDamage`, `ShieldBlock`, `DamagePre`, `DamagePost`, `ArmorHurt` is admitted for the type. Op `damage_begin`: victim + groups, `Source`, `float amount`. Java: `view.damageContainers.push(new DamageContainer(source, amount))` (field through an accessor mixin), then `CommonHooks.onEntityIncomingDamage(view, container)`. Reply: `bool canceled`, `float newDamage`, `int invulnerabilityTicks`, `bool sideEffects`, `int reductionMask` (the `Reduction`s with modifiers), effects.
- Canceled → native returns false from the hurt, as vanilla. Otherwise native continues with `newDamage` and the two settings where vanilla reads them.
- `java_damage_end(session)` where vanilla pops (`damage_end` op), on every exit path of the port, including early returns after `begin`.
- Sessions nest (a hurt inside a hurt: thorns); the Java stack mirrors it.
- Players (guests) get the same through `Player.actuallyHurt`'s patched path, under `as_guest`.

## Invulnerability
`horizon_world::damage_immunity::base_immunity` (`damage_immunity.rs:38`) callers in the bin: after the vanilla result, if `Hook::InvulnerabilityCheck` is admitted for the type, op `invulnerable` (`Source`, `bool vanilla`) → `CommonHooks.isEntityInvulnerableTo(view, source, vanilla)` → bool. Lib callers inside a mob's body use 4-2's shared crossing (`Site::Invulnerable`; add it).

## Test mod
- Listeners: `LivingIncomingDamageEvent` cancels for entities tagged `horizon_shield`, and halves `setAmount` for tag `horizon_half`; `EntityInvulnerabilityCheckEvent` sets invulnerable for tag `horizon_ghost` against `minecraft:fall`.
- Checks: `incoming_damage_cancel` (a tagged pig hit by `/damage … 5` keeps full health); `incoming_damage_halves` (`/damage … 6` takes 3); `invulnerable_to_fall` (a `horizon_ghost` pig dropped from 20 blocks keeps its health; an untagged one doesn't).

## Tests to run
- `cargo test … java_entities` (session codec), `java_mods.py build`, `check --analysis`, `bridges`.
- `python3 tools/parity.py damage_immunity damage_source server_player_damage`: unchanged without listeners.
