# SPEC-2 (W4): mods' writes to the local player reach native's step

Design: `DESIGN.md` §2.2, D1, Q6. Size: ~420 changed lines (Java ~170, Python ~80, Rust ~170). Depends on: SPEC-1.

Read first: SPEC-1 (as landed), `B/Lockstep.java`, `B/ClientBridge.java` (`fill`, `turnPlayer`), `B/Shells.java` `isMods`,
`B/EntityFacts.java` `confirm`, `T/mod_entity_facts.py` (`STATE_GROUPS`, the `motion`/`position` groups at line ~259, the `Player`
exclusion at line 303, `writes` at line 515), `G/java_lockstep.rs`, `G/remote.rs` `tick_remote_level` 6229–6555
(`previous_position` at 6333, the abilities send at 6536). Vanilla: `Entity.setDeltaMovement`, `setPosRaw`, `setOldPosAndRot`,
`ClientLevel.tickNonPassenger`, `LocalPlayer.onUpdateAbilities`.

Code comments: one line at most.

## Goal

In a lockstep tick, every change mod code makes to the local player's motion state (velocity, position, rotation, ground, fall
distance, sprinting, flight) reaches native's player step **in the same tick**, in the order vanilla would apply it, through
setter-level write-through (never a diff). Lockstep also turns on when the install analysis finds a mod that writes a player's
motion (Mekanism's jetpack writes from `ClientTickEvent.Pre`, which is not in SPEC-1's listener list).

Accept: the test mod's "rocket" (below) lifts the player exactly 5.0 blocks in its first 10 ticks; Mekanism's jetpack flies.

## Analysis (`T/mod_entity_facts.py`, ~80 lines)
- New result key `player_writes`: every call a mod's class makes to a writer of the `motion` or `position` group (the existing
  name lists; add `addDeltaMovement`, `push`, `setOnGround`, `setSprinting`, `onUpdateAbilities` to a `player` group used only
  here) whose static receiver `is_a` `net/minecraft/world/entity/player/Player` (so `Player`, `AbstractClientPlayer`, `LocalPlayer`,
  `ServerPlayer` and mod subclasses). Shape: `[{"key", "receiver", "mods"}]`, sorted, like `writes`. The `Player` exclusion at
  line 303 stays for `writes`.
