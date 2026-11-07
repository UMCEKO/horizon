# SPEC-8: experience orbs (Clumps), and field writes from Java bodies

Design: `DESIGN.md` §2.6 row 8. Needs SPEC-3, SPEC-7 (orb save data), W9 2-9 (orbs have turns: `ExperienceOrb.tick`'s merge scan inside the orb's turn) and 3-1 (`Recv::Entity`). Size: ~400 changed lines.

Paths as SPEC-2. Read first: vanilla `ExperienceOrb.java` (`tick`, `scanForMerges`, `canMerge` both, `merge`, `tryMergeToExisting`, `award`, `playerTouch`, `repairPlayerItems`); `G/../game.rs` `spawn_orbs`, `award_orb`, `orb_merges`, `merge_orbs`, `tick_experience`, and W9 2-9's orb turn; `horizon-client/src/entities.rs` `ExperienceOrb::can_merge` (~544); `G/lan_actions.rs` `guest_orbs`; `P/BridgeProcessor.java`; `tools/java_mods.py` `unbridged_writers`.

Comments in code: one line at most.

## Keys (all clumps)
| Key | Kind | Native seam (`Game` fn, gate first) | Receiver, args |
|---|---|---|---|
| `ExperienceOrb#tryMergeToExisting(Lnet/minecraft/server/level/ServerLevel;Lnet/minecraft/world/phys/Vec3;I)Z` | edge HEAD, static | `award_orb` becomes `try_merge_to_existing(at, value) -> bool` (the vanilla method) + the spawn when false | `Static`; `LevelArg`, `[f64;3]`, `i32` |
| `ExperienceOrb#canMerge(Lnet/minecraft/world/entity/ExperienceOrb;II)Z` | edge HEAD, static | `orb_can_merge_static(orb, id, value) -> bool`, called where `merge_orbs`/the orb turn call `can_merge` today | `Static`; `EntityArg`, `i32`, `i32` |
| `ExperienceOrb#canMerge(Lnet/minecraft/world/entity/ExperienceOrb;)Z` | edge HEAD | `orb_can_merge(into, other) -> bool` (wraps `orb_merges`' decision) | `Entity(into)`; `EntityArg(other)` |
| `ExperienceOrb#merge(Lnet/minecraft/world/entity/ExperienceOrb;)V` | body (inject before `discard`) | `orb_merge(into, other)` | `Entity(into)`; `EntityArg(other)` |
| `ExperienceOrb#playerTouch(Lnet/minecraft/world/entity/player/Player;)V` | from SPEC-1's analysis | `orb_player_touch(orb)` (the touch part of `tick_experience`, for the acting player; guests through `as_guest`) | `Entity(orb)`; `PlayerArg` |
| `ExperienceOrb#repairPlayerItems(Lnet/minecraft/server/level/ServerPlayer;I)I` | body | `orb_repair_player_items(orb, amount) -> i32` (Mending's part of the touch) | `Entity(orb)`; `PlayerArg`, `i32` |

Edge ones get body bridges (`route … if backed`) in `G/java_bridges/entities.rs` calling the body fns. The two body ones run vanilla's bodies on the views; their closure (`needs_bridge`: `merge` none, `repairPlayerItems` `ServerPlayer#level`, `LivingEntity#getItemBySlot`, `onEquippedItemBroken`, `Level#getRandom`) must be bridged: check `java_mods.py check --analysis` and add the missing ones in `G/java_bridges/player.rs` (`getRandom` → the level random bridge W9 4-7 made).

## 1. Field writes from Java code (new, general)
Vanilla's `merge` body writes `this.count` and `this.age` with `PUTFIELD`; Clumps' handlers (mixins into `ExperienceOrb`) may too. Those writes land on the view only: nothing bridges a field. W9's rule is "every writer is a bridge"; this makes field writes writers.
- **Analysis (`tools/java_mods.py`, `unbridged_writers` and `check --analysis`):** list every `PUTFIELD` whose owner is a vanilla `net/minecraft/world/entity/**` class, in (a) mod classes, (b) mixin handlers, (c) the bodies of routed methods (the vanilla method's own bytecode, read from the patched jar as the closure analysis does). Each `(owner, field)` must have a row in `J/boot/res/field_bridges.tsv` (new: `owner \t field \t desc \t native`) or `check` fails, naming the writer.
- **Class processor (`P/BridgeProcessor.java`):** for each row, rewrite `PUTFIELD owner.field` in every class it processes (vanilla and mods, after Mixin) into `INVOKESTATIC dev/horizonlab/javamods/boot/FieldBridges.<native>(Lowner;<desc>)V`. `FieldBridges.<native>` stores the field (through a generated accessor or `MethodHandle` setter, made once) and, when the instance is backed (`Backed.horizon$backed`), calls the JNI native with the entity id and the value. Instances that aren't backed keep plain field semantics.
- **Rows for this batch:** `ExperienceOrb.value I`, `ExperienceOrb.count I`, `ExperienceOrb.age I` (check the 26.3 field names in the source), natives in `G/java_bridges/entities.rs` writing the native orb and calling `note_touched(Some(id))`.
- `RouteStubCheck` (exists, `U/../plugin/`): add a case: a class with a `PUTFIELD` to a listed field is rewritten; one to an unlisted field isn't.

## 2. Seams
- `award_orb` → `try_merge_to_existing(at, value) -> bool` with vanilla's signature and returns, then `spawn_orbs` spawns when it returns false (vanilla's `award`). The `merge_id` draw stays where vanilla draws it (inside `tryMergeToExisting`'s body: check the source; the body fn keeps it).
- The merge scan (in the orb's turn after W9 2-9, else in `merge_orbs`): `if self.orb_can_merge(into, other) { self.orb_merge(into, other) }`, in vanilla's order. Inside `orb_can_merge`'s body, the static `canMerge(orb, id, value)` is reached through `orb_can_merge_static` (gated), since vanilla's instance `canMerge` calls the static one.
- `tick_experience`'s touch: `orb_player_touch(orb)` for each touching player, in vanilla's order; inside its body, `orb_repair_player_items` where vanilla calls it. `guest_orbs` (`G/lan_actions.rs`) goes through the same fns under `as_guest`.

## 3. Tests
- Test mod mixins (`J/testmod/…/mixin/OrbRouteMixin.java`), with values we choose:
  - `@Inject(method = "canMerge(Lnet/minecraft/world/entity/ExperienceOrb;)Z", at = @At("HEAD"), cancellable = true)`: when both orbs have value 3, `cir.setReturnValue(true)` regardless of id;
  - `@Inject(method = "merge", at = @At("HEAD"))`: count calls;
  - `@Inject(method = "tryMergeToExisting", at = @At("HEAD"), cancellable = true)`: for value 11, `cir.setReturnValue(true)` (the orb is swallowed: no spawn).
- `ChecksEntities` (`later`):
  - `orb_merge_forced`: the session summons 6 orbs of value 3 at the same spot (`/summon experience_orb ~ ~1 ~ {Value:3}`), waits 40 ticks: exactly 1 orb remains (`level.getEntities` in a box) with `count` 6, and `merge` ran 5 times. Vanilla without the mixin: the merge needs `id % 40` equality, so several remain; the mixin is what makes it 1;
  - `orb_try_merge_cancel`: `/xp`-free path: breaking a test block that drops 11 XP (or `ExperienceOrb.award(level, pos, 11)` from the test mod's command) spawns no orb;
  - `orb_fields_written_through`: after the merge, native's `/data get entity <orb> Count` is 6 (the field bridge reached native).
- Rust: `try_merge_signature_returns` (no java-mods: an orb of the same value within the box and the id rule merges, as today's tests; keep them green).
- `python3 tools/parity.py experience` (or the orb harnesses `--list` names); `cargo test -p horizon-client --features java-mods --lib`; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run.

## Done bar
Checks pass; `check --analysis` lists no unbridged field writer for `ExperienceOrb`; six gates listed; lint passes.

## For the lead and the user
- Lead (M3, needs a NeoForge server with the same showcase jars, as the energy suite did): on both, `/summon experience_orb ~ ~1 ~ {Value:1}` ×50 at one spot, wait 60 ticks, count orbs (`/execute if entity @e[type=experience_orb]` count) and `/data get` the one left. Expected: the same count (1 with Clumps) and the same total value on both.
- User: kill 20 zombies with a sword in a pit and stand nearby: orbs clump into one or a few, and picking it up gives the XP at once (a single pickup sound), as with Clumps on NeoForge. With a Mending tool damaged, the repair happens from the clumped orb.
