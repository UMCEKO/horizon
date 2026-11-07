# W4: camera, FOV, fog and movement hooks (design, revision 1)

Status: design only, nothing implemented. Specs beside this file: `SPEC-1.md` … `SPEC-11.md`.
Binding: RULES.md, COVERAGE-SPEC §1 (rule 0: Java only a modloader, detected behaviours only), `entities/DESIGN.md` §0
(decisions 1–6, and its "Corrections": the HUD rule for per-frame events), W5 D1–D12, W12 §0/§2.

Paths: `B/` = `mods/horizon/src/java-mods-host/boot/dev/horizonlab/javamods/boot/`, `J/` = `mods/horizon/src/java-mods-host/`,
`G/` = `crates/horizon-client/src/game/`, `L/` = `crates/horizon-client/src/`, `R/` = `crates/horizon-client-render/src/`,
`T/` = `tools/`, `P/` = the NeoForge-patched client jar
`agents/w12/install/libraries/net/neoforged/minecraft-client-patched/26.3.0.101/minecraft-client-patched-26.3.0.101.jar`.
Scratch evidence: `w4/scratch/*.txt` (javap -c -p of the patched classes), `w4/run1/` (the failed live run, §1.6).

## 1. The gap, with evidence

### 1.1 Where NeoForge fires W4's events (javap -c of `P/`)

| Event | Fired from (patched vanilla) | Rate | Inputs → result |
|---|---|---|---|
| `ComputeFovModifierEvent` | `Camera.tick` → `tickFov` → `AbstractClientPlayer.getFieldOfViewModifier` → `ClientHooks.getFieldOfViewModifier(player, modifier, fovScale)`; **not** posted on the spyglass branch (`firstPerson && isScoping` returns 0.1 first) | per tick, in `GameRenderer.tick`, which `Minecraft.tick` runs **before** `level.tickEntities()` | the vanilla modifier → `getNewFovModifier()`, then vanilla eases it (×0.5, clamp 0.1–1.5) |
| `MovementInputUpdateEvent` | `LocalPlayer.aiStep`: `input.tick()` → `ClientHooks.onMovementInputUpdate(player, input)` → tutorial | per tick, inside the local player's tick | `input.keyPresses` (public `Input` record) and `input.moveVector` (protected `Vec2`) |
| `LivingEvent.LivingJumpEvent` | `LivingEntity.jumpFromGround` → `CommonHooks.onLivingJump(this)` (both sides) | per jump | listeners change the entity's delta movement |
| `LivingFallEvent` | `LivingEntity.causeFallDamage` → `CommonHooks.onLivingFall` (both sides) | per landing | distance, multiplier, cancel |
| `PlayerTickEvent.Pre/Post` | `Player.tick`: `EventHooks.firePlayerTickPre` first, `firePlayerTickPost` last; the body between holds `super.tick()` (→ `LivingEntity.tick` → `aiStep` → `travel` → `move`) | per tick, both sides | listeners write anything |
| `ViewportEvent.ComputeCameraAngles` | `Camera.alignWithEntity` (non-minecart branch only): `new ComputeCameraAngles(camera, partial, viewYRot, viewXRot, 0)`, then `setRotation(yaw, pitch, roll)` = `rotationYXZ(π − yaw·d, −pitch·d, −roll·d)` (patched three-angle overload, new `roll` field) | per frame (`Camera.update`) | yaw, pitch, roll |
| `CalculateDetachedCameraDistanceEvent` | `alignWithEntity`, detached only: `ClientHooks.getDetachedCameraDistance(camera, mirrored, scale, distance, mountScale, mountDistance)` before `getMaxZoom` | per frame, third person | distance |
| `ViewportEvent.ComputeFov` | `Camera.modifyFovBasedOnDeathOrFluid` ends with `ClientHooks.getFieldOfView(gameRenderer, camera, partial, fov)`; called by `calculateFov` **and** `calculateHudFov` | twice per frame | fov |
| `ViewportEvent.ComputeFogColor` | `FogRenderer.computeFogColor`: after vanilla's RGB (locals 14–16), `ClientHooks.getFogColor(camera, partial, level, renderDistance, darken, r, g, b, dest)` | per frame, first step of `setupFog` | RGB |
| `ViewportEvent.RenderFog` | `FogRenderer.setupFog`, last step before `areturn`: `ClientHooks.onSetupFog(environment, fogType, camera, partial, (float) renderDistanceChunks, fogData)`; not cancellable; carries the `FogData` itself | per frame | every `FogData` field |
| `CalculatePlayerTurnEvent` | `MouseHandler.turnPlayer`: `ClientHooks.getTurnPlayerValues(sensitivity, smoothCamera)` | per frame while the mouse is grabbed | sensitivity, cinematic |

