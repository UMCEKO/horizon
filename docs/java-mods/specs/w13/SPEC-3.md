# SPEC-3 (W13): `ExplosionEvent.Start`, `ExplosionEvent.Detonate`, `ExplosionKnockbackEvent`

Design: `DESIGN.md` §1.4 (vanilla order), §4, D5, D6. Read first: SPEC-1, `G/explosion.rs` (`server_explode`,
`server_explode_as`, `exploded_positions`, `explosion_hurt_entities`, `explosion_hurt_player`, the two
`native_explosion_knockback` calls), `G/native_explosions.rs`, `G/combat.rs` (`explosion_source`, `explosion_owner`,
`explosion_cause`), `G/java_bridges/server.rs` (the `ServerLevel.explode` bridge), `B/EntityViews.java`, vanilla
`ServerExplosion.java` and `ServerLevel.explode` in `.local/minecraft/26.3/sources/server`.

Depends on SPEC-1 and W9 SPEC-1d (every player has a view; until it lands, Detonate's list can't hold players other than
the acting one, so this spec doesn't land before it). About 400 changed lines.
Code comments: one line at most, only for a non-obvious why.

## Vanilla (patched jar, `javap/callers.javap`)
`ServerLevel.explode`: `new ServerExplosion(level, source, damageSource, calculator, center, radius, fire, interaction)` →
`EventHooks.onExplosionStart(level, explosion)`; canceled → return (no `explode`, no packets). `ServerExplosion.explode`:
`gameEvent(EXPLODE)` → `calculateExplodedPositions` → `hurtEntities(blocks)`: `level.getEntities(source, box)` →
`EventHooks.onExplosionDetonate(level, explosion, entities, blocks)` (both lists mutable) → for each entity left: damage,
then `EventHooks.getExplosionKnockback(level, explosion, entity, knockback, blocks)` → `pushFromExplosion`. Then
`interactWithBlocks(blocks)` on the (possibly changed) list, then fire.

Horizon's `server_explode_as` already has that shape: `native_explosion_prime` (Start's moment), `post_game_event`,
`exploded_positions`, `native_explosion_detonate` (Detonate's moment, blocks only), `explosion_hurt_entities` (with
`native_explosion_knockback` per pushed entity), blocks, fire.

