# SPEC-10 (W4): mods' mixins on the local player's movement methods, routed in the window's step

Design: `DESIGN.md` §1.3 (Traveler's Backpack rows), §2.4, D3. Size: ~380 changed lines (Java ~90, Rust ~290). Depends on: SPEC-1
(the M park and its inbox), W12 SPEC-2 (`routes.bin`, `Routing`, route stubs), W5 SPEC-7 §1 (Java-bodied stubs, `GuiRoutes`,
`gui_route!`). Works for the window only; the server copy of these methods is W12's entity families (`deferred: view: entity`).

Read first: W12 `DESIGN.md` §2.1–2.6 (edge routes, the stub, contexts, hot routes), `SPEC-2.md`; W5 `SPEC-7.md` §1; SPEC-1;
`L/motion.rs` `flying_speed` (2797) and every `in_water`/`was_touching_water` read in `L/motion.rs` and `L/motion/*.rs` (20 reads);
vanilla `Player.getFlyingSpeed`, `Entity.isInWater` and their callers in `LivingEntity.travel`, `LivingEntity.aiStep`,
`LocalPlayer.aiStep`, `Entity.updateSwimming` (`rea_vanilla.py method net.minecraft.world.entity.Entity isInWater` for the callers).
Routes (`w12/install/analysis/routing.json`): `Entity#isInWater()Z` edge (Traveler's Backpack HEAD, cancellable, hot);
`Player#getFlyingSpeed()F` undecided (Traveler's Backpack RETURN): an edge by W12 §2.1 once SPEC-1 of W12 classifies it.

Code comments: one line at most.

## Goal

When a loaded mod has an edge route on one of these two methods, native's step for the window's player calls the route at each
place vanilla calls the method, during M (SPEC-1), with native's own value as the method's body: Mixin's HEAD/RETURN handlers run on
the Java client's `LocalPlayer` and their result is what native uses. M2: both keys gated for the window.

## 1. Seams in the lib (`L/motion.rs`, `L/native_client_hooks.rs`; ~140 lines)
- `PlayerMotion::is_in_water(&self) -> bool` (vanilla's `isInWater()`: `wasTouchingWater`) and `MotionWorld::flying_speed` already
  exists. Replace each read of `fluids.was_touching_water` that ports a vanilla **call** of `isInWater()` (check each against the
  vanilla caller list; a vanilla read of the field itself stays a field read) with a call through
  `crate::native_client_hooks::route_bool(ROUTE_IS_IN_WATER, native_value)`, and `flying_speed`'s result with
  `route_f32(ROUTE_FLYING_SPEED, value)`.
- `NativeClientHooks` gains `pub route_bool: fn(u16, bool) -> bool`, `pub route_f32: fn(u16, f32) -> f32` (identity in every table
  literal, `menu.rs:3662` included); constants `ROUTE_IS_IN_WATER = 1`, `ROUTE_FLYING_SPEED = 2` in `L/native_client_hooks.rs`
  with the vanilla keys in their doc lines, and a `gui_route!`-equivalent marker at each call (`horizon_client::window_route!("key")`,
  a new no-op macro in `L/lib.rs`; extend `T/java_routing.py`'s gate scan to count `window_route!` as it counts `gui_route!`).
- A fast path that costs nothing unrouted: the fn pointer's gameplay side tests one `AtomicBool` per key (set at boot from
  `horizon_java_host::routes()`), as W12's `route!` does.

## 2. Gameplay (`L/entry.rs`, `G/java_lockstep.rs`; ~80 lines)
- `L/entry.rs` installs `route_bool`/`route_f32` → `crate::game::java_hooks::java_window_route_bool/f32(key, value)` (free fns, two
  arms; not-arm returns `value`).
- Java arm: unrouted key → `value`. Routed and `CLIENT.stage == PlayerHeld` on this thread → `java_lockstep_call(4 ROUTE, key,
  value)` and the reply's value. Otherwise (server thread, or the window outside M) → `value` (the server copy is W12's; the window
  never calls these outside its step).
- Profile: one span `lockstep_route` (count and mean): these are hot (W12 §2.6); the done bar reports the per-tick total.

## 3. Java (`B/LockstepEvents.java` kind 4, `B/GuiRoutes.java`, `RES/gui_bridges.tsv`; ~90 lines)
- `gui_bridges.tsv` rows: `net/minecraft/world/entity/Entity#isInWater()Z	local_motion` and
  `net/minecraft/world/entity/player/Player#getFlyingSpeed()F	local_motion`.
- `GuiRoutes.backed(this)`: true when `this == Minecraft.getInstance().player` and `Lockstep.routeValue` is set (only during a kind-4
  request); `GuiRoutes.body("local_motion", this, args)` returns `Lockstep.routeValue` (boxed), so the stub's body is native's value.
- Kind 4 handler: `int key`, then `bool` or `float` by key; set `routeValue`, call the real method on the local player
  (`player.isInWater()` / `player.getFlyingSpeed()`: Mixin's handlers and the stub run), clear `routeValue`, reply the result.
- Not routed (no mod patches the key): kind 4 is never sent (Rust's fast path).

## Test mod
Mixins (`testmod/res/horizon_java_test.mixins.json`): `@Inject(method = "isInWater", at = @At("HEAD"), cancellable = true)` on
`Entity` that returns `true` for the local player while it holds `horizon_java_test:fins` (new item); `@Inject(method =
"getFlyingSpeed", at = @At("RETURN"), cancellable = true)` on `Player` doubling the value while holding `horizon_java_test:wings`.
With the test mod in `bridges`, both keys are routed.
Checks (client, `ClientMotion.java`): `fins_swim_on_land`: holding the fins on dry stone with `w` held 20 ticks, the window's player
moves with vanilla's water travel, not the walk: `travelInFluid` adds 0.02 × 0.98 a tick and keeps 0.8 of it, so tick k moves
0.098·(1 − 0.8ᵏ) and 20 ticks from rest sum to ≈ 1.57 blocks (assert 1.45–1.70; SPEC-3's walk is 4.06); and the route's handler ran
during M (a counter > 0); `wings_double_fly_speed`: creative
flight with `w` held 20 ticks from rest covers 2.0× (assert 1.99–2.01) the horizontal distance of the same flight without the wings:
in vanilla's air travel the acceleration is `getFlyingSpeed()` times the input and the kept fraction 0.91 doesn't depend on it, so the
distance is linear in the flying speed.

## Tests
- Rust: unrouted keys never call the gameplay fn (a counting test hook); the lib's water reads that port a vanilla `isInWater()` call
  go through `route_bool` (a test hook returning true makes `travel` take the fluid branch on dry ground: compare with native's own
  water travel on the same input, not with a constant).
- Java unit: `RouteStubCheck` (W12's) gains a `local_motion` row whose body returns the request's value.
- `cargo test -p horizon-client --lib motion`, `--features java-mods --lib java_lockstep`, clippy both ways, `java_mods.py build`
  (private dir), `unit`, `check`, `bridges`; `python3 T/java_routing.py gates`, `lint`.
- Harnesses: the two motion harnesses `parity.py --changed-files` names for `L/motion.rs` (at most two): unchanged.

## Done bar
Tests pass; harnesses unchanged; `java_routing.py report` on the showcase install lists both keys `gated` for the window (the server
copy stays `deferred (view: entity)` until W12/W9 serve it); `lockstep_route` per-tick total reported.

## Live test (lead; showcase install)
1. Traveler's Backpack worn, with whatever its docs say changes flying or water behaviour (its upgrades): the window's player behaves
   as on Forge with the same jar.
2. Without the backpack: walking, swimming and creative flight unchanged (no per-tick cost beyond the fast path: profile shows no
   `lockstep_route` calls).
