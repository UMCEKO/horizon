# SPEC-1 (W4): the lockstep client tick

Design: `DESIGN.md` §2.1, D1–D3. Size: ~480 changed lines (Java ~230, Rust ~250). Depends on: nothing. Paths as DESIGN.

Read first: `CLAUDE.md`, RULES.md, `docs/java-mods-map.md` §1 (threads), §4 (the Java client, HUD timing), §5 (client tick and
status rows), §8 (gotchas: "Calls don't nest", "Every cfg site has two arms"); `B/ClientBridge.java` (`call`, `run`, `tick`,
`turnPlayer`, `move`, `status`, `OPS`), `B/mixin/MinecraftTickMixin.java`, `B/mixin/LocalPlayerMoveMixin.java`, `B/Shells.java`
(`isMods`, `ownerOf`, `wanted`); `G/java_screens.rs` (the `live` module: `Job`, `Worker`, `Sequence`, `java_client_tick_begin`,
`java_client_tick_send`, `java_client_collect`, `java_client_status`); `G/remote.rs` `tick_remote_level` (lines 6116–6555);
`G/game.rs` `fn tick` (7882–7915). Vanilla: `Minecraft.tick`, `LocalPlayer.aiStep`, `LivingEntity.aiStep`.

Code comments: one line at most, only for a non-obvious why.

## Goal

When a mod listens to a client tick-time movement event, the Java client's tick runs **interleaved** with the window's tick in
vanilla's order: Java's `handleKeybinds` point (K) is where the window's clicks are served, and Java's `LocalPlayer.aiStep` (M) is
where native's player step runs. Nothing changes for a pack without such a listener. This spec builds the mechanism and fills the
local player at M; SPEC-2…4 put events and mods' writes through it.

Accept: with the test mod (it listens to `PlayerTickEvent.Pre/Post` on the client, below), the client-side order per tick printed by
`HORIZON_JAVA_DEBUG` is `ClientTickEvent.Pre, K, PlayerTickEvent.Pre, M, PlayerTickEvent.Post, ClientTickEvent.Post`, and the
player's view in `PlayerTickEvent.Post` holds native's position of **this** tick.

## Java

### `B/Listeners.java` (new, ~40 lines)
Move `Shells.ownerOf` and the bus walk out of `Shells.wanted` into
`static boolean modListens(IEventBus bus, List<Class<? extends Event>> events, Set<String> heard, String what)`, logging
`Horizon: <what> heard by <owner>` once per owner. `Shells.wanted` calls it (behaviour unchanged).

### `B/Lockstep.java` (new, ~150 lines)
```java
final class Lockstep {
    enum Point { NONE, K, M, ENDED }
    static final List<Class<? extends Event>> EVENTS = List.of(MovementInputUpdateEvent.class, ComputeFovModifierEvent.class,
            PlayerTickEvent.Pre.class, PlayerTickEvent.Post.class, LivingEvent.LivingJumpEvent.class, LivingFallEvent.class);
    static boolean wanted();                // Listeners.modListens(NeoForge.EVENT_BUS, EVENTS, …) || Analysis.playerMotionWriters (SPEC-2; false here)
    static void begin();                    // the tick job's thread, before `events(in)`
    static boolean active();                // a lockstep tick runs on this thread
    static byte[] park(Point p, byte[] record); // publish, serve the inbox until released, return the release payload
    static byte[] waitFor(Point p);          // window thread: block until parked at p or past it; reply below
    static byte[] call(byte kind, byte[] in);// window thread: run on the parked thread, wait for the reply
    static void release(Point p, byte[] payload);
    static void end();                       // serve what is left, mark ENDED, wake every waiter
    static boolean intercept(String op, byte[] in, CompletableFuture<byte[]> reply); // ClientBridge.call's inbox route
}
```
- State under one `ReentrantLock` + `Condition`: `jobThread`, `parked` (`Point`), `passed` (highest point reached), `record`,
  a `ArrayDeque<Request>` inbox (`Request(byte kind, String op, byte[] in, CompletableFuture<byte[]> reply)`), and a release slot
  `(Point, byte[])`.
- `park(p, record)`: set `parked = p`, `passed = p`, `this.record = record`, signal; loop: wait until the inbox has a request or the
  release slot holds `p`; run inbox requests in order on this thread (kind 5 `OP`: `ClientBridge.run(op, in)`; kinds 1–4 are added by
  SPEC-3/10 through `Lockstep.handler(kind, Function<byte[], byte[]>)`, an unknown kind completes exceptionally); on the release,
  clear `parked`, return its payload. A `Throwable` from a request completes that request exceptionally and the loop goes on.
