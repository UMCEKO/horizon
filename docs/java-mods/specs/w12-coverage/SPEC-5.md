# SPEC-5: gates in library code (`lib_gate`)

Design: `DESIGN.md` §2.4; W12 `DESIGN.md` §2.5 (contexts). Needs **W9 4-2** (crossings from inside a turn: shared mode, deferred writes) for its hook; SPEC-3 (values), SPEC-4 (item admission). Size: ~300 changed lines.

Paths as SPEC-2; `C/` = `crates/horizon-core/src/`. Read first: `G/java_routing.rs` (`Gate`, `route_call`), W12 `DESIGN.md` §2.5, W9 `SPEC-4-2.md` when it exists (its shared-crossing API), and the lib call sites this spec's users will gate: `crates/horizon-items/src/item_data.rs` (enchantment level, ~274), `crates/horizon-client/src/motion.rs` (`MotionWorld::flying_speed`, the `is_in_water` reads), `crates/horizon-client-render/src/effects.rs` (~40, effect merge), `crates/horizon-client/src/mobs/{villager_brain.rs,tame.rs,finalize.rs}`, `crates/horizon-world/src/level/{bonemeal.rs,tick.rs}`.

Comments in code: one line at most, only for a non-obvious why.

## Why
Some routed methods are ported only inside lib crates, which can't name `Game` and so can't hold a `route!` (W12 §2.5: "a gate in a lib fn doesn't compile"). Lifting each to a `Game` method would mean threading `&mut Game` through physics and brain code that W9 4-2 is already making re-entrant. The lib fn instead asks a process-wide hook, which the game installs and which crosses through W9 4-2's shared crossing.

## 1. The primitive (`C/lib_gate.rs`, new; `pub mod lib_gate;` in `C/lib.rs`)
```rust
//! A vanilla method a loaded mod patches, ported in a lib crate: the game's hook runs the merged method in Java.
pub struct LibGate { key: &'static str, state: AtomicU32 }        // 0 unresolved, 1 off, n + 2 = route n
pub trait RouteHost: Sync {
    fn resolve(&self, key: &str) -> Option<u32>;                   // None: off or not booted yet (not cached)
    fn admits_item(&self, route: u32, item: &str) -> bool;
    fn admits_block(&self, route: u32, state: u32) -> bool;
    fn call(&self, route: u32, recv: LibRecv<'_>, args: &[u8]) -> Vec<u8>;  // the reply after the status byte; panics as route_call does on a throw
}
pub enum LibRecv<'a> { Static, Level(&'a str), Block(u32), Entity(u32), Player, Value(&'a [u8]) }
pub fn install(host: &'static dyn RouteHost);                      // OnceLock; a second install panics
impl LibGate {
    pub const fn new(key: &'static str) -> Self;
    pub fn admitted(&self, item: Option<&str>, block: Option<u32>) -> Option<u32>;  // no host: None after one OnceLock load
}
pub struct Args(pub Vec<u8>);  // int, long, float, double, bool, text, bytes: big-endian, the layout of `horizon_java_host::wire::Out`
```
- `LibRecv::Block(u32)` is a Horizon block state id; `Value` is the receiver's own encoded form (e.g. a `MobEffectInstance` as SPEC-3's effect bytes), rebuilt in Java for the call and written back after the value in the reply (§3).
- A macro for call sites:
  ```rust
  macro_rules! lib_route { ($key:literal, $recv:expr, $(item = $item:expr,)? $(block = $block:expr,)? |$args:ident| $write:block) => { … } }
  ```
  It expands to `static GATE`; when admitted it builds `Args`, calls the host and evaluates to `Some(reply)`, else `None`. The caller decodes the reply with `lib_gate::Reply` (the reader twin of `Args`).
- No feature flag: with no host installed the cost is one `OnceLock::get` returning `None`.

## 2. The game's host (`G/java_routing.rs`, `cfg(feature = "java-mods")`)
- `struct GameRouteHost;` implementing `RouteHost`:
  - `resolve`: `horizon_java_host::routes::get()?.id(key)` with the same `Pending`/`None` rules as `Gate::resolve_with`;
  - `admits_item`, `admits_block`: SPEC-4's item and block lists (a list the call site doesn't pass panics, as SPEC-4's `route!` does);
  - `call`: requires an active game: published by W9 4-2's shared crossing inside turns (`horizon_java_host::bridge::active_shared()` or whatever 4-2 names it), and, for `Game` code outside turns that calls a lib fn (mining speed reading enchantment levels, a command), by the server thread's tick: if 4-2 doesn't publish the game for the whole `fn tick`/`tick_world` and `as_guest` scope, add that publication here (one guard object at the top of each, dropped at its end). With none (a lib call from a thread or moment without a published game, such as a streaming worker), panic: `Horizon: route <key> reached from <thread> with no game to answer it`. That is a missing seam to report, not a case to skip.
  - It writes the `route` op's request as `route_call` does (route id, receiver byte, receiver fields, args) with the receiver bytes for `LibRecv`: `Static` 2, `Level` 1 (+ `text dim`, `long time` from the shared game), `Player` 0 (+ the SPEC-2 fill from the shared game: make `java_use_request` and `java_player_snapshot` take `&self`; `SENT` is thread-local, so the update stays legal), `Entity` (W9 3-1's form), `Block` 6 (+ `int` Java state id through `STATES`), `Value` 7 (+ `bytes`).
  - It applies the reply's effects and block events through the shared game's deferred-write queue (4-2), never directly.
- `java_routing::install_lib_host()` is called once where the game first binds Java states (`java_bind_states`), after `routes()` is readable.

## 3. Java (`B/Routing.java`)
- Receiver bytes 6 (`Block`: `Block.BLOCK_STATE_REGISTRY.byId(id).getBlock()`; a route owner of `BlockStateBase` takes the state itself) and 7 (`Value`: built by the owner class's `Flat.to<Owner>(byte[])`; the boot fails at `confirm` for a gated `Value` route whose owner has no `to` method).
- After the value and SPEC-3's written outputs, a `Value` receiver's final form: `Flat.raw(receiver)` bytes. The lib caller replaces its receiver with it: the receiver is the method's in-out parameter, as Java mutates it in place.

## 4. Tests
- `C/lib_gate.rs` tests: `no_host_is_never_admitted` (and the state stays 0); `an_off_key_caches_off` with a test host whose `resolve` counts calls (second `admitted` doesn't call it); `item_filter_applies`; `args_layout` (bytes written out: `int -2, long 1, double -2.25, bool true, text "hi"`, the same bytes `route_args_write_as_java_reads_them` expects for those values).
- `G/java_routing.rs`: `lib_args_match_route_args`: the same values through `lib_gate::Args` and `RouteArg` give equal bytes.
- No live check here: the first users (SPEC-9, 10, 14) add theirs.
- `cargo test -p horizon-core lib_gate`, `cargo test -p horizon-client --features java-mods --lib java_routing`, clippy with and without the feature.

## Done bar
Tests pass; `cargo bench`-free cost check: a test that calls `LibGate::admitted` 10⁷ times with no host installed finishes in under 50 ms in `--release` (report the number; it guards the "one load" claim).

## For the lead
No behaviour change by itself. Nothing to run live.
