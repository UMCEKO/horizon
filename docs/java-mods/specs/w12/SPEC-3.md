# W12 SPEC-3: the gate (`route!`), the `route` op, and the first live route

Design: `DESIGN.md` §2.2, §2.4 (player and level receivers only), §2.5 (Game context), §2.6, §3. Needs SPEC-2 (`routes.bin`, `horizon_java_host::routes`, route stubs, `bridge::early`). Read first:
- `G/java_mods.rs`: `java_use_request`, `java_request`, `java_level_call`, `java_apply_effects`, `java_refresh_player`;
- `B/GameBridge.java`: `fill`, `use`, `effects`, `safely`, `register`;
- `G/lan_actions.rs` around the `S::SetCarriedItem` arm and `as_guest`;
- vanilla `Inventory.setSelectedSlot` and `ServerGamePacketListenerImpl.handleSetCarriedItem` in `.local/minecraft/26.3/sources/server/`.

## 1. The macro and the gate (`G/java_routing.rs`, new, always compiled)
```rust
//! W12: a vanilla method a loaded mod's mixin patches runs as Java's merged method; its body stays native unless the patch is inside it.
```
- In `game.rs`: add `mod java_routing;` before the other `game/` modules, plus `use java_routing::{Recv, route};`, so every `use super::*;` file sees them.
- `pub(in crate::game) enum Recv { Player, Level, Static }`. Later specs add `Entity(u32)`, `BlockEntity(BlockPos)`, `Block(BlockStateId)`, `Item(..)`.
- Without the `java-mods` feature: `route!` expands to `()`, and `route::early(v)` is `v`. Provide `pub(in crate::game) mod route { pub fn early<T>(v: T) -> T { … } }` with a `cfg` pair calling `horizon_java_host::bridge::early`.
- With the feature:
```rust
macro_rules! route {
    ($game:expr, $key:literal, $recv:expr, ($($arg:expr),* $(,)?)) => {
        #[cfg(feature = "java-mods")]
        {
            static GATE: $crate::game::java_routing::Gate = $crate::game::java_routing::Gate::new($key);
            if let Some(id) = GATE.admitted(&$recv) {
                if let Some(value) = $game.route_call(id, $recv, |out| { $( $crate::game::java_routing::RouteArg::put(&$arg, out); )* }) {
                    return value;
                }
            }
        }
    };
}
pub(in crate::game) use route;
```
- **`Gate`** (`java-mods` only): `pub struct Gate { key: &'static str, state: AtomicU32 }`.
  - `state`: 0 unresolved, 1 off, `n + 2` = `RouteId(n)`.
  - `fn admitted(&self, recv: &Recv) -> Option<RouteId>`:
    - one `Relaxed` load;
    - on 0, resolve against `horizon_java_host::routes::get()`. If that is `None` (no boot yet), return `None` **without** caching. Otherwise store 1 or `n + 2`;
    - for `Recv::Player`/`Level`/`Static`, any on route is admitted;
    - `Admitted::Pending` is never admitted;
    - `Admitted::Only` is checked by later specs' receivers.
  - `#[cfg(test)] fn resolve_with(&self, routes: &Routes)`, for unit tests.
- **`RouteArg`**: `fn put(&self, out: &mut Out)` for `i32`, `i64`, `f32`, `f64`, `bool`, `BlockPos` (`pack()` as a long), `&str`/`String` (`text`). Writes as `wire::Out`.
- **`RouteRet`**: `fn read(reply: &mut In<'_>) -> Option<Self>` for `()`, `bool`, `i32`, `i64`, `f32`, `f64`, plus `Clone + 'static`.

### `gui_route!` (W5's marker, `../w5/SPEC-7.md` §1)
- `macro_rules! gui_route { ($key:literal) => {}; }`, exported beside `route!`.
- It expands to nothing in both builds. It exists so `java_routing.py gates` (SPEC-4) lists the native site that serves a GUI key.
- Put it as the first statement of the fn that serves the key on the window thread. That fn needn't be a `Game` method.

## 2. `Game::route_call` (`G/java_routing.rs`, `cfg(feature = "java-mods")` impl block)
```rust
/// Runs a routed vanilla method as Java's merged method; `None` only when there is no host.
pub(in crate::game) fn route_call<R: RouteRet>(&mut self, id: RouteId, recv: Recv, args: impl FnOnce(&mut Out)) -> Option<R>
```
1. `self.java_bind_states();`.
2. Build the request:
   - `int id`, `byte recv` (0 Player, 1 Level, 2 Static);
   - for `Player`: the bytes of `self.java_use_request()?`;
   - for `Level`: `text dim`, `long game_time`, as `java_level_call`;
   - then `args(&mut out)`.