- `waitFor(p)`: wait until `passed >= p` (enum order) or `parked == ENDED`; reply `bool parkedHere` (`parked == p`), `int len` +
  `record` (empty when not parked here).
- `release(p, payload)`: when `parked != p` and `passed > p` or `ENDED`, drop it (nothing waits); else fill the slot and signal.
- `end()`: serve the inbox until empty, `parked = passed = ENDED`, `jobThread = null`, signal all.
- `static void announce()`: called by the window (op below) **before** it hands the tick job to the worker; from then until
  `end()`, `intercept` routes requests to the inbox even if the job hasn't begun. Without it, a window request submitted to the
  Render executor behind the queued tick job would wait for that job, which parks at K waiting for the window: a deadlock.
- `intercept`: when announced or a lockstep job is in flight, and the caller isn't `jobThread`, queue `OP` and return true;
  `ClientBridge.call` then waits on `reply` exactly as it waits on the executor's future. Requests queued before `begin()` are
  served at K like the others.
- With `HORIZON_JAVA_DEBUG`: `Horizon: lockstep <point>` at each park and `Horizon: lockstep end` (the accept's order line reads
  these with the test mod's lines).

### `B/ClientBridge.java` (~50 lines)
- `call(op, in)`: before submitting to the Render executor, `if (Lockstep.intercept(op, in, future)) return future.get(…)` with the
  same timeout handling as today.
- `OPS`/`run`: four ops that run **on the caller's thread** (add them where `component_read` is exempted in `Crossing`/`call`):
  - `client_lockstep_announce`: no input → `Lockstep.announce()`; replies `{1}`.
  - `client_lockstep_wait`: `int point` → `Lockstep.waitFor`.
  - `client_lockstep_call`: `byte kind`, `bytes` → `Lockstep.call`.
  - `client_lockstep_release`: `int point`, `bytes` → `Lockstep.release`; replies `{1}`.
- `tick(in)`: after `float partial` read **`bool lockstep`** (wire change, both sides, map §5). If set: `Lockstep.begin()` first; in a
  `try/finally`, `Lockstep.end()` after `recordShell` and before `status(...)`. Lockstep requires `ticks == 1` (the window sends one
  tick per job in lockstep); log and treat as not lockstep otherwise.
- `status(...)`: append `bool lockstep` = `Lockstep.wanted()` (the decision for the window's next tick).
- `fill(LocalPlayer, DataInputStream)` (new, from `move`): the release payload below, applied with vanilla setters; `move` keeps
  its job for the non-lockstep path.

### `B/mixin/MinecraftTickMixin.java` (~6 lines)
`horizon$keybinds` (HEAD of `handleKeybinds`): `if (Lockstep.active()) Lockstep.park(Lockstep.Point.K, new byte[0]);` before
`info.cancel()`. This is vanilla's `handleKeybinds` moment, after `ClientTickEvent.Pre`, `gui.tick` and `pick`.

### `B/mixin/LocalPlayerStepMixin.java` (new, ~30 lines; add to `J/boot/res/horizon_java.client.mixins.json` `client`)
`@Inject(method = "aiStep", at = @At("HEAD"), cancellable = true)` on `LocalPlayer`: when `Lockstep.active()` and `this` is
`Minecraft.getInstance().player`: `byte[] fill = Lockstep.park(M, ParkRecord.take())` (SPEC-1: `ParkRecord.take()` returns an empty
record; SPEC-2 and SPEC-4 add to it), then `ClientBridge.fill(this, fill)`, `info.cancel()`. Otherwise do nothing.

### The release payload (M), written by Rust `java_lockstep_fill`
`bool moved`; if moved: 3 `double` position, 3 `double` velocity, 2 `float` yaw/pitch, `bool onGround`, `bool horizontalCollision`,
`bool verticalCollision`, `double fallDistance`, `bool sprinting`, `bool shiftKeyDown`, `bool swimming`, `bool fallFlying`,
`bool flying`, `byte pose` (`Pose` ordinal), `float xxa`, `float zza`, `bool jumping`, `byte keyPresses` (bits forward 1, backward 2,
left 4, right 8, jump 16, shift 32, sprint 64), 2 `float` moveVector (x, y). `moved == false` (native's step didn't run): Java
applies nothing. Java sets `input.keyPresses` and `moveVector` (accessor `B/mixin/ClientInputAccess.java`, new) so a mod reading
`player.input` later in the tick sees this tick's input, as vanilla's `aiStep` leaves it.
After the fill, in both cases: `int n` + the tick's `animateTick` picks as `Input::Animate` writes them (native picks them in
`tick_particles`, before its player step). Java makes them the current group (`Animate`'s current-group setter; add one if only
`group`/`next` exist), so this tick's `level.animateTick`, later in `Minecraft.tick`, uses this tick's picks as today's
batch-at-the-end order does. When Java never parked at M, Rust leaves the picks for the next batch (today's path).

Until SPEC-3 lands, `MovementInputUpdateEvent`, `LivingJumpEvent` and `LivingFallEvent` don't fire for the local player in
lockstep (Java's `aiStep` body no longer runs); their results were discarded before, so nothing a player sees changes.

## Rust

### `G/java_lockstep.rs` (new, cfg `java-mods`; declare beside `mod java_screens;` in `G/game.rs` with the same `cfg`)
```rust
/// Releases a pending park on every exit path of the window's tick (K, then M as "no move").
pub(in crate::game) struct LockstepTick;
impl Drop for LockstepTick { fn drop(&mut self) { release_pending(); } }

impl Game<'_> {
    pub(in crate::game) fn java_lockstep_keys_begin(&mut self);   // wait K (once per tick)
    pub(in crate::game) fn java_lockstep_keys_end(&mut self);     // release K
    pub(in crate::game) fn java_lockstep_player_begin(&mut self) -> Option<Vec<u8>>; // release K if pending, wait M, the record
    pub(in crate::game) fn java_lockstep_player_end(&mut self, moved: bool);          // release M with java_lockstep_fill
    fn java_lockstep_fill(&self, moved: bool) -> Vec<u8>;
}
/// A lockstep inbox call from the window thread while Java is parked; None outside a lockstep tick.
pub(in crate::game) fn java_lockstep_call(kind: u8, request: &[u8]) -> Option<Vec<u8>>;
fn release_pending();
```
- State in the `CLIENT` thread-local (`java_screens.rs` `Client`): `lockstep: bool` (this tick), `lockstep_next: bool` (from the last
  status), `stage: Stage` (`Idle`, `Sent`, `KeysHeld`, `KeysReleased`, `PlayerHeld`, `Done`).
- `java_lockstep_keys_begin`: `Stage::Sent` → `client_lockstep_wait(K)`; `KeysHeld` whatever the reply (not parked here means Java
  finished early: then every later wait returns at once). Other stages: nothing.
- `java_lockstep_player_begin`: release K if `KeysHeld`; then `client_lockstep_wait(M)`; `PlayerHeld` and the record when parked
  here, else `None` and `Done`.
- `java_lockstep_player_end(moved)`: when `PlayerHeld`, `client_lockstep_release(M, fill)`, `Done`.
- `release_pending` (Drop): `KeysHeld` → release K; `PlayerHeld` → release M with `[0]` (moved false); then `Done`. It needs only
  the host and `CLIENT`, not `Game`, so the guard holds no borrow.
- Profile spans (`horizon_client::spans::time`): `lockstep_k`, `lockstep_m` around the two waits.

### `G/java_screens.rs` (~90 lines)
- Split today's batch assembly out of `java_client_tick_send` into `fn java_client_batch(&mut self, lockstep: bool) -> Job`
  (unchanged content; writes `bool lockstep` after the partial).
- `java_client_tick_begin`: when `CLIENT.lockstep_next` and the player is joined: `java_client_collect(true)` (the previous job must
  have ended), set `lockstep = true`, `stage = Sent`, `owed = 1` (lockstep is one window tick per job), call
  `client_lockstep_announce` on this thread, then send `java_client_batch(true)` to the worker. Return a `LockstepTick` guard to the
  caller (below). Otherwise today's behaviour.
- `java_set_animate_picked` in a lockstep tick keeps the picks for `java_lockstep_fill` instead of the next batch.
- `java_client_tick_send`: when this tick is lockstep: drop the guard's work (`release_pending`), and if a Java screen is open
  `java_client_collect(true)` (a screen ticks in step with the window, as today); no second batch. Otherwise today's.
- `java_client_status`: read the appended `bool lockstep` into `lockstep_next`.
- A window request issued while a lockstep job runs needs nothing new: `host.request` blocks until Java serves it at its next park.

### Sites (two-arm wrappers in `G/java_hooks.rs`, empty `cfg(not)` arms)
- `G/game.rs` `fn tick` (7904): `let _lockstep = self.java_hook_tick_begin();` replaces the bare `self.java_client_tick_begin();`
  (returns `Option<LockstepTick>` under the feature, `()` without).
- `G/remote.rs` `tick_remote_level`: `self.java_hook_keys_begin();` right before `self.remote_handle_input();` (6164),
  `self.java_hook_keys_end();` right after it.
- `G/remote.rs`: `let java_step = self.java_hook_player_begin();` right before `let entity_collisions = self.player_entity_collisions();`
  (6229), before the `is_lit` early return (vanilla's `LocalPlayer.tick` reaches `aiStep` only when loaded; Java parks or not on its
  own state, the guard covers both). SPEC-1 ignores the record (`let _ = java_step;`).
- `G/remote.rs`: `self.java_hook_player_end(true);` right before `let auto_jumped = …` (6551): vanilla's `PlayerTickEvent.Post`
  precedes `LocalPlayer.sendPosition`. Early returns between the two calls leave it to the guard (moved false).

## Test mod (`J/testmod/dev/horizonlab/javamods/testmod/ClientMotion.java`, new; registered beside `ClientKeys.register`)
- Client listeners on `PlayerTickEvent.Pre` and `.Post` guarded `event.getEntity().level().isClientSide()`: record the tick's
  sequence into a list with the K/M debug markers (`Lockstep` exposes a `static IntSupplier` of parks for checks).
- `Post` check `lockstep_post_sees_this_tick` (once, the 40th client tick after join): the local player's `getX/getY/getZ` equal the
  position Rust sent in the last M release (`Lockstep.lastFill()` debug getter, Java-only) and differ from `xo/yo/zo` when the player
  walked (the drive presses `w` for 20 ticks first).
- `lockstep_order`: over 20 ticks, every tick's markers read `Pre, K, PlayerTick.Pre, M, PlayerTick.Post, Post`.

## Tests
- Java unit `J/unit/dev/horizonlab/javamods/boot/LockstepCheck.java` (no game; two threads): a job thread parks at K then M;
  a "window" thread calls `waitFor(K)` (gets parked), queues two `OP` requests (served in order at K, replies in order), releases K,
  `waitFor(M)` (parked, record bytes equal what was published), queues an `OP` (served at M), releases M with a payload (the job's
  `park` returns it byte-equal); a request queued after `end()` runs on the executor path (intercept returns false); `waitFor(M)`
  after a job that ended without parking at M returns `parkedHere = false` at once; a release for a point already passed is dropped;
  a request made after `announce()` and before the job's `begin()` is answered at K (the deadlock case, with a 2 s timeout as the
  failure).
- Rust (`java_lockstep.rs` `#[cfg(test)]`): the fill bytes for a known player read back field by field in the documented order
  (expected values typed in the test, not taken from the writer); the `Stage` machine: `release_pending` from `KeysHeld` emits
  release K then M(false); from `PlayerHeld` emits only M(false); from `Done` emits nothing (record the emitted ops through a test
  sink instead of the host).
- `cargo test -p horizon-client --features java-mods --lib java_lockstep java_screens`, clippy `-D warnings` with and without the
  feature, `java_mods.py build` (private `HORIZON_JAVA_MODS_DIR`), `unit`, `check`, `bridges` (the two new checks plus every old
  one: lockstep is on there because the test mod listens).
- Harnesses: none touch the window's Java client.

## Done bar
- All the above pass; `check --analysis` unchanged.
- One `hz_scenario.py` run (play install, private jars, `HORIZON_JAVA_DEBUG=1`, `HORIZON_PROFILE=OUT/profile`) of
  `tools/scenarios/w4_lockstep.json` (new: floor as `w5`'s scenario, survival, `{"press":"w"}`, `{"wait":40}`, `{"release":"w"}`,
  `{"wait":20}`; checks `no_exceptions`, `{"log": "Horizon: lockstep M"}`): passes (Mekanism's `LivingJumpEvent` listener makes
  lockstep wanted); report `lockstep_k` and `lockstep_m` mean and max from `profile_report.py` in the hand-off.
- The off path: `java_mods.py play` with only AE2 and its library (no listener in `Lockstep.EVENTS`, checked in its `replaces.json`)
  logs no `Horizon: lockstep` line and the client tick status reads `lockstep false`.

## Live test (lead)
1. `bridges`: `lockstep_order ok`, `lockstep_post_sees_this_tick ok`.
2. Play install (Mekanism, AE2, RS, Jade, BG2; lockstep on): walk, sprint, jump, swim, ride a boat, open the inventory, use a
   Mekanism machine's screen, click and mine blocks, place with BG2. Everything behaves as before this spec; mining, clicks and
   item use are not delayed (they are served at K now).
3. With `HORIZON_JAVA_DEBUG=1`: open a Mekanism machine's screen while falling from 10 blocks; no freeze, the player keeps falling
   under the screen (native's step ran at M with the screen open), and closing it shows the right place.
4. Read `lockstep_k` mean and max in the profile: if the mean exceeds 1 ms, report it to the user (Q1) before SPEC-2.
