# SPEC-1c2a: TNT and lightning as native kinds, their writers, item hurt, the check's two false reports

Design: `DESIGN.md` §2 (views, write-through), §0 decision 1. Base: worktree `agent-aef92fbe041188edb` at `539668997` (SPEC-1c). Needs SPEC-1b's `level_add`/`level_remove` and SPEC-1c's views. Then `SPEC-1c2c.md`, `SPEC-1c2d.md`, `SPEC-1c2b.md`, in that order. 1c merges only after all four, when `java_mods.py check --analysis /tmp/hz-poc/agents/spec0/out/entity-facts.json` prints 0 problems.

Closes:
- `PrimedTnt.setFuse(I)V`, `PrimedTnt.setBlockState(BlockState)V`, `LightningBolt.setDamage(F)V`, `LightningBolt.setVisualOnly(Z)V`, `Entity.thunderHit(ServerLevel, LightningBolt)V`;
- `hurt` on an item entity skipping `isInvulnerableToBase`;
- the two `server.rs` reports.

Who calls these (bytecode of the pack's jars, `/tmp/hz-poc/agents/spec1c2/callers.py`):
- `setFuse`: `TinyTNTPrimedEntity.<init>/tick/readSpawnData`, `TinyTNTBlock.wasExploded` (ae2); `BlockObsidianTNT.wasExploded`, `EntityObsidianTNT.setData` (mekanismadditions).
- `LightningBolt.setDamage`, `setVisualOnly`, `Entity.thunderHit`: Mekanism `SPSMultiblockData.kill`.

Most receivers are mod types (phase 6) or unspawned objects. The bridges must still be exact on any backed view: a vanilla `PrimedTnt` or `LightningBolt` that mod code reaches through a query, and phase 6's subclasses.

## 1. TNT and lightning become kinds
Today both live outside SPEC-1b: `self.tnts: Vec<tnt::PrimedTnt>` and `self.bolts: Vec<lightning::Bolt>` (`game.rs` 2247, 2184). Their ids are 0 until `lan_entities.rs` `give` (line 451, 460) hands one out at first tracking. They never join, so they never get a view.

### 1.1 `entity_life.rs`
- Append `Tnt` and `Bolt` to `Kind`, after `Dragon`.
- Add `NewEntity::Tnt(tnt::PrimedTnt)` and `NewEntity::Bolt(lightning::Bolt)`, with their `kind()` and `id()` arms.
- `level_add` pushes to `self.tnts` / `self.bolts`.
- `level_remove`: `Kind::Tnt => take!(self.tnts)`; `Kind::Bolt => take!(self.bolts, crossing::LentKinds::BOLTS)`.
- `level_remove_all` and `level_ids` get both arms.
- `java_entities.rs` `ALL_KINDS` grows to 15.
- Add a `pub removed: bool` field to `Bolt`, which the macro needs.

### 1.2 Ids at construction
Vanilla's `Entity` constructor takes `ENTITY_COUNTER.incrementAndGet()`.
- Every constructor site does `self.next_entity += 1; id = self.next_entity` before building: `tnt.rs` `add_primed_tnt`, `persist.rs:359`, `lightning.rs` `spawn_bolt`.
- `spawn_remote_bolt` and `remote_projectiles.rs:265` keep the server's id.
- Delete the `tnts` and `bolts` lines from `give` in `lan_entities.rs`. The `filter(|t| t.id != 0)` in `lan_projectile_states` stays harmless; drop it for these two.

### 1.3 Sites through `level_add` / `level_remove`
| Site | Becomes |
|---|---|
| `tnt.rs` `add_primed_tnt` push | `level_add(NewEntity::Tnt(t), Fresh, None)`; returns `Option<u32>` |
| `persist.rs:359` (chunk load) | `level_add(…, origin, Some(&tag))`, as the other kinds there |
| `remote_projectiles.rs:265`, `:539`, `:706`, `:722` | `level_add` / `level_remove(Kind::Tnt, id, Discarded)` / `level_remove_all(Kind::Tnt, UnloadedToChunk)`, as `remote.rs` does for mobs (lines 506–513) |
| `tnt.rs` `tick_tnt`: `self.tnts.remove(i)` before `server_explode` | `level_remove(Kind::Tnt, id, Discarded)`, then the explosion. Vanilla: `discard()` then `explode()` |
| `lightning.rs` `spawn_bolt`, `spawn_remote_bolt` | `level_add(NewEntity::Bolt(b), Fresh, None)`; `spawn_remote_bolt`'s `retain` becomes `level_remove(Kind::Bolt, id, Discarded)` |
| `tick_bolts` `return false` (flashes spent: vanilla `discard()`) | set `b.removed = true`, then `left_in_loop(&mut [Lent::Bolts(&mut bolts)], Kind::Bolt, b.id, Discarded)` after the borrow ends (restructure `retain_mut` into an index loop over the taken vector, then `retain(!removed)` at the end, as `explosion.rs` 540–546 does for items) |
| `remote_objects.rs:425`, `game.rs:10634` and `:10905` (`clear`) | `level_remove` / `level_remove_all(…, UnloadedToChunk)` |
| `dispense_fixture.rs:145` | unchanged (fixture) |
| `trident.rs:257` `self.bolts.last_mut()` | look the bolt up by the id `spawn_bolt` now returns |

`tick_tnt` becomes re-entrant, as vanilla's `EntityTickList` is: entities added during the pass don't tick, removed ones are skipped.
- Take `let ids: Vec<u32>` of `self.tnts` at the start.
- For each id, find its index now; `continue` if it is gone.
- Delete the `count`/`i` bookkeeping.

**`crossing.rs`:** `Lent::Bolts(&'a mut Vec<lightning::Bolt>)`, `LentKinds::BOLTS = 32`, its `kind` and `swap_lent` arms. `tick_bolts` lends it at `left_in_loop` and at every `thunder_hit` call, since §3 crosses into Java there: `self.lend(&mut [Lent::Bolts(&mut bolts)], |g| g.thunder_hit(b))`, with the bolt taken out by index around it.

**`tests/test_entity_lifecycle_sites.py`:** add `tnts|bolts` to `VECTORS`. Allowlist, with reasons:
- `("explosion.rs", "explosion_hurt_entities")`: knockback only, no crossing between take and restore;
- `("wind.rs", <fn at 397>)`: the same;
- `("lightning.rs", "tick_bolts")`: restores the lent vector.

### 1.4 State the kinds gain (vanilla fields native lacks)
`PrimedTnt` (vanilla `PrimedTnt.java`):
- `block_state: BlockStateId`, default `minecraft:tnt`'s default state;
- `explosion_power: f32 = 4.0` (`readAdditionalSaveData` clamps to 0..=128);
- `owner: Option<[i32; 4]>` (UUID, `EntityReference`);
- `used_portal: bool` (not saved).

Uses:
- `tick_tnt` passes `explosion_power` to `server_explode` in place of `4.0`.
- `tnt_geometry` draws `block_state`'s model in place of `default_state("minecraft:tnt")`.

`Bolt` (vanilla `LightningBolt.java`, plus NeoForge's `damage` field: `javap -p` of the patched jar shows `private float damage`, `setDamage(F)`, `getDamage()F`, and patched `Entity.thunderHit` calls `getDamage()` where vanilla passes `5.0F`):
- `damage: f32 = 5.0`;
- `uuid: [i32; 4]` (`uuid_for(id)`), which §3's mooshroom and copper golem need.

**Save and load.** `entity_nbt.rs` gets `tnt_to_nbt(t, uuid, raw)` and `tnt_from_nbt(tag)`: `fuse` short, `block_state` (`BlockState.CODEC`: `Name` + `Properties`), `explosion_power` only when ≠ 4, `owner` as an int-array UUID.
- `persist.rs:353` uses `tnt_from_nbt`.
- `save_entities` writes TNT like the other kinds; vanilla saves primed TNT.
- `LightningBolt` saves nothing of its own, and `EntityType.LIGHTNING_BOLT` is not serializable (`canSerialize` false). Bolts are never saved, and the base tag is only for views.

**Tracking** (`lan_projectile_states`):
- TNT also sends `(9, serializer::BLOCK_STATE, Value::State(…))` when `block_state` isn't the default;
- `remote_projectiles.rs:432` reads index 9 into `t.block_state`.

### 1.5 Commands and views see them
- `commands::Target` gains `Tnt(u32)` and `Bolt(u32)`. Handle them at the 15 `Target::Crystal` sites across `java_entities.rs`, `java_mods.rs`, `scoreboard.rs`, `lan_camera.rs`, `commands/data.rs`, `commands.rs`:
  - `level_candidates` lists them (vanilla `@e` includes both);
  - `target_position`, `target_type` (`minecraft:tnt`, `minecraft:lightning_bolt`), `entity_nbt` (TNT: `tnt_to_nbt`; bolt: the base tag), `kill_target` (`level_remove(…, Killed)`), `kind_of`, `java_target_id`;
  - `teleport_to_in_level`: position and previous for TNT; a bolt moves its `at`.
- `java_new_entity_tag` arms: TNT `tnt_to_nbt`, `"minecraft:tnt"`; bolt base tag, `"minecraft:lightning_bolt"`.
- `spawn_loaded_entities` gets a `"minecraft:lightning_bolt"` arm, for `/summon` with NBT and Java spawns:
  - `spawn_bolt_at(pos, visual_only: false)` without the game event;
  - vanilla's constructor draws `seed` and `flashes` from the entity's own random; for Java spawns §2.3 overwrites them.
  - `commands.rs:2085`'s `"lightning_bolt"` arm stays.

## 2. The `species` group on the wire
`species` is bit 13 (8192) of `entity_facts::GROUPS`. Add `SPECIES` to `java_entities.rs` and to `write_groups`. It is present only if its bit is set, and its layout depends on the view's shape:

| Shape | Bytes |
|---|---|
| TNT | `int fuse`, `int javaState` (`StateMap.to_java(block_state)`), `float explosionPower`, `bool hasOwner` (+ 2 `long`) |
| Bolt | `float damage`, `bool visualOnly`, `int life`, `int flashes`, `long seed` |
| any other | nothing (SPEC-1c2b adds arrows and projectiles) |

- `Shape` gains `species: SpeciesShape { None, Tnt, Bolt }`, from the kind in `java_shape`, or from `f.base(type_id)` when the facts name `PrimedTnt` / `LightningBolt` as a type's base class (SPEC-0 `entity-types.bin`).
- `java_shape`'s mask always includes `SPECIES` for those two shapes, even when `reads` doesn't, because §3's bridges refresh it.

**Java, `EntityViews.apply` (raw, no setter: the setters are bridged):**
- TNT: `entityData` `DataItem.setValue` for `DATA_FUSE_ID` and `DATA_BLOCK_STATE_ID` (`Flat.toBlockState`), the way 1c writes health; `PrimedTntAccess.explosionPower`, `owner` (`EntityReference.of(uuid)`).
- Bolt: `LightningBoltAccess` mixin (new, common) with accessors for `visualOnly`, `damage`, `life`, `flashes`; `seed` is public.
- List the accessors in `horizon_java.mixins.json`.

**Java, `EntityViews.write(entity, mask, out)` (new):** the mirror of `apply`, for `SPECIES` only. §2.3 uses it.

### 2.3 Spawn carries what the saved form lacks
- `EntityViews.spawn([B)I` becomes `spawn([B[B)I`: the saved form, then `write(entity, SPECIES)`, so `LightningBolt.setDamage`/`setVisualOnly` called before `addFreshEntity` reach native. Vanilla saves neither.
- `entities_spawn(game, saved: Bytes, species: Bytes)` → `java_spawn_entity(&saved, &species)`.
- When the tag's id is `minecraft:lightning_bolt`, it builds the `Bolt` from the tag's `Pos` plus the species bytes, then `level_add(Bolt, Fresh, None)`, and returns its id. Other ids keep 1c's path. For TNT the tag already carries everything.

## 3. The bridges (`G/java_bridges/entities.rs`)
- Guard `if "@dev/horizonlab/javamods/boot/EntityViews.backed"`, receiver `id: Ent`.
- Each body finds the native entity by id in its vector (lent or not); when it is absent it does nothing (vanilla on a removed entity).
- Then `after(game, id, SPECIES)`.

| Vanilla method | Native body (write-through) | Refresh |
|---|---|---|
| `PrimedTnt.setFuse(I)V` (`entityData.set(DATA_FUSE_ID)`) | `t.fuse = v` | `SPECIES` |
| `PrimedTnt.setBlockState(BlockState)V` | `t.block_state = STATES.to_horizon(v)` (`Arg State`) | `SPECIES` |
| `LightningBolt.setVisualOnly(Z)V` | `b.visual_only = v` | `SPECIES` |
| `LightningBolt.setDamage(F)V` (NeoForge) | `b.damage = v` | `SPECIES` |
| `Entity.thunderHit(ServerLevel, LightningBolt)V` | `thunder_hit_entity(target, &hit)`, §3.1 | the groups it changed: `HEALTH \| FIRE \| REMOVAL \| FLAGS \| SPECIES` |

A player receiver gets its own entry with the same method and `if "net/minecraft/server/level/ServerPlayer"`, in `player.rs`. Its body is §3.1's player arm on the acting player, then `java_refresh_player()`. Both entries chain on one method, as `snapTo` already does in `player.rs` and `entities.rs`.

**The bolt argument.** Add `Flat.raw(LightningBolt) -> byte[]`: `bool backed`, `int id` (0 if not backed), `float damage`, `bool visualOnly`, 2 `long` UUID, `double` x, y, z. Rust reads it as `Bytes` into `BoltHit { damage, uuid, at }`. The SPS bolt is never spawned, so its id means nothing to native; its UUID is what vanilla compares.

### 3.1 `lightning.rs`: one entity's `thunderHit`, as vanilla dispatches it
Factor the per-entity work out of `thunder_hit(b)` into `fn thunder_hit_entity(&mut self, t: Target, hit: &BoltHit, events: &mut Vec<MobEvent>) -> bool` (true if it changed). `thunder_hit` builds `BoltHit` from its bolt and calls it per entity, in the order vanilla's `getEntities(box)` returns them (the current order: mobs, cushions, players, items).

The arms follow vanilla's overrides exactly:
- **Base `Entity.thunderHit`:**
  1. `remainingFireTicks += 1`;
  2. if it is now 0, `igniteForSeconds(8)`;
  3. `hurt(lightningBolt source, hit.damage)` (NeoForge's patched body calls `hurt`).

  Use `hit.damage` where native has `5.0` now.
- **Pig** (not peaceful): zombified piglin conversion, as now. **Villager** (not peaceful): witch, as now. **Turtle:** `hurtServer(MAX)`. **Creeper:** charged, then base.
- **MushroomCow:** compares `hit.uuid` with a new `Mob.extra.last_bolt: Option<[i32; 4]>` (vanilla `lastLightningBoltUUID`). This replaces `b.converted` for mooshrooms; the sound and variant flip are as now.
- **CopperGolem:** base first, then the `last_bolt` check and weather step. Vanilla calls `super.thunderHit` first; native does it last today. Fix the order.
- **ArmorStand, item frames, paintings** (`BlockAttachedEntity`): nothing (vanilla overrides with empty bodies).
- **Cushion:** `break_cushion`, as now.
- **ItemEntity:** base. Its `hurt` is §4's `hurt_item_entity` with the lightning source, which replaces the `netherite` name test and the `burnt` list.
  - `spared` stays bolt-local: a cushion's dropped items are not hit by the same bolt in its first 20 ticks.
  - Keep `hit_items` for the advancement trigger.
- **Boats, carts:** base: fire, then their `hurt_boat` / `hurt_cart` with the source.
- **Players:** the existing `strike` closure body, with `hit.damage`.

`b.converted` and `b.hit_items` stay on `Bolt` only for what isn't per-entity state (the trigger lists).

## 4. Item hurt: `ItemEntity.hurtServer`, ported (`java_entities.rs`)
`java_hurt_target`'s `Target::Item` arm does `item.health -= amount as i32`, with no immunity check. Replace it with a new `fn hurt_item_entity(&mut self, id: u32, source: &Source, amount: f32) -> bool` in `game/explosion.rs` beside the item damage it already has, following `ItemEntity.java:287` line by line:
1. `isInvulnerableToBase(source)`: `horizon_world::damage_immunity::base_immunity` through a small `ImmunityWorld` adapter over the item:
   - `removed`;
   - `permanently_invulnerable` = a new `ItemEntity.invulnerable: bool`, read from and written to `"Invulnerable"` in `entity_nbt.rs`'s item load and `item_entity_to_nbt`, so native owns it;
   - `temporarily_invulnerable` false;
   - `tag` = the source's damage-type tags;
   - `creative_source`;
   - `fire_immune` = `!can_be_hurt_by(stack, "minecraft:in_fire")`;
   - `fall_immune` = the item type in `#fall_damage_immune`.

   True → return false.
2. `!rules.bool("mob_griefing") && source.causing is a Mob` → false.
3. `!can_be_hurt_by(stack, source.kind)` → false. Add `Items::can_be_hurt_by(stack, damage_type, in_tag)`: the `damage_resistant` component's `types` (a tag or a list) against the type, with the damage-type tag table native already loads; the item's legacy `fire_resistant` for `#is_fire`. `resists_fire` and `resists_cactus` become callers of it.
4. `markHurt`: the item's velocity sync flag, as knockback sets it.
5. `health = (health as f32 - amount) as i32`: vanilla subtracts in float and then truncates.
6. Game event `minecraft:entity_damage` at the item, with the source's entity as actor.
7. `health <= 0` → `Item.onDestroyed` (run `port_ledger.py where Item.onDestroyed`; port `ItemUtils.onContainerDestroyed` for container and bundle contents if the ledger has none), then `level_remove(Item, id, Discarded)`.
8. Return true.

**The source.** `Flat.raw(DamageSource)` becomes `byte[]`: `text type`, `int causing` (0 none), `int direct`, `bool creativePlayer`, `bool hasPos` (+ 3 `double`).
- New `Arg` `Source` in `H/bridge.rs` (`RAW "[B"`), decoded to `struct Source { kind, causing, direct, creative, at }`.
- Every bridge taking `source: Text` switches to `source: Source`: `entity_hurt`, `living_hurt_server`, `item_hurt_server`, `player_hurt`, and any other `grep -n "source: Text"` finds.
- The mob arm of `java_hurt_target` sets `Damage.attacker` from `causing` (mob id → `Who::Mob`, the acting player → `Who::Player`) and `from` from `at`, where it passes `None` today.

## 5. The two `server.rs` reports (`tools/java_mods.py` `check_bridges`)
Neither is a wrong bridge.
- **`world_data_level_name`:** `Text`'s `RAW` is `Ljava/lang/String;`, the descriptor's return, and `BridgeProcessor` (line 126) adds no converter when they are equal. The check requires `Flat.toString` anyway. Fix the check: `elif ret[0] in 'L[' and ret != mine_ret:` before asking for `to{simple}`, mirroring the argument side's `if java == raw: continue`.
- **`integrated_profile_permissions`:** `IntegratedServer` is a client class. The check loads only the server-patched and universal jars. `javap -s` of the patched merged jar shows `getProfilePermissions(Lnet/minecraft/server/players/NameAndId;)Lnet/minecraft/server/permissions/LevelBasedPermissionSet;`, and `Flat.toLevelBasedPermissionSet(I)` exists.
  - Fix the check: also load `oracle.client()`'s `libraries/net/neoforged/minecraft-client-patched/<ver>/minecraft-client-patched-<ver>.jar` (present at `.local/neoforge-oracle/client-26.3.0.101/`), adding only classes the server set lacks.
  - Run the check with `HORIZON_NEOFORGE=26.3.0.101`.
- Add a `tests/test_java_mods_check.py` case for each, on a synthetic class table (equal String return passes; a class only in the client jar resolves).

## Test mod (`ChecksEntities`)
Read-backs go through `((ServerLevel) level).getEntity(id)`, which refreshes from native (1c `queryId`).

**`run`:**
- `tnt_spawns_as_a_view`: `var tnt = new PrimedTnt(level, x, y, z, null); tnt.setFuse(200); level.addFreshEntity(tnt)` → `tnt.getId() > 0 && getEntity(id) == tnt && tnt.getFuse() == 200`.
- `tnt_fuse_writes_through`: `tnt.setFuse(150)` → `getEntity(id)` → `getFuse() == 150`. In `later`, `getFuse() < 150`: native ticked it from 150.
- `tnt_block_state_writes_through`: `tnt.setBlockState(Blocks.OBSIDIAN.defaultBlockState())` → read back `getBlockState().is(Blocks.OBSIDIAN)`.
- `bolt_spawn_keeps_damage_and_visual`: `var b = EntityType.LIGHTNING_BOLT.create(level, SPAWN); b.setDamage(0F); b.setVisualOnly(true); b.snapTo(…); level.addFreshEntity(b)` → `getEntity(b.getId()) == b`. Then, on a chicken 1 block from it, the chicken's health is unchanged after `later` (visual-only and zero damage).
- `bolt_setters_write_through`: `b.setDamage(2F)` → read back: `((LightningBoltAccess) b).damage() == 2F` (test-mod side through a public getter `getDamage()`).
- `thunder_hit_uses_bolt_damage`: an unspawned bolt with `setDamage(3F)`; `chicken.thunderHit(level, bolt)` → read back the chicken: `health == max - 3` and `getRemainingFireTicks() > 0`.
- `thunder_hit_mooshroom_once_per_bolt`: a mooshroom, the same unspawned bolt twice → variant flipped once.
- `item_hurt_respects_invulnerable`: an item `setInvulnerable(true)` before spawn; `item.hurtServer(level, damageSources().generic(), 10)` → false, and it is still in the level.
- `item_hurt_truncates_like_vanilla`: a fresh item (health 5); each `hurtServer(…, 0.5F)` returns true and takes 1 (`(int)(5 - 0.5) = 4`). The item still exists after 4 hits and is gone after the 5th. Native's `amount as i32` took nothing.

## Tests to run
- `cargo test -p horizon-client --features java-mods --bins java_entities` (new: the species group round-trips for TNT and bolt; `tnt_to_nbt` ↔ `tnt_from_nbt`).
- `cargo test -p horizon-client --lib --bins game_tests` (new `a_tnt_joins_and_leaves_with_its_reason`: lit TNT joins `Fresh`, leaves `Discarded` before the explosion's first block change; `a_bolt_leaves_when_its_flashes_end`).
- `python3 -m unittest tests.test_entity_lifecycle_sites tests.test_java_mods_check`.
- `python3 tools/java_mods.py build && HORIZON_NEOFORGE=26.3.0.101 python3 tools/java_mods.py check --analysis /tmp/hz-poc/agents/spec0/out/entity-facts.json`: the 5 TNT and lightning lines and both `server.rs` lines are gone.
- `python3 tools/java_mods.py bridges`.
- Harnesses: `python3 tools/parity.py damage_immunity damage_source server_entities dedicated_server`, plus whatever `parity.py --changed-files` lists for `tnt.rs`, `lightning.rs`, `explosion.rs`.

## Scenario (parent, `hz_scenario`)
`tools/scenarios/tnt_lightning.json`:
1. Build a 9×9 stone floor at y=100 (forceload).
2. `tp` there, then a `near` check.
3. `/summon tnt ~3 ~1 ~ {fuse:200s}`, then `/data get entity @e[type=tnt,limit=1] fuse`, which is less than 200 after 20 ticks.
4. `/summon lightning_bolt ~ ~1 ~3`; `/execute if entity @e[type=lightning_bolt]` succeeds in the same tick.
5. Save, quit, relaunch: the TNT is back with its fuse. Vanilla saves primed TNT; native didn't.

**User step:** light TNT with flint and steel on a Mekanism world. It blinks and explodes after 4 s, as before.