- Keep `format` 2; readers treat a missing key as empty.
- Tests in `tests/test_mod_entity_facts.py`: a javac-built class calling `player.setDeltaMovement(0, .5, 0)` on a `Player`-typed
  local, and one calling it through a generic `<P extends Player> void f(P p)` (erasure gives `Player`, Mekanism's shape), are in
  `player_writes`; a call on an `ItemEntity` is not; a call on `Entity` is not (an `Entity` receiver is not provably a player).

## Java

### `B/EntityFacts.java` (~8 lines)
`confirm`: `playerMotionWriters = facts.has("player_writes") && !facts.getAsJsonArray("player_writes").isEmpty()`; log once
`Horizon: entity facts: <n> player motion writers (<mods>)`. `Lockstep.wanted()` ORs it in.

### `B/MotionLog.java` (new, ~110 lines)
```java
final class MotionLog {
    static final byte VELOCITY = 1, POSITION = 2, YAW = 3, PITCH = 4, GROUND = 5, FALL_RESET = 6, SPRINT = 7, ABILITIES = 8, OLD_POS = 9;
    static volatile Entity watching;   // the local player while the client is in lockstep, else null
    static boolean filling;            // Horizon's own fills (ClientBridge.fill, turnPlayer)
    static void velocity(Entity e, Vec3 v); static void position(Entity e, double x, double y, double z);
    static void yaw(Entity e, float y); static void pitch(Entity e, float x); static void ground(Entity e, boolean g);
    static void fallReset(Entity e); static void sprint(Entity e, boolean s); static void abilities(Player p); static void oldPos(Entity e);
    static byte[] take();              // the entries since the last take, then empty
}
```
- Every recorder returns at once unless `e == watching && !filling`; then, except `oldPos`, it records only when
  `byMod()`: `StackWalker.getInstance(RETAIN_CLASS_REFERENCE)` skips its own frames and the mixin handler frame, then skips frames
  whose declaring class's name starts with `net.minecraft.world.entity.`, and answers `Shells.isMods(first remaining class)`.
- `oldPos` records unconditionally (it marks where vanilla takes `xo/yo/zo`: `ClientLevel.tickNonPassenger`).
- Entry layout: `byte kind`, then VELOCITY 3 `double`; POSITION 3 `double`; YAW `float`; PITCH `float`; GROUND `bool`; FALL_RESET
  nothing; SPRINT `bool`; ABILITIES `bool flying`, `bool mayfly`, `float flyingSpeed`; OLD_POS nothing. `take()` writes `int n` first.
- The log is **not** cleared by `Lockstep.end()`: writes made after M (`PlayerTickEvent.Post`, `ClientTickEvent.Post`, packet
  handlers of mods' payloads) belong to the next tick's step, as in vanilla. `watching` is set by `Lockstep.begin()` (to
  `minecraft.player`) and cleared when the client leaves (`ClientBridge` leave path) or a tick runs without lockstep (then
  `take()` once to drop stale entries).

### `B/mixin/MotionWritesMixin.java` (new, ~45 lines; client mixins list)
`@Mixin(Entity.class)` RETURN injections: `setDeltaMovement(Lnet/minecraft/world/phys/Vec3;)V` → `velocity`; `setPosRaw(DDD)V` →
`position`; `setYRot(F)V`; `setXRot(F)V`; `setOnGround(Z)V`; `resetFallDistance()V`; `setSprinting(Z)V`; `setOldPosAndRot()V` →
`oldPos`. `@Mixin(LocalPlayer.class)` HEAD of `onUpdateAbilities()V` → `abilities`. Every handler's first statement is the
`watching` reference test, so server views pay one field load.

### `B/ClientBridge.java` (~10 lines)
`fill` and `turnPlayer` set `MotionLog.filling = true` in `try/finally`. `ParkRecord.take()` (SPEC-1's empty record) becomes
`MotionLog.take()` followed by SPEC-4's block when that lands: the record is `int len` + log, then later blocks.

## Rust

### `G/java_lockstep.rs` (~140 lines)
```rust
/// What mods wrote to the local player since the last step, applied in order; `old` is where vanilla took `xo/yo/zo`.
pub(in crate::game) struct MotionWrites { pub entries: Vec<MotionWrite>, }
pub(in crate::game) enum MotionWrite { Velocity([f64; 3]), Position([f64; 3]), Yaw(f32), Pitch(f32), Ground(bool), FallReset,
    Sprint(bool), Abilities { flying: bool, may_fly: bool, flying_speed: f32 }, OldPos }
pub(in crate::game) fn read_motion_writes(bytes: &mut In) -> Option<MotionWrites>;
impl Game<'_> {
    /// Applies a write log to the window's player; returns the position vanilla took as `xo/yo/zo`, when the log marked it.
    pub(in crate::game) fn java_apply_motion_writes(&mut self, writes: &MotionWrites) -> Option<Vec3>;
}
```
- Apply in order: `Velocity` → `player.velocity`; `Position` → `player.position` (only the field: a client `setPos` is not a
  teleport and sends nothing); `Yaw`/`Pitch` → `self.yaw`/`self.pitch` and the player model's head as `turn_player` keeps it
  (`G/perspective.rs:630`); `Ground` → `player.ground.on_ground`; `FallReset` → `player.fall_distance = 0.0`; `Sprint` →
  `player.sprinting`; `Abilities` → `player.flying`, `player.may_fly`, `player.flying_speed`; when that changes `flying`, send
  `S::PlayerAbilities` as `tick_remote_level` does at 6536 (vanilla's `LocalPlayer.onUpdateAbilities` sends at once); `OldPos` →
  remember the player's position at that point as the return value.
- An unknown kind or a short read: log once `Horizon: a lockstep record ended in pieces` and apply nothing of that record.

### `G/remote.rs` (~15 lines)
- After `let java_step = self.java_hook_player_begin();`: `let java_old = self.java_hook_motion_writes(java_step.as_deref());`
  (two-arm wrapper in `G/java_hooks.rs`: reads the record's log and applies it; `None` without the feature).
- At 6333: `self.previous_position = java_old.unwrap_or(player.position);` (vanilla took `xo` before a `PlayerTickEvent.Pre`
  write and after a `ClientTickEvent.Pre` one).

## Test mod (`ClientMotion.java`, ~50 lines)
- Item `horizon_java_test:rocket` (plain `Item`, in `testmod/content`'s item list as the wand is).
- `ClientTickEvent.Pre` listener: if the local player holds the rocket in the main hand and `rocketTicks < 10`:
  `player.setDeltaMovement(0, 0.5, 0)`, `rocketTicks++`, and before the first of them remember `startY = player.getY()`.
- Client `PlayerTickEvent.Post` listener (same tick, after native's step was filled in): on the first rocket tick check
  `rocket_moves_in_its_own_tick`: `Math.abs(player.getY() - startY - 0.5) < 1e-9`; on the tenth check `rocket_lifts_five_blocks`:
  `Math.abs(player.getY() - startY - 5.0) < 1e-9`. Expected values from vanilla's `LivingEntity.travelInAir`: `move` uses the delta
  movement as it stands (0.5, set before `aiStep`; no keys held, so `moveRelative` adds nothing), and gravity applies to the
  remainder after the move, so tick k ends exactly 0.5·k above the start. A one-tick lag (the write reaching native's next step) reads
  0 at the first `Post`, which is what today's code does.
- `PlayerTickEvent.Pre` listener (client): while holding the rocket on tick 12 only, `player.setPos(x, y + 1, z)`; check on tick 13
  `pre_write_moves_without_lerp_start`: the window's player is 1 block higher (Rust sent it back in the fill) — read through
  `Lockstep.lastFill()` — and `xo` equals the pre-write y (vanilla took `xo` before `PlayerTickEvent.Pre`).
- Drive (`T/java_mods.py` `bridges` `drive`, before the wand part): `command fill` a 3×3×20 air column over a stone floor at the
  player, `give horizon_java_test:rocket`, `select horizon_java_test:rocket`, `wait 20`, `command clear @s horizon_java_test:rocket`,
  `wait 40` (land).

## Tests
- Python: the `player_writes` cases above (`python3 -m unittest tests.test_mod_entity_facts`).
- Java unit `J/unit/dev/horizonlab/javamods/boot/MotionLogCheck.java`: with a fake `watching` object and a predicate standing in
  for `Shells.isMods` (package-private seam `MotionLog.modClass`), a call from a class named `mod.Fake` is recorded, a call through
  a `net.minecraft.world.entity.Entity`-named frame from `mod.Fake` is recorded, a call from a `net.minecraft.client.multiplayer`
  frame is not, a call while `filling` is not, a call on another object is not; `take()` bytes read back as the documented layout.
- Rust: `read_motion_writes` on bytes built in the test (typed values); `java_apply_motion_writes` on a test player: a velocity
  then a position then `OldPos` then a second velocity leaves the second velocity, the position, and returns the position as `old`;
  an `Abilities` entry that turns flying on queues one `PlayerAbilities` packet with flags 2 (vanilla's `ServerboundPlayerAbilitiesPacket`
  writes 2 for flying).
- `cargo test -p horizon-client --features java-mods --lib java_lockstep`, clippy with and without the feature, `java_mods.py build`
  (private dir), `unit`, `check --analysis` (now lists player writers for Mekanism; still 0 problems on install-rs),
  `bridges` (`rocket_moves_in_its_own_tick`, `rocket_lifts_five_blocks`, `pre_write_moves_without_lerp_start`).

## Done bar
Tests pass. `check --analysis` on install-rs prints Mekanism under player motion writers. One `hz_scenario.py` run of
`tools/scenarios/w4_lockstep.json` (SPEC-1) still passes with the analysis present.

## Live test (lead; play install, survival)
1. Fill a Mekanism jetpack with hydrogen (an Electrolytic Separator on water, or the creative tab's filled jetpack in creative,
   then `/gamemode survival`), wear it, hold jump: the player rises smoothly from the first tick, hovers in Hover mode, and stops
   rising the tick jump is released (compare with a Forge client on the same jar: same climb rate, about 0.5 blocks a tick at full
   thrust). No rubber-banding back down (the server accepts the client's position).
2. Meka-Suit with its jetpack unit: same as 1.
3. Walk off a ledge with the jetpack in Hover mode: no fall sound on landing softly; land hard without it: the fall sound plays
   once (not twice).
