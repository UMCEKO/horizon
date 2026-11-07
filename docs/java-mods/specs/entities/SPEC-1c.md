# SPEC-1c: live, native-backed, write-through entity views

Design: `DESIGN.md` §2 "Views". Needs SPEC-0 (`entities.bin`: groups and writers), SPEC-1a (crossings) and SPEC-1b (choke points).

This is about 600 lines, over the usual size. It is deliberately one spec: deleting today's copy-back (`EntityViews.writeChanges`), making views live, and writing through must land together. Any one alone either loses mods' writes or keeps a copy-back. If it must be split, split only the writer bridges by family (§5's table rows), **after** this spec's core has landed with every row of the pack's writers present.

## Goal (final)
- Each non-player native entity has one Java view for its life. It is made at the join choke point (SPEC-1b `java_hook_entity_joining`, java-mods arm) and dropped at the leave choke point, its removal reason set raw. No events yet: SPEC-1d posts them.
- A view is **native-backed**: a `horizon$backed` flag on `Entity`. Entity bridges apply only to backed views.
- A view carries only `facts.reads(type)` groups, plus each written group (§5). They are refreshed raw at every crossing that hands the view to mod code:
  - an entity query (`EntityViews.query`, `queryId`): native returns ids plus group bytes;
  - after each write-through bridge on that view (the written group);
  - event subjects and route receivers, in later phases, through the same `refresh`.
- Every writer the analysis lists for the loaded mods is a bridge that writes through to native. `java_mods.py check` fails otherwise.
- `ServerLevel.addFreshEntity` from mod code goes to native at once (`level_add`, Fresh) and returns native's answer. The Java object becomes the live, backed view under native's id. `GameBridge.SPAWNED` and the effects' "spawned" list are deleted.
- Java-owned keys (`neoforge:attachments`, `NeoForgeData`) live in Java. At each native save of an entity with a view, native asks Java for them (`entities_save`) and writes them through `persist.raw`.
- `EntityViews.writeChanges` and Rust's "changed" and "removed" loops in `java_apply_effects` are deleted, and so are those two lists in the effects wire.

## Wire
**Group bytes**, written by Rust `write_groups(out, target, mask)` and read by Java `EntityViews.apply(entity, mask, in)`. Big-endian; groups in bit order; each present only if its bit is set:
- `position` (2): 3 `double` pos, `float` yRot, `float` xRot, `float` headYaw, `float` bodyYaw.
- `motion` (4): 3 `double`, `bool` onGround.
- `health` (8): `float` health, `float` absorption, `short` hurtTime, `short` deathTime. Living only; others write nothing.
- `effects` (16): `int n` × (`text id`, `int amplifier`, `int duration`, `bool ambient`, `bool visible`, `bool icon`).
- `equipment` (32): `int n` × (`byte slot`, `bytes stack` or empty).
- `fire` (64): `int remainingFireTicks`.
- `air` (128): `int air`.
- `flags` (256): `int` bit set: onGround 1, crouching 2, sprinting 4, swimming 8, silent 16, glowing 32, invisible 64, noGravity 128, inWater 256, inLava 512.
- `name` (512): `bool` has + tag (`Component` NBT), `int n` × `text tag`.
- `item` (1024): `bytes stack`, `short pickupDelay`, `int age`, `bool hasOwner` (+ 2 `long`), `bool hasTarget` (+ 2 `long`).
- `riding` (2048): `int vehicle` (0 none), `int n` × `int passenger`.
- `removal` (4096): `byte` reason + 1 (0 = not removed).

**Ops:**
- `entity_view` (Rust → Java, at join; java-mods arm of the join hook): `text dim`, `long time`, `int id`, `bool fromDisk`, `bytes tag` (the saved form: `saved` when given, else the kind's save encoder on the value), `int mask`. Reply: `bool` true (SPEC-1d returns the join result here), then the effects block.
- `entity_left` (Rust → Java, leave hook): `text dim`, `long time`, `int id`, `byte reason`. Reply: effects.
- `entities_save` (Rust → Java, from `persist.rs` `save_entities` before encoding): `text dim`, `int n` × `int id`. Reply: `int n` × (`int id`, `bytes` compound of the Java-owned keys, empty when none).
- **Bridged natives (Java → Rust):**
  - `EntityViews.groups(II)[B`: id and mask → group bytes;
  - `EntityViews.spawn([B)I`: saved form → native id, 0 when refused;
  - `EntityViews.query` and `queryId` (existing). Replies become `int n` × (`int id`, `bool known`, if not known `bytes tag`, `int mask`, group bytes). `known` is true when Java holds a view for that id; it always is after this spec, and false is a bug logged once.

**The effects block loses** its "spawned" list and the `EntityViews.writeChanges` lists. Java (`writeEffects`) and Rust (`java_apply_effects`) change together.

## Java (`boot/…`)
### 1. `mixin/EntityBackedMixin.java` (new, common)
```java
@Mixin(Entity.class) public abstract class EntityBackedMixin implements Backed {
    @Unique private boolean horizon$backed;
    public boolean horizon$backed() { return horizon$backed; }
    public void horizon$backed(boolean b) { horizon$backed = b; }
}
```
`Backed` is a new interface in `boot/`. Also add an `EntityRemovalAccess` accessor (`removalReason`), and accessors for the raw fields §Wire writes that aren't public:
- `Entity`: `remainingFireTicks`, `onGround`, `yRotO`, `xRotO`, `wasTouchingWater`, `wasInLava`;
- `LivingEntity`: `yBodyRot`, `yHeadRot` (setters exist), `absorptionAmount`, `hurtTime`, `deathTime`;
- `ItemEntity`: `pickupDelay`, `age`, `owner`, `target`.

Health and synched flags go through `entityData.set` of the vanilla accessors, which is a data write with no logic. `onSyncedDataUpdated` must not run for them: use `SynchedEntityData.DataItem.setValue` through an accessor, without `markDirty`. List the mixins in `horizon_java.mixins.json`.

### 2. `EntityViews.java`
- **State:**
  - `LIVE: IdentityHashMap<ServerLevel, Int2ObjectOpenHashMap<Entity>>`;
  - `static boolean backed(Object o)`: `o instanceof Backed b && b.horizon$backed()`. This is the bridge guard.
- **Joining** (`join(level, id, fromDisk, tag, mask)`, op `entity_view`):
  - `EntityType.create(TagValueInput…, level, new EntitySpawnRequest(fromDisk ? LOAD : …, true))`. Use the 26.3 `EntitySpawnReason` matching: `LOAD` for both; the join event (1d) carries `fromDisk`.
  - `setId(id)`, `backed(true)`, `apply(mask, groups)` (none needed: the tag has them), put in `LIVE`.
  - Attachments deserialize from the tag here, the only time.
- **Leaving** (op `entity_left`): the view is removed from `LIVE` and `horizon$removalReason(reason)` is set raw. `backed` stays `true`, so a mod keeping the object still can't run vanilla bodies on it. A later bridged call on a removed view finds no native entity, and its bridge answers as vanilla does for a removed entity (§4 per row: `hurtServer` false, setters no-op).
- **`read(listed)`:** for each entry, `LIVE.get(id)`. If present, `apply(mask, in)`. If absent (`known` false), create it as `join` does and log once. Return the not-removed ones.
- **Spawn** (`GameBridge.spawn(entity)`, from `ServerLevelMixin.horizon$addFreshEntity`):
  - `CompoundTag tag = save(entity)` (`entity.save(output)`, so the `id` is included);
  - `int id = spawn(bytes)` (bridged), which re-enters Rust's `level_add` (Fresh) → the join hook → `entity_view`… The spawning entity is not in `LIVE` yet, so Rust must not ask Java to make another object: the native side of `spawn` passes the hook a flag (§Rust 3), and `join` adopts `entity` instead of creating one;
  - `id == 0` → `return false` (refused);
  - else `entity.setId(id)`, `backed(true)`, `LIVE.put(id, entity)`, `return true`.
- **Delete:** `writeChanges`, `VIEWS`, `abandon`'s view clearing (keep an empty `abandon` only if `GameBridge` needs the name), and `GameBridge.SPAWNED`, `spawned()`, the spawned loop in `writeEffects`, and the call in `within`/`get`.
- **`save(level, ids)`** (op `entities_save`): per id, `view.getPersistentData()` copied under `NeoForgeData` if not empty, plus `serializeAttachments(output)` into the same compound (`AttachmentHolder.serializeAttachments(ValueOutput)`; `TagValueOutput.createWithContext`). Reply as in §Wire.
- **`reset()`** clears `LIVE`; called from `GameBridge.closeWorld`.

### 3. `GameBridge.java`
- Cases `entity_view`, `entity_left`, `entities_save`.
- `spawn` as above.
- `writeEffects` keeps sounds only (plus what other families add later).

### 4. `BridgeProcessor` (plugin) and `H/bridge.rs`: a predicate guard
- A guard starting with `@` is `@owner.method`, a static `(Ljava/lang/Object;)Z`. The head emits `ALOAD 0; INVOKESTATIC owner.method; IFEQ body` in place of `INSTANCEOF`.
- `Bridge::holder()` returns `owner` for `@` guards: natives go on `EntityViews`.
- **The receiver argument:** a new `Arg` type `Ent(u32)`, `RAW "I"`, made by `Flat.raw(Entity e) { return e.getId(); }`. `BridgeProcessor` already flattens `this` first when the entry has one more argument than the method.
- `java_mods.py check` learns `@` guards and `Ent`.

## Rust
### 1. New `G/java_entities.rs` (`cfg(feature = "java-mods")`)
- `java_entity_joining(&mut self, e, origin, saved) -> bool`, the java-mods arm of SPEC-1b's hook:
  - only when `java_level_running()`;
  - builds the tag (`saved`, or the kind's save encoder on `e`: the same functions `persist.rs` `save_entities` calls per kind, with `uuid_for(id)` and `persist.raw.get(id)`);
  - `mask = facts.reads(type) | written_groups(type)`;
  - sends `entity_view`, unless `self.java_adopting == Some(id)` (§3), applies the effects, returns the reply's bool.
- `java_entity_left(kind, id, why)`: sends `entity_left`.
- `write_groups(out, target, mask)`: the encoder of §Wire, from the native structs. Its unit test writes every group for a mob and reads it back with a Rust mirror of Java's reader.
- `java_entities_save(ids) -> FxHashMap<u32, Tag>`, called by `persist.rs` `save_entities` (a `java_hook_entities_save` cfg pair) before it encodes. Each answer's keys go into `persist.raw[id]`, replacing those keys, so `with_raw` writes them.

### 2. `G/java_bridges/entities.rs`: the queries
`entities_query` and `entities_query_id` now write ids, `known` and group bytes (the type's mask) in place of saved forms. `java_write_entity` stays only for `entity_view`.

### 3. `G/java_bridges/entities.rs`: `EntityViews.spawn([B)I`
- Decode the tag; build the `NewEntity` the way `spawn_loaded_entities` does for one tag. Factor that per-tag body out of `persist.rs` into `fn new_entity_from_nbt(&mut self, tag) -> Option<NewEntity>`, which both use.
- `self.java_adopting = Some(id)`, then `level_add(e, Fresh, None)` (the hook sends `entity_view` with an `adopt` flag, so Java's `join` adopts the spawning object and posts the join event in 1d). Then `java_adopting = None`.
- Return the id, or 0.

### 4. The writers: `G/java_bridges/entities.rs`, one entry each
`if "@dev/horizonlab/javamods/boot/EntityViews.backed"`; receiver `Ent`. Each body acts on the native entity, then `game.java_refresh_entity(id, GROUP)` sends op `entity_refresh` (`int id`, `int mask`, group bytes), which Java applies raw. It is the `java_refresh_player` pattern. A missing native entity returns vanilla's answer for a removed one.

| Java method (descriptor) | Native body | Refresh |
|---|---|---|
| `Entity.discard()V` | `level_remove(kind, id, Discarded)` | `removal` |
| `Entity.remove(Lnet/minecraft/world/entity/Entity$RemovalReason;)V` | `level_remove(kind, id, reason)` (new `Arg` from `Flat.raw(RemovalReason)` → ordinal) | `removal` |
| `Entity.kill(Lnet/minecraft/server/level/ServerLevel;)V` | as `/kill` (`kill_target`) | `health`, `removal` |
| `Entity.hurtServer(ServerLevel, DamageSource, F)Z`, `Entity.hurt(DamageSource, F)Z`, and the overrides in `LivingEntity` and `ItemEntity` (one entry per declaring class, since the guard is per method) | `java_hurt_target(t, source, amount)`: `cmd_damage`'s per-target body, factored out of `commands/world_cmds.rs` 185–215 (mobs: `damage` + `resolve_player_responsible`; items: the explosion item hurt, factored; boats: `hurt_boat`; carts: `hurt_cart`) | `health`, `removal` |
| `Entity.setDeltaMovement(Lnet/minecraft/world/phys/Vec3;)V`, `(DDD)V`; `Entity.addDeltaMovement(Vec3)V`; `Entity.push(DDD)V` | motion `=` or `+=`; `needs_sync`/`hurt_marked` as the native knockback sets it | `motion` |
| `Entity.setPos(DDD)V`, `Entity.teleportTo(DDD)V`, `Entity.snapTo(DDDFF)V` | position, previous position, box | `position` |
| `Entity.teleport(Lnet/minecraft/world/entity/TeleportTransition;)Lnet/minecraft/world/entity/Entity;` | same level: position, rotation and motion as the transition says. Another level: the native entity portal travel (`entity_portals`' function), which removes with `ChangedDimension` and adds a fresh copy. Returns the new view (`Flat.toEntity(int id)` from `LIVE`) | `position`, `motion`, `removal` |
| `LivingEntity.knockback(DDD)V` | `creatures.rs:2716`'s knockback | `motion` |
| `Entity.igniteForSeconds(F)V`, `Entity.setRemainingFireTicks(I)V` | fire ticks | `fire` |
| `LivingEntity.setHealth(F)V` | health, clamped as vanilla | `health` |
| `LivingEntity.setItemSlot(EquipmentSlot, ItemStack)V` | the mob's equipment slot | `equipment` |
| `ItemEntity.setItem(ItemStack)V`, `setPickUpDelay(I)V`, `setNoPickUpDelay()V`, `setTarget(UUID)V` | the item entity's fields | `item` |
| `Entity.startRiding(Entity, Z, Z)Z` (26.3 descriptor; `javap` it), `Entity.stopRiding()V` | `passengers.rs` `mount_mob`; the kind's dismount (`boats.rs:665`, `minecarts.rs:966`, `mounts.rs:468`) | `riding`, `position` |
| `Entity.setSilent(Z)V` | silent flag | `flags` |
| `Sheep.setColor(DyeColor)V` | the sheep's color | `flags` (the color is bits 16–19 of the flags int; Java writes it to `Sheep`'s synched wool item raw) |
| `Entity.setCustomName(Component)V`, `Entity.addTag(String)Z`, `Entity.removeTag(String)Z` | name and tags | `name` |

Rows come from `entity-facts.json` `writes` for the pack plus the test mod. A writer the facts list that isn't in this table must be added here or skipped with a reason in `tools/java_mods_skips/entities.tsv`. `java-owned` (attachments, persistent data) is the only allowed reason; anything else fails `check`.

### 5. Deleted
- In `java_mods.rs` `java_apply_effects`: the spawned, changed and removed loops.
- `apply_entity_nbt` is no longer called from Java paths.

## Test mod
Extend `ChecksEntities` (the `run`, `later`, `afterChange` flow of `java_mods.py bridges`):
- **`run`:**
  - `spawn_is_native_at_once`: after `addFreshEntity(drop)`, `drop.getId() > 0` and `level.getEntity(drop.getId()) == drop`;
  - `spawned_mob_is_the_live_view` (in `later`, the found chicken `==` the one from `run`, kept in a static).
- **`later`:**
  - `set_health_writes_through`: `chicken.setHealth(1.5F)`; then `/data get entity` of it through `Checks.command`, or the server's own `getEntity` refresh via `queryId`, shows 1.5;
  - `discard_writes_through`: discard a spawned item; `level.getEntity(id) == null` within the same call.
- **`afterChange`** (second session): the existing `mob_change_written_back` (health 1.5) and the custom name, now through bridges; `persistent_data_saved` (`getIntOr("horizon_java_test", 0) == 7`, set in `later`).
- **A delta-movement check:** in `later`, `chicken.setDeltaMovement(0, 0.5, 0)` → `chicken.getDeltaMovement().y == 0.5` read back (refreshed from native), and native's mob has `velocity.y == 0.5` via `Checks.command("data get entity @e[type=chicken,limit=1] Motion")`.

## Tests to run
- `cargo test -p horizon-client --features java-mods --bins java_entities` (group codec round-trip, save merge into `persist.raw`).
- `cargo test -p horizon-java-host` (the `Ent` argument and predicate-guard table lines).
- `python3 tools/java_mods.py build && python3 tools/java_mods.py check`: it fails if any analysed writer lacks a bridge or a skip.
- `python3 tools/java_mods.py bridges`: all CHECKs, old and new, in both sessions.
- `python3 tools/parity.py server_entities dedicated_server`.

## How the parent tests it
- `bridges` all ok.
- A Mekanism + AE2 world: AE2's annihilation plane picks up items, so `ItemEntity.discard` writes through and the items vanish without duplicating. A Mekanism electric bow arrow (`AbstractArrow.setBaseDamage` on a Java-spawned arrow) lands.

**User steps:** throw 10 cobblestone in front of an AE2 annihilation plane on a powered network. All 10 enter the network; none stay on the ground.