Event classes (javap of `neoforge-26.3.0.101-universal.jar`): getters/setters as in the table; `MovementInputUpdateEvent` is a
`PlayerEvent`; none of the viewport events is cancellable.

### 1.2 What Horizon does today

- **The Java client's `LocalPlayer` ticks in Java, but its movement is discarded.** `ClientBridge.tick` (`B/ClientBridge.java:491`) runs
  `minecraft.tick()` *after* the window's tick (`G/game.rs:7909`, `java_client_tick_send` at the end of the remote branch), so every
  event above that fires in Java's tick fires after native already moved the player. `B/mixin/LocalPlayerMoveMixin` cancels
  `LocalPlayer.move` and copies native's last result in (`ClientBridge.move`). Java's vanilla `aiStep`/`travel` still run (a W0
  duplicate) and their output is thrown away.
- So: a mod writing the local player's velocity in `ClientTickEvent.Pre` (Mekanism's jetpack, §1.3) is overwritten at `move`;
  `MovementInputUpdateEvent` fires in Java's `aiStep` and its result is never read by native; `LivingJumpEvent` fires on Java's own
  jump decision, not native's; `ComputeFovModifierEvent` fires from Java's `Camera.tick` (`B/mixin/GameRendererMixin.horizon$tick`
  calls `mainCamera.tick()`) on Java's state and its result is discarded.
- **Per-frame events never fire**: `MinecraftTickMixin.horizon$frame` cancels `renderFrame`, so `Camera.update`, `setupFog` and
  `turnPlayer` never run in Java: `ComputeFov`, `RenderFog`, `ComputeFogColor`, `CalculatePlayerTurnEvent`,
  `CalculateDetachedCameraDistanceEvent` are 0. `ClientBridge.turnPlayer` sets the look directly.
- **`ComputeCameraAngles` fires at a non-vanilla moment**: `ClientBridge.cameraAndPick` calls `CameraAccess.horizon$align(1.0F)`
  twice per client tick (before and after `minecraft.tick()`), so it posts 40/s with partial 1, and its result is discarded.