3. `let reply = self.java_request("route", &request);`
5. Read the reply:
   - `byte status`:
     - 0: `R::read` the value;
     - 1: Java threw. Read `text mod`, `text message`, and `panic!("Horizon: a Java mod threw in {key} ({mod}): {message}")`. That's U6: it propagates as vanilla's crash does. Check how a server-thread panic is reported (`integrated.rs`, `dedicated.rs`) and make sure the message reaches the crash output, without catching it;
   - then `self.java_apply_effects(&mut reply)`;
   - then `self.java_drain_block_events()`.
6. A short read is a broken protocol: panic with `route <key> answered in pieces`. Only a missing host returns `None`.

## 3. The `route` op (`B/Routing.java`, `GameBridge.register`)
- `Crossing.game("route", Routing::call)`, in the `(In, Out)` form.
- `Routing.call(In in, Out out)`:
  1. `int id`, `byte recv`.
  2. Receiver:
     - `Player`: run the same header read as `GameBridge.use` (`fill(in, false)`), then pick by the route's owner class:
       - `Inventory` → `player.getInventory()`;
       - `AbstractContainerMenu` → `player.containerMenu`;
       - otherwise the player.
     - `Level`: `GameBridge.level(dim)` (make it package-private if needed); set the game time as `java_level_call`'s ops do.
     - `Static`: none.
  3. Arguments: read by the descriptor's parameter types: `I J F D Z`, `Lnet/minecraft/core/BlockPos;` (`BlockPos.of(long)`), `Ljava/lang/String;`. Any other type: the route can't run yet. Write status 1 and log once: `route <key>: no argument reader for <type>`.
  4. Invoke `HANDLES[id]` (`invokeWithArguments`, receiver first unless static). **No `safely`** (U6). Time it with `Profile.event("route:" + key, ns)` when `Profile.ON`.
  5. Write status 0 and the result (`V` nothing; `Z I J F D` as `Out`).
     - If it threw, write status 1, `text mod` and `text message` (the exception's string with its first stack frame).
     - `mod` is the owning mod of the first stack frame whose class is a mixin class (`MixinMerged` on the frame's method, or `BootFacts.owns`-style lookup of the class's mod), else `unknown`.
     - Skip the effects after a throw.
  6. Then the effects, exactly as `use` ends: `writeEffects`, depth rules unchanged.
- `KEYS[id]`, `TYPES[id]` and `HANDLES[id]` are filled by SPEC-2's `confirm`. Ids are the `routes.bin` order, which is also `RouteId` in Rust.

## 4. Body bridges
- A route body bridge must leave the receiver's view fresh, because handlers after the body read it. Family `player`: call `game.java_refresh_player()` after the body, inside the bridge entry, as `change_mode` does.

## 5. First live route: `Inventory#setSelectedSlot(I)V`
- **The seam:**
  - In `G/lan_actions.rs`, the `S::SetCarriedItem` arm keeps its range check and its `play.using` handling, as vanilla's `handleSetCarriedItem` does.
  - Its assignment becomes `self.as_guest(host, conn, |g| g.set_selected_slot(p.slot));`. Check `enter_guest` swaps `selected` (it does at line ~777).
  - Order against `play.using`: follow vanilla (`stopUsingItem` before `setSelectedSlot`).
- **The port and body, in a new `G/inventory.rs`.** Check first that no `game/` file already owns hotbar code; if one does, put them there.
  ```rust
  /// `Inventory.setSelectedSlot`: the acting player's hotbar selection.
  pub(in crate::game) fn set_selected_slot(&mut self, slot: i32) {
      route!(self, "net/minecraft/world/entity/player/Inventory#setSelectedSlot(I)V", Recv::Player, (slot));
      self.set_selected_slot_body(slot);
  }
  /// `Inventory.setSelectedSlot`'s body.
  pub(in crate::game) fn set_selected_slot_body(&mut self, slot: i32) {
      if let Ok(slot) = usize::try_from(slot) && slot < 9 { self.selected = slot; }
  }
  ```
  Vanilla throws on a slot out of range. The packet handler already filters, so the body's guard is only defensive. Keep it, without a comment.
- **The body bridge**, in `G/java_bridges/player.rs`:
  ```rust
  route "net/minecraft/world/entity/player/Inventory".setSelectedSlot "(I)V" =>
  fn route_inventory_set_selected_slot(game, slot: i32) -> () { game.set_selected_slot_body(slot); game.java_refresh_player(); }
  ```
  - This also makes a mod's own `inventory.setSelectedSlot(n)` on a server player write through to native, which it doesn't today. That's correct, and it's the reason a route bridge is always installed.
  - If `java_mods.py check` reports a missing `Flat` overload for `I`, add it.

## 6. The test mod's mixins (`J/testmod/`)
- Add `testmod/res/horizon_java_test.mixins.json` (package `dev.horizonlab.javamods.testmod.mixin`, `mixins: ["InventoryRouteMixin"]`, `required: true`, `minVersion` as in `J/boot/res/horizon_java.mixins.json`).
- Add the `[[mixins]] config = "horizon_java_test.mixins.json"` block to `testmod/res/META-INF/neoforge.mods.toml`.
- Make sure `java_mods.py build` packs the json at the jar root.
- `testmod/dev/horizonlab/javamods/testmod/mixin/InventoryRouteMixin.java`, `@Mixin(Inventory.class)`. It reads `((Inventory)(Object)this).getSelectedSlot()`; add a `@Shadow` only if the getter doesn't exist.

  | Handler | At | Does |
  |---|---|---|
  | `horizon$head` | HEAD, cancellable | slot 7: record `canceled = true`, `ci.cancel()`. Otherwise: if `canceled`, `Checks.check("route_head_cancel", () -> getSelectedSlot() != 7)` and clear it |
  | `horizon$tail` | TAIL | `Checks.check("route_tail_sees_body", () -> getSelectedSlot() == slot)`; `tails++` |
  | `horizon$return` | RETURN | `returns++`; `Checks.check("route_return_equals_tail", () -> returns == tails + 1)`. RETURN runs before TAIL at the same return? Check Mixin's order for two injectors at one return and fix the expected relation; don't guess |

  - Throw propagation (U6) is tested in Rust only: a unit test feeds `route_call`'s reply reader a status-1 reply and expects the panic message (`#[should_panic(expected = "a Java mod threw in")]`). Don't make the live session crash.
  - `Checks.check` must be callable from a mixin. If it is package-private, call it through a small public static in `ChecksRouting.java`.
- **The session:** add `select 2`, `select 8`, `select 3` to the `java_mods.py bridges` script (first session only). horizon-ctl counts slots 1–9, so the server sees 1, 7, 2. Check that the window sends `ServerboundSetCarriedItem` for a `select` (`game/agent.rs` ~1475, then `remote.rs`); if it doesn't, report it rather than work around it.
- Add the names to `bridges`' required list: `route_tail_sees_body`, `route_head_cancel`, `route_return_equals_tail`.
- **No route without the mod:** a run without the test mod jar (any `bridges --extra-jars` run of a pack without it, or a Rust unit test) shows no `route` op in `HORIZON_PROFILE`. The Rust unit test in §7 covers the logic.

## 7. Tests
- **Rust unit tests in `G/java_routing.rs`** (`cfg(all(test, feature = "java-mods"))`):
  - `a_gate_without_routes_stays_native`;
  - `an_off_route_is_never_admitted`;
  - `a_resolved_gate_caches_its_id` (`resolve_with` a hand-built `Routes`);
  - `pending_admission_never_crosses`;
  - `a_throwing_route_panics_naming_the_mod`;
  - `route_args_write_as_java_reads_them` (bytes of `put` for each type).
- **Java unit `J/unit/.../boot/RoutingArgsCheck.java`:** the argument reader and return writer round-trip with Rust's byte layout (hard-code the bytes the Rust test prints).
- `cargo clippy -p horizon-client --features java-mods -- -D warnings` and without the feature; `cargo test -p horizon-client --features java-mods --bins java_routing`.
- `python3 tools/java_mods.py build`, `check`, `unit`, then **one** `bridges` run, gate first.
- **Cost:**
  - add `bench_route` to `H/examples/crossing_bench.rs`: a route op with a `Static` receiver and a no-op static target, through `Host::request`;
  - report ns per call from `java_mods.py crossing`;
  - report the `route:` mean for `setSelectedSlot` from one `HORIZON_PROFILE` run of the `bridges` session.

## How the parent tests it
- `bridges` prints the three `CHECK route_* ok` lines.
- The parity harnesses touching inventories stay unchanged: `python3 tools/parity.py --changed-files` for the files you touched, `--list` only; the parent runs them.
- **User test** (needs a window): with the test mod loaded, press hotbar key 8.
  - The client shows slot 8 selected.
  - The server keeps the old slot: the held item doesn't change for other players watching (second client), and using an item uses the old slot's item.
  - That's the expected effect of a server-only cancel. Carry On also cancels on the client (target T1b).

## Not in this spec
Entity, block-entity, block or item receivers; `route_lent!`; the lint (SPEC-4); Carry On itself (SPEC-5).