## Wire (`level_event`, SPEC-1)
- Explosion header, in every call: `long serial` (a counter in `Game`, `explosion_serial: u64`, incremented per
  `server_explode_as` call), 3 `double` center, `float radius`, `bool fire`, `byte interaction`
  (`Explosion.BlockInteraction` ordinal: 0 `KEEP`, 1 `DESTROY`, 2 `DESTROY_WITH_DECAY`, 3 `TRIGGER_BLOCK`; map
  `BlockInteraction::Keep` → 0, the decay ones → 2, the rest → 1), `int source` (`combat.explosion_source` as the
  entity id, -1 none), `bytes owner` (empty, or the causing player's uuid as 2 `long`), `text damageType`.
- id 0 `ExplosionStart`: header. Result: empty. `canceled` is the answer.
- id 1 `ExplosionDetonate`: header, `int n` x `long pos` (the targets, in order), `int m` x entity ref. An entity ref is
  `byte 0` + `int id` (a native entity) or `byte 1` + 2 `long` uuid (a player). Result: `int n` x `long pos`, `int m` x
  entity ref: the lists as the listeners left them.
- id 2 `ExplosionKnockback`: header, entity ref, 3 `double` knockback, `int n` x `long pos`. Result: 3 `double`.

## Java (`B/ServerEvents.java`, cases 0–2 of `level`)
- `explosion(level, in)`: reads the header; looks up `serial` in `EXPLOSIONS`, a `LinkedHashMap<Long, ServerExplosion>`
  with access order capped at 8 entries (`removeEldestEntry`), so Start, Detonate and every Knockback of one blast share
  one object and a blast set off inside a listener keeps its own. Else builds
  `new ServerExplosion(level, sourceView, damageSource, null, center, radius, fire, interaction)`: `sourceView` is the
  entity view for `source` (`EntityViews` by id; null for -1); `damageSource` is
  `new DamageSource(registry holder of damageType, sourceView, ownerView)` where `ownerView` is the player view for
  `owner` (`GameBridge`'s player views, W9 SPEC-1d) or null. A null calculator gives vanilla's own
  (`makeDamageCalculator`).
- case 0: `ServerEvents.post("ExplosionEvent.Start", () -> canceled = EventHooks.onExplosionStart(level, explosion))`.
- case 1: builds `ArrayList<BlockPos>` and `ArrayList<Entity>` (views; a ref with no view is a Horizon bug: throw
  `IllegalStateException("Horizon: no view for entity " + ref)`), posts
  `EventHooks.onExplosionDetonate(level, explosion, entities, blocks)`, writes both lists back as refs (`Entity` →
  `ServerPlayer` ⇒ uuid ref, else id ref).
- case 2: `EventHooks.getExplosionKnockback(level, explosion, entity, new Vec3(x, y, z), blocks)`; writes the Vec3.
- Debug line (`HORIZON_JAVA_DEBUG`): `Horizon: server event ExplosionEvent.Start canceled=<b> at <x y z>`,
  `ExplosionEvent.Detonate blocks <n0>-><n1> entities <m0>-><m1>`.

## Rust (`G/explosion.rs`, new `G/java_explosions.rs` for the java-mods side, hooks in `G/java_hooks.rs`)
- `java_hooks.rs` pairs (not-arms return the input unchanged / false):
  - `java_hook_explosion_start(&mut self, blast: &JavaBlast) -> bool` (true = canceled);
  - `java_hook_explosion_detonate(&mut self, blast: &JavaBlast, blocks: &mut Vec<[i32; 3]>, entities: &mut Vec<BlastEntity>)`;
  - `java_hook_explosion_knockback(&mut self, blast: &JavaBlast, entity: BlastEntity, blocks: &[[i32; 3]], push: &mut [f64; 3])`.
  `JavaBlast { serial, center, radius, fire, interaction: u8, damage_type: &str }` (source and owner are read from
  `self.combat` inside the java side). `BlastEntity` is `enum { Entity(u32), Player(u128) }`, defined in `explosion.rs`.
  Each java arm returns at once unless `self.java_listens(JavaEvent::…)`.
- `server_explode_as`:
  1. After `native_explosion_prime`, inside the same server-only `if`: `if self.java_hook_explosion_start(&blast) { return Ok(()) }`.
     Nothing after it runs, as vanilla returns before `explode()`: no game event, no sound, no particles, no packet.
  2. After `native_explosion_detonate`: when `ExplosionDetonate` is listened, build the entity list with a new
     `fn explosion_entities(&self, center, radius) -> Vec<BlastEntity>`: every entity `explosion_hurt_entities` would
     consider, in the order it considers them (the player, the guests, then each kind in its loop order), selected by
     the same box (`center ± radius*2 + 1`, `Mth.floor` bounds as vanilla) and the same source exclusion
     (`explosion_source_named`). Call the hook; the targets vector becomes the returned positions.
  3. `explosion_hurt_entities` gets `only: Option<&[BlastEntity]>`: when `Some`, an entity not in it is skipped (neither
     hurt nor pushed). Pass `None` when Detonate isn't listened.
  4. At both `native_explosion_knockback` calls (`explosion.rs` ~488, ~746), right after the native hook, call
     `java_hook_explosion_knockback` with the same `push`.
- `JavaEvent::ExplosionStart`, `ExplosionDetonate`, `ExplosionKnockback` go into `SITED`.
- The bridged `ServerLevel.explode` (Java's own calls) reaches `server_explode_as`, so it fires the events too; nothing
  else to do for it.

## Test mod (`ChecksServerEvents`)
Listeners: Start cancels when the explosion's center is the marked point `area + (0.5, 1.5, 0.5)`; Detonate removes every
position holding `minecraft:white_wool` from `getAffectedBlocks()`; Knockback returns `Vec3.ZERO` for entities whose type
is `minecraft:pig`. Checks, each calling `level.explode(null, x, y, z, 3.0f, Level.ExplosionInteraction.TNT)`
from Java (the bridged path):
- `explosion_start_cancel_keeps_blocks`: dirt at the marked point's six neighbours; explode at the marked point; every
  dirt block is still there (without the cancel vanilla always breaks them: a ray starts at intensity >= 3 * 0.7 = 2.1
  and dirt takes (0.5 + 0.3) * 0.3 = 0.24 per step).
- `explosion_detonate_spares_listed_out_blocks`: white wool and dirt side by side at distance 1 from a second point;
  after the blast the wool is there and the dirt is air (wool, resistance 0.8, would break as surely as dirt).
- `explosion_knockback_zero_for_pig`: a pig and a cow 2 blocks away on opposite sides, both `NoAI`; after the blast
  the pig's `getDeltaMovement()` is `(0, 0, 0)` within 1e-9 and the cow's length is > 0.1.
- `explosion_events_share_one_object`: the Start and Detonate listeners record `getExplosion()`; same instance.

## Rust tests (`java_explosions.rs`)
- `a_detonate_reply_reads_blocks_and_entities`: a reply with 2 positions and refs `(0, 7)`, `(1, uuid)` reads back.
- `explosion_entities_keeps_the_hurt_order`: with a test `Game` fixture holding the player and two mobs in range and
  one out of range (copy the fixture style `explosion.rs` tests use), the list is player, mob A, mob B.

## Done bar
- The test-mod CHECKs pass in `java_mods.py bridges`; `unit`, `check` clean.
- `cargo test -p horizon-client --features java-mods --lib explosion`, clippy both ways, fmt.
- `python3 tools/parity.py explosion` (named harness through `/home/umceko/hz-poc/verify/parity.sh`) unchanged.
- M1 (`w13` scenario, step `tnt`): `ExplosionEvent$Start`, `$Detonate` counts equal Forge's; `ExplosionKnockbackEvent`
  equal (it is listened only if puzzleslib subscribes; else it is in `unlistened` on both).
- Append to `tools/scenarios/w13-events.json`: checks for `ExplosionEvent.Start` and `.Detonate` lines since `tnt`.

## Live test (lead, showcase jars, `HORIZON_JAVA_DEBUG=1`)
1. Creative, `/setblock ~3 ~ ~ minecraft:tnt`, `/setblock ~3 ~1 ~ minecraft:redstone_block`, wait 5 s. Expect the blast
   as without mods, and in the log one `ExplosionEvent.Start canceled=false` and one `ExplosionEvent.Detonate` line.
2. A creeper near stone (`/summon minecraft:creeper ~4 ~ ~`, survival, stand close): same two lines, blocks break.
3. No `a Java mod threw during ExplosionEvent` anywhere in the log.
