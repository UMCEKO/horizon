# SPEC-3 (W4): `MovementInputUpdateEvent`, `LivingJumpEvent` and `LivingFallEvent` for the local player at native's sites

Design: `DESIGN.md` §1.1, §2.1, D3. Size: ~400 changed lines (Java ~130, Rust ~270). Depends on: SPEC-1; SPEC-2 for the write log
the replies carry (land after it, or read an empty log until it lands). Coordinates with W9 SPEC-4-6 (the server's arm, §Rust 3).

Read first: SPEC-1, SPEC-2, `B/Lockstep.java` (`handler`), `B/MotionLog.java`, `G/java_lockstep.rs`, `G/remote.rs` 6262–6300
(keys, `native_movement_input`, the input byte), `L/motion.rs` 2084–2098 (`jump_from_ground`), `L/landing.rs` 30–40 and 320–350
(`native_living_fall`), `L/native_client_hooks.rs`, `L/entry.rs` 145–160. Vanilla: `LocalPlayer.aiStep` (`input.tick()` then the
event, then `jumping = input.keyPresses.jump()`, `xxa/zza` from `moveVector` in `applyInput`), `LivingEntity.jumpFromGround`,
`LivingEntity.causeFallDamage`, `Player.causeFallDamage`. NeoForge (javap): `ClientHooks.onMovementInputUpdate(Player, ClientInput)`,
`CommonHooks.onLivingJump(LivingEntity)`, `CommonHooks.onLivingFall(LivingEntity, double, float) → LivingFallEvent`,
`EventHooks.onPlayerFall(Player, float, float)`; `ClientInput.keyPresses` is public, `moveVector` protected.

Code comments: one line at most.

## Goal

In a lockstep tick (SPEC-1), the three events fire on the Render thread at native's moments, with native's values as inputs, and
their results drive native's step:
- `MovementInputUpdateEvent` right after native read the keys (where `native_movement_input` runs; Java first, then native mods);
- `LivingJumpEvent` when native's player jumps (`jump_from_ground`), with the jump's velocity in place;
- `LivingFallEvent` when native's player lands with a fall (`native_living_fall` site), and `PlayerFlyableFallEvent` where native's
  `Player.causeFallDamage` port takes the `mayfly` branch.

Accept: Traveler's Backpack's input listener and Mekanism's Hydraulic Propulsion jump boost act on the window's player.

## Java (`B/LockstepEvents.java`, new, ~110 lines; registered in `ClientBridge.install` with `Lockstep.handler`)
- Kind 1 `INPUT`: request `byte keyPresses` (SPEC-1's bit order), 2 `float` moveVector (x = left impulse, y = forward impulse, as
  `KeyboardInput.tick` builds `new Vec2(left, forward).normalized()`). Under `MotionLog.filling`: `player.input.keyPresses = new
  Input(...)`, `((ClientInputAccess) player.input).horizon$moveVector(new Vec2(x, y))`; then, not filling,
  `ClientHooks.onMovementInputUpdate(player, player.input)`. Reply: `byte keyPresses`, 2 `float` moveVector (read back after the
  post), then `MotionLog.take()`.
- Kind 2 `JUMP`: request 3 `double` velocity (after vanilla's jump set it). Filling: `player.setDeltaMovement(v)`; then
  `CommonHooks.onLivingJump(player)`. Reply: `MotionLog.take()`.
- Kind 3 `FALL`: request `bool flyable`, `double distance`, `float multiplier`. `flyable`: `EventHooks.onPlayerFall(player,
  (float) distance, multiplier)`, reply `bool canceled = false`, `double distance`, `float multiplier` unchanged. Else
  `var e = CommonHooks.onLivingFall(player, distance, multiplier)`; reply `bool e.isCanceled()`, `double e.getDistance()`,
  `float e.getDamageMultiplier()`. Then `MotionLog.take()`.
- `B/mixin/ClientInputAccess.java` (SPEC-1's accessor) gains the setter if SPEC-1 made only the getter.
- Since the player's `aiStep` body is native in lockstep, Java posts these three events nowhere else for the local player.

## Rust

### 1. Input (`G/remote.rs`, `G/java_lockstep.rs`; ~70 lines)
- `G/java_hooks.rs`: `pub(super) fn java_hook_movement_input(&mut self, keys: &mut Keys, strafe: &mut f32, forward: &mut f32)`
  (not-arm: nothing).
- Call it in `tick_remote_level` right before `self.native_movement_input(...)` (6282). Java arm: when `CLIENT.stage == PlayerHeld`,
  `java_lockstep_call(1, …)` with `keys`' bits and `[strafe, forward]`; on a reply:
  - `keys.forward/backward/left/right/jump/shift/sprint` from the returned bits;
  - when the returned vector differs from the sent one, `[strafe, forward]` = it and `keys.analog = Some([strafe, forward])`;
    otherwise unchanged (native's own direction stays exact);
  - apply the returned write log with `java_apply_motion_writes` (SPEC-2) at once.
- The `original` comparison after `native_movement_input` stays: it must compare with the values **after** Java's hook, so move
  `let original = [strafe, forward];` below the new call.

### 2. Jump (`L/native_client_hooks.rs`, `L/motion.rs`, `L/entry.rs`; ~50 lines)
- `NativeClientHooks` gains `pub living_jump_motion: fn(u32, &str, &mut crate::motion::PlayerMotion)`, with a forwarding fn
  `pub(crate) fn living_jump_motion(entity, kind, player)`; `menu.rs:3662`'s test table and every other `NativeClientHooks { … }`
  literal gain `living_jump_motion: |_, _, _| {}`.
- `L/motion.rs` `jump_from_ground`: after `native_living_jump(...)`, inside the same `if !self.player.replaying`, call
  `crate::native_client_hooks::living_jump_motion(id, "minecraft:player", self.player)`.
- `L/entry.rs` installs `living_jump_motion: |entity, kind, player| crate::game::java_hooks::java_living_jump(entity, kind, player)`
  (a free `pub(crate) fn` in `G/java_hooks.rs`, two arms; it needs no `Game`).
- `java_living_jump` (Java arm): if `CLIENT.stage == PlayerHeld` on this thread → `java_lockstep_call(2, velocity)`, then apply the
  reply's log to `player` with `apply_motion_writes_to(player, &writes, &mut pending)` (new lib-free fn in `G/java_lockstep.rs`:
  velocity, position, ground, fall reset, sprint go to the `PlayerMotion`; yaw, pitch and abilities go to `CLIENT.pending`, which
  `java_hook_player_end` applies through `java_apply_motion_writes` before building the fill). Otherwise call
  `java_living_jump_server(entity, kind, player)`: an empty fn with the doc line `/// The server copy's jump (W9 SPEC-4-6).`, which
  W9's spec fills.

### 3. Fall (`L/entry.rs`, `G/java_hooks.rs`; ~40 lines)
- `L/entry.rs` `living_fall`: after `dispatch_living_fall(...)`, call `crate::game::java_hooks::java_living_fall(entity, kind,
  distance, multiplier, cancel)` (free fn, two arms). Java arm: client branch as for the jump (kind 3, `flyable = false`), applying
  `canceled` (OR into `cancel`), distance and multiplier; else the server branch `java_living_fall_server` (empty, W9 SPEC-4-6).
- `PlayerFlyableFallEvent`: find native's port of `Player.causeFallDamage`'s `abilities.mayfly` early return (`L/landing.rs` or
  `L/living_fall.rs`; `grep -n may_fly`). There, add `crate::native_client_hooks::player_flyable_fall(distance, multiplier)` (one
  more hook field, same pattern) whose client arm sends kind 3 with `flyable = true`. If native has no such branch, the spec stops
  and reports it (it would be a vanilla-parity gap, not W4's).

### 4. Order with native mods
Java's events run before native mods' hooks at each site (Java is vanilla's own event; native mods are Horizon's extension). Write
this in the doc comment of each `java_hook_*`.

## Test mod (`ClientMotion.java`, ~60 lines; client listeners guarded `isClientSide()`)
- Item `horizon_java_test:spring`. `LivingJumpEvent` listener: if the entity is the local player holding the spring, `entity.
  setDeltaMovement(dm.x, dm.y + 0.5, dm.z)` and record the jump's start y. Client `PlayerTickEvent.Post`: track the highest y until
  landing, then check `spring_jump_apex`: `|apex − start − expected| < 1e-6`, where `expected` is computed **in the check** by
  vanilla's air loop, not by any Horizon code: `v = 0.42 + 0.5; h = 0; while (v > 0) { h += v; v = (v − 0.08) × 0.98; }` (vanilla's
  `getJumpPower` 0.42 at jump strength 0.42 with no Jump Boost; `travelInAir` moves first, then gravity 0.08 and drag 0.98).
- `MovementInputUpdateEvent` listener: while the player holds `horizon_java_test:compass` (new item), set `keyPresses` to forward
  only and `moveVector` to `(0, 1)` through the test mod's own accessor mixin on `ClientInput` (`testmod/res/horizon_java_test.mixins.json`).
  Check `input_event_walks` after 20 ticks of holding it with no key down: the player's position moved forward (along its look,
  yaw 0 → +z) by more than 3.5 and less than 4.5 blocks. Independent value from vanilla's `travelInAir` on stone: acceleration
  a = 0.1 × 0.98 × 0.21600002 / 0.6³ ≈ 0.098 a tick, kept fraction 0.6 × 0.91 = 0.546, so tick k moves a·(1 − 0.546ᵏ)/0.454 and
  20 ticks from rest sum to ≈ 4.06 blocks.
  Check `input_event_seen_every_tick`: 20 events in those 20 ticks, each with the `keyPresses` the window held (all false) on entry.
- `LivingFallEvent` listener: holding `horizon_java_test:feather` (new item), cancel; check `fall_canceled_client` after a 6-block
  drop: the event fired once on the client with distance in [5.9, 6.1] and the window played no fall sound (`ClientSounds` saw none).
- Drive in `T/java_mods.py bridges`: for each item, `command fill` a clear space, `give`, `select`, the action (`key space tap`;
  `wait 20` with the compass; `tp ~ ~6 ~` with the feather), `wait 40`, `command clear @s <item>`.

## Tests
- Rust: the input reply's bits map to `Keys` fields one by one (a table typed in the test, from vanilla's `Input` field order);
  a reply whose vector equals the sent one leaves `keys.analog` as it was; `apply_motion_writes_to` puts a velocity into the
  `PlayerMotion` and a yaw into `pending`.
- Java unit `LockstepEventsCheck` (no game): the `INPUT` request bytes decode to the `Input` record vanilla's constructor builds from
  the same seven booleans, and the reply encodes back to the same byte.
- `cargo test -p horizon-client --features java-mods --lib java_lockstep`, `cargo test -p horizon-client --lib motion` (the hook
  call must not change native results: the motion tests and `python3 tools/parity.py` harness `player_motion` or what
  `--changed-files` names for `L/motion.rs`, at most two harnesses), clippy with and without the feature, `java_mods.py build`
  (private dir), `unit`, `check`, `bridges` (`spring_jump_apex`, `input_event_walks`, `input_event_seen_every_tick`,
  `fall_canceled_client`).

## Done bar
Tests pass; the two motion harnesses are unchanged; `bridges` shows the four new checks ok.

## Live test (lead)
1. Play install, Meka-Suit boots with the Hydraulic Propulsion unit (jump boost on): jumps are visibly higher than without it, the
   same height as on a Forge client with the same jar and module setting; the server doesn't pull the player back down.
2. Showcase install, Traveler's Backpack worn: do what its docs say changes movement input (for example sitting in its sleeping
   bag or a hose action); the window's player follows (on Forge the same).
3. Fall 10 blocks without gear: one fall sound, damage as before (server side unchanged).