- **Native already has a seam for each value** (native mods' hooks, `G/native_hooks.rs`): `native_movement_input` (`G/remote.rs:6282`,
  right after the keys are read), `native_living_jump` / `living_fall` (`L/motion.rs:2087`, `L/native_client_hooks.rs`),
  `native_compute_fov_modifier` (`G/fov.rs` `tick_fov`), `native_compute_fov` (`fov_degrees`), `native_camera_distance`
  (`G/perspective.rs:331`), `native_compute_fog_color` and `native_render_fog` (`G/render_tick.rs:161–205`), `look_options`
  (`G/game.rs:8640`, `turnPlayer`'s inputs). No Java arm exists at any of them (`grep java_ G/fov.rs G/render_tick.rs` is empty).
- **Native has no camera roll**: `R/camera.rs` `set_rotation(y, x)`, `view_matrix(yaw, pitch)`, `view_rotation_matrix(yaw, pitch)`.
- **Native order finding (not W4's to fix, Q3)**: native's `tick_fov` runs after `self.tick()` (`G/game.rs:20051`), i.e. after the
  player moved; vanilla's `Camera.tick` runs in `GameRenderer.tick` before `level.tickEntities()`. Native's FOV target is one tick
  early. W4's Java arm uses vanilla's order (SPEC-4).

### 1.3 What the finish-bar mods use (static analysis; javap signatures of mods only)

`finish/rep-rs/replaces.json` (play pack), `finish/rep-showcase/replaces.json` (top-50 set), `w12/install/analysis/entity-facts.json`,
`*/analysis/mixins/mixins.json`:

| Mod | Mechanism | Evidence | What breaks today |
|---|---|---|---|
| Mekanism | `ClientTickHandler.onTick(ClientTickEvent.Pre)` with `lambda$onTick$0/1(LocalPlayer)` and `IJetpackItem.handleJetpackMotion(PLAYER, mode, thrust, Predicate<PLAYER>)` | javap -p; `CommonPlayerTickHandler.onTick(PlayerTickEvent.Post)` has a **proven server guard** (entity-facts `guards.side = server`), so the client's thrust is the `ClientTickEvent.Pre` one | jetpack and gravitational-modulation thrust overwritten at `move` |
| Mekanism | `CommonPlayerTickHandler.onLivingJump(LivingJumpEvent)`, guard both sides, requires `Player` | entity-facts | Hydraulic Propulsion jump boost never applied to native's jump |
| Mekanism | `CommonPlayerTickHandler.livingFall(LivingFallEvent)`, both | entity-facts | client side is sounds only; server side is W9 SPEC-4-6 |
| Mekanism | `ClientTickHandler.onFog(ViewportEvent.RenderFog)`, `onFogLighting(ComputeFogColor)` | replaces | Vision Enhancement / scuba fog never applied |
| Mekanism | `ItemAttributeModifierEvent` listener (in `Mekanism.<init>`), `CommonPlayerTickHandler.getStepBoost(Player)`, `ModuleHydraulicPropulsionUnit$StepAssist.getHeight()`; Meka gear's facts show `minecraft:attribute_modifiers: []` | replaces, javap -p, boot facts | step assist: native computes equipment modifiers from the item type's default component (`G/game.rs:13866` `apply_equipment`, `G/lan_attributes.rs:124`), so per-stack modifiers from the event never exist natively |
| Mekanism | server-side abilities (flight) through the player view | no abilities bridge in `G/java_bridges/player.rs` or `PlayerSync`; `ServerPlayer.onUpdateAbilities` sends its packet through the view's connection, which reaches only the Java client | Meka-Suit flight never reaches native |
| Traveler's Backpack (top-50) | `NeoForgeClientEventHandler.updateInputEvent(MovementInputUpdateEvent)`, both | entity-facts | discarded |
| Traveler's Backpack | mixins `Player#getFlyingSpeed` RETURN, `Entity#isInWater` HEAD (cancellable) | mixins.json; W12 routing: `getFlyingSpeed` undecided, `isInWater` deferred `view: entity`, hot | native-owned in the window's motion (M2 gap) |
| Jade (both packs) | mixin `FogRenderer#setupFog` TAIL with `locals = true`; javap: `jade$setupFog(Camera, int, DeltaTracker, float, ClientLevel, CallbackInfoReturnable, FogData)`: the only captured local is `fog`, the returned value | W12 report: `body`, `deferred (client: phase C)`, boot log game31 L187 `UNGATED`-listed route | M2 gap |
| Architectury (top-50) | generic forwarders on `PlayerTickEvent.Pre/Post`, both | replaces | listeners present: must see vanilla order |
| No mod in either pack | `ComputeFovModifierEvent`, `ComputeFov`, `ComputeCameraAngles`, `CalculatePlayerTurnEvent`, `CalculateDetachedCameraDistanceEvent` | replaces (both packs, and `tools2/.../out-top50`) | served generically, tested with the test mod |

Out of W4 (drawing, not camera or motion): `RenderArmEvent` (Mekanism, Meka-Suit arm: W2 SPEC-8 armour), `RenderHandEvent`
(Carry On, carried block in first person: W1 world recorder).

`T/mod_entity_facts.py:303` excludes `Player` receivers from `writes`, so Mekanism's jetpack writes (receiver `Player` after
erasure of `PLAYER`) are invisible to today's analysis (SPEC-2 adds them).

### 1.4 The other workstreams' assumptions about W4

- W7 D6 / SPEC-8: "W4 runs the real `FogRenderer.setupFog` in Java when a detected hook exists; W7 adds the eye's fluid having fog
  code to the predicate." Kept as a predicate (`ViewFrame.fogWanted()`, SPEC-6). The *real* `setupFog` is entered only with a
  Java-bodied stub (SPEC-7); NeoForge's own `ClientHooks.getFogColor`/`onSetupFog`, which call the fluid type's
  `modifyFogColor`/`modifyFogRender`, run in both.
- W8 SPEC-3 sends native's real camera (third person, mirrored, real FOV) as `Input::Camera`; SPEC-5 fills the event camera from
  the same values. W8 left `Input` kinds 26–39 to W4/W6; W1-outline and W6 both took 26 and W3 took 30 (Q5): **W4 takes 35 and 36.**
- W9 SPEC-4-6 fires the *server's* `LivingFallEvent`/`LivingJumpEvent` for players at `L/motion.rs` under `as_guest`; SPEC-3 fires
  the *window's* at the same lib sites through one new hook field, with W9's server arm as the other branch.
- W12 §2.5 parks window routes in "phase C". SPEC-10 provides the window-motion context (the lockstep's park) for local-player routes.
- W13's table gives `ItemAttributeModifierEvent` to W11 (no spec yet). SPEC-9 is written here because step assist needs it (Q4).

### 1.5 Measures

- M1 today: `ComputeFovModifierEvent`, `MovementInputUpdateEvent` and `ComputeCameraAngles` fire in Java (wrong moment or wrong
  rate, result discarded); the six per-frame events are absent.
- M2 today: `FogRenderer#setupFog` (Jade), `Player#getFlyingSpeed`, `Entity#isInWater` (local player) native-owned.

### 1.6 Live run: none completed

One offscreen trace was prepared (`run1/scenario.json`, `run1/env.sh`: JFR `MethodTrace` on `LocalPlayer.aiStep/tick`, the eight
`ClientHooks` above, `Camera.tick/alignWithEntity`, Mekanism's tick handlers and `handleJetpackMotion`). The launch failed in 1 s
before the JVM started (`run1/out/home/horizon/logs/latest.log` L9: `Failed to load one of xlib's shared libraries`: forcing
`DISPLAY=:99` needs `parity.sh`'s X11 libraries). The corrected relaunch was refused by the permission system, so no game ran.
Everything above is from javap of the patched jar, the static analyses and the code. What the trace would have shown is a check in
SPEC-1's done bar (event counts per tick with `HORIZON_JAVA_DEBUG`).

## 2. The approach

### 2.1 Tick-time hooks: the lockstep client tick (SPEC-1)

Vanilla's order inside one client tick is: inputs and packets, `ClientTickEvent.Pre`, `gui.tick`, `pick`, **`handleKeybinds`**
(clicks), **`gameRenderer.tick`** (camera, `ComputeFovModifierEvent`), **`level.tickEntities`** (`PlayerTickEvent.Pre`, `aiStep`:
input, `MovementInputUpdateEvent`, jump, `LivingJumpEvent`, `travel`, `move`, fall, `PlayerTickEvent.Post`), level tick, particles,
`ClientTickEvent.Post`. Native's window tick (`tick_remote_level`) follows the same order natively. The Java client's tick has to
interleave with it **at the two points where native needs Java's answer or Java needs native's**:

```
window tick N                                    Java client tick N (worker → Render thread)
─ collect tick N-1 (wait if still running)
─ send batch N (inputs, look) ─────────────────► events, packets, ClientTickEvent.Pre, gui, pick
                                                  handleKeybinds point: PARK K (serve inbox)
─ … remote_handle_input: wait K; clicks ───────► served at K (InteractionKeyMappingTriggered here, as vanilla)
─ release K ───────────────────────────────────► gameRenderer.tick (FOV modifier, SPEC-4)
                                                  tickEntities: setOldPosAndRot, PlayerTickEvent.Pre
                                                  LocalPlayer.aiStep HEAD: PARK M, publish the park record
─ player step: wait M; apply the record ◄───────  (mods' motion writes since the last fill, the FOV target)
   MIU / jump / fall through the inbox ─────────► served at M (events posted on the Render thread)
─ release M with native's result ──────────────► aiStep returns (body native), PlayerTickEvent.Post,
                                                  level tick, animateTick (the tick's picks), ClientTickEvent.Post,
─ rest of the window tick; frames                 HUD / world / shell / view recordings → status
```

- **Lockstep only when detected.** `Lockstep.wanted()` (Java, read at the end of every tick, so a listener registered at runtime
  turns it on for the next tick): a mod listener (`Shells.isMods` owner) on `MovementInputUpdateEvent`,
  `ComputeFovModifierEvent`, `PlayerTickEvent.Pre/Post`, `LivingJumpEvent`, `LivingFallEvent`; or the install analysis fact
  `player_motion_writers` non-empty (SPEC-2). The status carries the bit; the next window tick uses it. Without it today's path
  (Java tick after native's) is unchanged.
- **The inbox.** From the moment the window announces a lockstep tick (before handing the job to the worker) until the tick ends,
  every `client_*` request from a thread other than the job's own is put in `Lockstep.INBOX` instead of the Render executor; Java
  drains the inbox at K, at M and when the tick ends. Java blocks only at K and M, and only M needs the window to continue, so a
  window request is always answered once Java reaches its next park: no deadlock (the announce closes the race of a request queued
  on the executor behind a job that hasn't started).
- **The tick's `animateTick` picks** (native picks them in its tick before the player step) travel in M's release, so Java's
  `level.animateTick` later in the same tick uses them, as today's batch-at-the-end order does.
- **One owner per effect.** In lockstep, `LocalPlayer.aiStep`'s body doesn't run in Java (native's `controlled_simulation_tick` is
  it); the release fills position, velocity, ground, flags and input (as `ClientBridge.move` does today, moved to the park). The rest
  of Java's player tick (`baseTick`, `updatingUsingItem` for mod items' client `onUseTick`, `PlayerTickEvent.Post`) runs as today.
- **Native guard.** `LockstepTick` (Rust, RAII) releases a pending K, then M ("no move"), on every exit path of the window's tick.
- Cost: the window waits at K for Java's pre-keybinds part (events, packets, `ClientTickEvent.Pre` listeners). Measured per spec
  (`HORIZON_PROFILE` spans `lockstep_k`, `lockstep_m`); `GameBridge.LOCK` held by the server thread's Java can stretch it (W14, Q1).

### 2.2 Mods' writes to the local player (SPEC-2)

Setter-level write-through, never a diff: `B/mixin/MotionWritesMixin` records every call on the local player to
`Entity.setDeltaMovement(Vec3)`, `setPosRaw`, `setYRot`, `setXRot`, `setOnGround`, `resetFallDistance`, `LivingEntity.setSprinting`,
`LocalPlayer.onUpdateAbilities` **whose nearest caller outside `net.minecraft.world.entity.*` is a mod's class**
(`StackWalker` + `Shells.isMods`; only for the local player, only while lockstep). Vanilla paths (Java's own `baseTick` fluid push,
packet handlers native also runs) and Horizon's fills are therefore not recorded; a mod's mixin merged into a vanilla entity method
counts as vanilla (its effect is an M2 route, SPEC-10/W12). `setOldPosAndRot` on the local player writes a marker, so native takes
`previous_position` where vanilla took `xo/yo/zo`. The log is the park record at M and the reply of each inbox event; native applies
it in order.

### 2.3 Frame-time hooks: the view job (SPEC-5, SPEC-6)

The user's HUD rule (entities "Corrections"): fire per tick, and per frame only while the result keeps changing. Native computes
each frame's vanilla values (camera angles, detached distance, fov, hud fov, fog colour, fog, turn options) at its seams and keeps
the last frame's as `ViewInputs`. Java fires the listened events with those inputs, in vanilla's frame order (`turnPlayer`,
`ComputeCameraAngles`, `CalculateDetachedCameraDistanceEvent`, `ComputeFov` ×2, `ComputeFogColor`, `RenderFog`), through NeoForge's
own static hooks (`ClientHooks.getFieldOfView`, `getFogColor`, `onSetupFog`, `getTurnPlayerValues`, `getDetachedCameraDistance`;
`ComputeCameraAngles` is posted as the patched `alignWithEntity` does), on the real `mainCamera` filled from native's camera. The
reply marks each value the listeners changed; native applies a changed value in place of its own at the matching seam, before
Horizon's zoom extra and before native mods' hooks. Timing exactly as the HUD: the tick job takes one, the frame job
(`client_view_frame`, in `FrameJob`) asks per frame only while the reply animates; none while a tick runs. Vanilla's body of
`Camera.update`, `setupFog` and `turnPlayer` never runs in Java.

### 2.4 Mixins into these vanilla methods

- `FogRenderer#setupFog` (Jade TAIL capturing only the returned `fog`): a Java-bodied stub (W5 SPEC-7 §1 GUI bridge) whose body
  is native's `FogData` through the two NeoForge hooks, stored in the `fog` local so Mixin's TAIL capture sees it (SPEC-7). A new
  W12 classifier rule: an inject at TAIL/RETURN whose captured locals are only arguments and the returned local is an edge.
- `Player#getFlyingSpeed`, `Entity#isInWater` on the local player in the window's motion: W12 edge routes served through M's inbox,
  with native's value as the stub's body (SPEC-10).
- Everything else on `Camera`, `GameRenderer`, `MouseHandler`, `LocalPlayer.aiStep`, `LivingEntity.travel` (physicsmod, fancymenu,
  polytone: top-50 *outside* the finish set) stays in W12's report as native-owned; out of scope (§4).

### 2.5 Server-side movement effects (SPEC-8, SPEC-9)

- Abilities and attribute modifiers mod code sets on the acting player's view write through to native's player
  (`ServerPlayer.onUpdateAbilities`, `AttributeInstance.addTransientModifier`/`addOrUpdateTransientModifier`/
  `addPermanentModifier`/`removeModifier` on an instance of the acting player's map, from a mod's caller); native then sends its
  own packets.
- `ItemAttributeModifierEvent` at native's equipment collection: when listened, the stack's modifiers come from Java's real
  `ItemStack.getAttributeModifiers()` (NeoForge posts the event inside), cached per stack, so step assist is an ordinary
  equipment modifier natively, synced to the client by native's attribute packets.

## 3. Decisions (with reasons)

- **D1. Lockstep, not a one-tick lag.** Applying mods' pre-move writes on the next tick would start and stop the jetpack a tick late
  (no debt, entities decision 4). Native firing `ClientTickEvent.Pre` itself would reorder it against packets and every mod's client
  logic. Splitting Java's tick at vanilla's two interleaving points keeps every client event in vanilla's order.
- **D2. Park at `LocalPlayer.aiStep` HEAD, not at `move` or `Player.tick`.** At `move` Java's `travel` would already have duplicated
  native's physics; at `Player.tick` mods' client `onUseTick` (in `LivingEntity.tick`) would be lost.
- **D3. Events at native's sites through the inbox; results applied by native.** Inputs are native's values (native owns vanilla);
  listeners run on the Render thread (`Minecraft.gameThread`), as on Forge.
- **D4. The FOV modifier fires in Java's camera tick (vanilla's moment) with native's base**, computed by native's `tick_fov` of the
  previous tick (the state vanilla's `Camera.tick` reads). Java's `Camera.tickFov` body is replaced by that call.
- **D5. Per-frame events follow the HUD rule** (binding user rule); M1 compares them by presence (SPEC-11, Q2).
- **D6. Events posted through NeoForge's public static hooks**, never re-implemented, so fluid fog extensions (W7) and any NeoForge
  logic inside them run unchanged.
- **D7. Detection, not namespaces.** Lockstep and the view job are on only when the bus has a mod listener (`Shells.isMods`) or the
  analysis has a mod writer; no mod id anywhere.
- **D8. Roll is native.** `ComputeCameraAngles` can set roll, so native's camera gains vanilla's three-angle `setRotation`.

## 4. Out of scope (beyond the finish bar), with why

- Mixins into `Camera`, `GameRenderer.render*`, `MouseHandler.turnPlayer/onScroll` bodies, `LocalPlayer.aiStep/tick`,
  `LivingEntity.travel/jumpFromGround`, `AtmosphericFogEnvironment`/`FogRenderer.computeFogColor` bodies: only physicsmod, fancymenu,
  polytone, notenoughanimations (top-50, not in the finish install); W12 phase C routes them when a pack needs them.
- `RenderHandEvent` (Carry On), `RenderArmEvent` (Mekanism): drawing, W1 / W2 SPEC-8.
- Panoramic mode, the spectator camera on another entity (`camera_mob`): no mod in the set hooks them.
- Optimising the inbox handoff (≈ two thread wakes per call) for hot routes (`isInWater`): W14's crossing work.
- Native's own `tick_fov` order (§1.2): a vanilla-parity fix outside the Java-mods layer (Q3).

## 5. Specs

| Spec | Scope | Depends on | Accept (user) |
|---|---|---|---|
| [SPEC-1](SPEC-1.md) | Lockstep client tick: parks K and M, inbox, guard, detection by listeners, `aiStep` body native | none | nothing visible changes; M1 order of client tick events matches Forge |
| [SPEC-2](SPEC-2.md) | Mods' writes to the local player (write log), analysis fact `player_motion_writers` | SPEC-1 | Mekanism jetpack and Meka-Suit thrust |
| [SPEC-3](SPEC-3.md) | `MovementInputUpdateEvent`, client `LivingJumpEvent` and `LivingFallEvent` at native's sites | SPEC-1 (SPEC-2 for writes inside them) | Meka-Suit jump boost; Traveler's Backpack input |
| [SPEC-4](SPEC-4.md) | `ComputeFovModifierEvent` at Java's camera tick with native's base; native eases Java's target | SPEC-1 | test mod's FOV listener |
| [SPEC-5](SPEC-5.md) | The view job; `ComputeCameraAngles` (roll), `CalculateDetachedCameraDistanceEvent`, `ComputeFov` ×2, `CalculatePlayerTurnEvent`; `cameraAndPick` stops posting | W8 SPEC-3 helps, not required | test mod's roll and zoom |
| [SPEC-6](SPEC-6.md) | `ComputeFogColor`, `RenderFog` in the view job; `fogWanted` predicate | SPEC-5 | Meka-Suit Vision Enhancement fog |
| [SPEC-7](SPEC-7.md) | `FogRenderer#setupFog` Java-bodied stub; return-local edge rule; Jade's TAIL served | SPEC-6, W5 SPEC-7 §1, W12 SPEC-2 | M2: no native-owned `setupFog` |
| [SPEC-8](SPEC-8.md) | Server: abilities and attribute modifiers from mod code reach native | none | Meka-Suit flight |
| [SPEC-9](SPEC-9.md) | Server: `ItemAttributeModifierEvent` at native's equipment collection | none | Meka-Suit step assist |
| [SPEC-10](SPEC-10.md) | Local-player routes in the window's motion (`getFlyingSpeed`, `isInWater`) through M | SPEC-1, W12 SPEC-2, W5 SPEC-7 §1 | M2: Traveler's Backpack's two keys gated |
| [SPEC-11](SPEC-11.md) | M1 for W4: frame events by presence; the W4 scenario | SPEC-1..6 for the scenario's checks | — |

Order: SPEC-1, then SPEC-2, SPEC-3, SPEC-4 (any order); SPEC-5, then SPEC-6 (independent of 1–4); SPEC-8 and SPEC-9 any time;
SPEC-7 and SPEC-10 after W5 SPEC-7 §1; SPEC-11 last (its tooling part any time).

### Wire allocations (W4)

- Client ops (`ClientBridge.OPS`): `client_lockstep_announce`, `client_lockstep_wait`, `client_lockstep_call`,
  `client_lockstep_release` (all four exempt from the Render executor: they run on the caller), `client_view_frame` (Render thread,
  via `FrameJob`).
- `Input` kinds in the tick batch: **35** `FovBase` (SPEC-4), **36** `ViewInputs` (SPEC-5).
- Tick status, appended after the last field present when each spec lands: `bool lockstep` (SPEC-1); the view block (SPEC-5).
- Lockstep inbox kinds (`client_lockstep_call`): 1 `INPUT` (SPEC-3), 2 `JUMP` (SPEC-3), 3 `FALL` (SPEC-3), 4 `ROUTE` (SPEC-10),
  5 `OP` (any other window request, SPEC-1).
- Park record and write-log entry kinds: SPEC-2 (`VELOCITY` 1, `POSITION` 2, `YAW` 3, `PITCH` 4, `GROUND` 5, `FALL_RESET` 6,
  `SPRINT` 7, `ABILITIES` 8, `OLD_POS` 9).
- Server ops: `player_jumped` is W9's; SPEC-9 `item_attribute_modifiers`.

### Rules repeated in every spec

Clean room (javap signatures and runtime traces only for NeoForge and mods; vanilla source freely); no per-mod code; no polling or
fallbacks; **comments in code one line at most**; no commits; private `HORIZON_JAVA_MODS_DIR` while a Java change is unproven; the
test budget of RULES.md (touched modules, clippy, fmt, `pr_ready.py --quick`, at most the named harnesses); games only through
`hz_scenario.py`/`live.sh` with the gate, headless.

## 6. Open questions for the user

- **Q1.** In lockstep the window waits each tick for the Java client's tick up to its keybinds point (expected well under 1 ms;
  `GameBridge.LOCK` held by the server thread's Java can stretch it until W14 removes the lock). Accept, with the per-spec
  `lockstep_k` measurement as the gate?
- **Q2.** Per-frame events fire per tick plus per frame while changing (the HUD rule), and only while a mod listens. M1 then
  compares `ViewportEvent.*`, `CalculatePlayerTurnEvent` and `CalculateDetachedCameraDistanceEvent` by presence and order when the
  install has a listener, not by count (SPEC-11). Accept?
- **Q3.** Native's `tick_fov` runs after the player's move; vanilla's camera tick runs before `tickEntities`. File it as a separate
  vanilla-parity issue?
- **Q4.** SPEC-8 (player abilities/attributes write-through) and SPEC-9 (`ItemAttributeModifierEvent`, W11 in W13's table) are
  server work written here because W4's accept (Meka-Suit flight, step assist) needs them. Keep them in W4 or move them?
- **Q5.** `Input` kind 26 is claimed by both W1-outline and W6, and 30 by W3; W4 takes 35–36. The lead reconciles the table.
- **Q6.** The write log treats a write as a mod's when its nearest caller outside vanilla's entity classes is a mod's class. A
  mod's mixin merged into a vanilla entity method therefore counts as vanilla (left to W12's routes). Accept?
