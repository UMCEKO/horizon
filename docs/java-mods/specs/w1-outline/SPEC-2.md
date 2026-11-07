# SPEC-2 (W1 outline): native composes mods' outlines with its own, for the current hit only

Read first: DESIGN.md §2–3, SPEC-1 (landed), `G/java_screens.rs` `java_client_frame`, `java_client_collect`, `Worker`
(`send_frame`, `collect`), `FrameJob`; `G/java_world_renders.rs`; `R/world_renders.rs` (`outline()`, `draw`); `game.rs`
~11840 (`world_renders::draw`) and ~16582–16645 (the outline). Paths as SPEC-1.

Depends on: SPEC-1. Size: about 350 changed lines. Comments in code: one line at most, only for a non-obvious why.

## Goal

What a player sees matches Forge: a canceled outline is gone, a replaced one shows only the mod's lines (Mekanism's model
wireframe, AE2's part box), an extended one shows both, and none of them is ever drawn for a block the crosshair has left.
On a block change the frame waits for Java's answer (decision D2), at most once per distinct key.

## Rust

### R1. The decision (pure, `G/java_world_renders.rs`)

```rust
/// What native draws of the block outline this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OutlineDraw {
    /// Vanilla's lines, drawn natively.
    pub lines: bool,
    /// The mods' outline pieces of the world recording.
    pub pieces: bool,
}

/// The key an answer must match: block, face (3D data value), state, gate, high contrast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OutlineKey { pub pos: [i32; 3], pub face: u8, pub state: u32, pub render: bool, pub high_contrast: bool }

pub(super) fn outline_draw(answer: Option<&OutlineAnswer>, key: Option<OutlineKey>, native_canceled: bool) -> OutlineDraw
```
Rules, in order (from the patched bytecode, DESIGN §1.1, and D4):
1. `native_canceled` (Horizon's own mods, `native_block_highlight`) → `{lines: false, pieces: false}`.
2. `key` is `None` (no block hit) → `{false, false}`.
3. `key.render` false → `{false, false}` (vanilla's gate: `submitBlockOutline` is not called).
4. No answer (no mod changes outlines, or no Java client) → `{lines: true, pieces: false}`: today's behaviour.
5. The answer's block, face, state, render and contrast differ from `key` → `{true, false}` (stale: never draw old pieces).
6. Matching answer: `None` → `{true, false}`; `Canceled` → `{false, false}`; `Vanilla` → `{true, true}`; `Replaced` → `{false, true}`.

`OutlineKey` comes from `OutlineHit` plus the window's state id at the hit (`self.world.block(..).0`).

### R2. One decision per frame, made before drawing

- `thread_local! static OUTLINE: Cell<OutlineDraw>` in `java_world_renders.rs`, and
  `pub(super) fn java_outline_draw() -> OutlineDraw` (default `{lines: true, pieces: false}` without java-mods: the
  not-feature arm in `java_hooks.rs` style returns that constant).
- `impl Game { pub(super) fn java_outline_frame(&mut self) }`, called first thing in `java_client_frame`
  (`G/java_screens.rs`), before its `collect`:
  1. Not joined, no host, or `world_renders::outline()` has never been `Some` since the join (no mod changes outlines):
     store `{true, false}` and return. No wait ever happens in that case.
  2. `let hit = self.java_outline_hit(); let key = ...;`
  3. If the current answer does not match `key` and `key != last_synced` (a thread-local `Option<OutlineKey>`):
     `last_synced = key`; `self.java_client_collect(true)`; if the answer still does not match, send one
     `FrameJob { hud: None, world: Some(self.java_world_frame_request_now()) }` through the worker and
     `self.java_client_collect(true)` again. `java_world_frame_request_now` is `java_world_frame_request` without the
     `world_frame_wanted` test (split the builder out).
  4. If only the hit's point moved since the last request (same key, different point): set a thread-local `point_moved` that
     `java_world_frame_request` treats like `animating` (`world_frame_wanted(animating || point_moved, recorded, busy)`),
     cleared when the request is built. This keeps AE2's part choice inside one block face current without waiting.
  5. Store `outline_draw(world_renders::outline().as_ref(), key, false)` in `OUTLINE`. The native-mod cancel is applied in
     the render (R3), where it is computed today.
- The `last_synced` rule makes an answer that never matches (Java sees another state at that block, a column it lacks) cost
  one wait, not one per frame.

### R3. The render (`game.rs`)

- ~16596: after `native_block_highlight` sets `canceled`, `#[cfg(feature = "java-mods")]` take
  `let java = java_outline_draw();` and filter the hit out of the native lines when `!java.lines` (the closure's
  `.filter`, beside the canceled one). The native-mod cancel also hides pieces: keep its `canceled` in a local the
  `world_renders::draw` call reads.
- ~11840: `horizon_client::world_renders::draw(&recorded, &mut world, pieces)` with
  `pieces = java_outline_draw().pieces && !native_canceled_this_frame`. If the native cancel is only known later in the
  frame, move the cancel check into `java_outline_frame` instead (call `native_block_highlight` there once and keep its result
  for the render) so both sites read one value; do not call it twice per frame.
- Without the feature nothing changes.

### Docs

`docs/java-mods.md` "Block outlines" row: native draws vanilla's lines unless the answer for the aimed block says canceled
or replaced; mods' outline geometry is drawn for the aimed block only; a change of aimed block waits for the answer.
`docs/java-mods-map.md`: `java_outline_frame` in the world recorder section.

## Tests

- `outline_draw` table test in `G/java_world_renders.rs`, every row of R1 with the expected pair written out (not computed):
  native cancel wins over `Replaced`; no hit; gate false with a `Replaced` answer → nothing; no answer → lines only;
  stale `Replaced` (other position) → lines only; stale by face, by state, by high contrast → lines only; matching
  `None`/`Canceled`/`Vanilla`/`Replaced` → `(true,false)`, `(false,false)`, `(true,true)`, `(false,true)`.
- `world_frame_wanted` gains the point-moved input: a test that `point_moved` alone asks a frame, and not while `busy` or on
  a `recorded` frame (the same as `animating`).
- The once-per-key rule: a test of the small state machine (extract it as `fn should_sync(answer_matches: bool, key, last_synced) -> bool`):
  first mismatch syncs, the same mismatching key again does not, a new key does.

## Done bar

- The tests above; `cargo test -p horizon-client --features java-mods --lib java_`; clippy `-D warnings` with and
  without `--features java-mods`; fmt.
- With a pack whose mods don't touch outlines (`HORIZON_JAVA_DEBUG`): no `client_world_frame` request comes from a hit
  change (count the `Horizon: world frame` lines while turning).
- `HORIZON_PROFILE` on the live test: report the window frames that waited (add one span `java_outline_wait` around the two
  `collect(true)` calls) with their mean and max.

## Live test (lead; then the user, it is visual)

Play pack, creative, `HORIZON_JAVA_DEBUG=1 HORIZON_PROFILE=...`:
1. `setblock` a `mekanism:metallurgic_infuser` and a `mekanism:digital_miner` (a multiblock with bounding blocks) beside
   stone. Screenshot each aimed: Mekanism's outline follows the model's edges; on the miner the whole machine's wireframe
   shows from any of its blocks; on stone the vanilla outline. No frame shows both.
2. AE2: place `ae2:fluix_glass_cable` and put an `ae2:terminal` part on it. Aim at the terminal: only the terminal's box is
   outlined; aim at the cable: only the cable. Hold another `ae2:terminal` and aim at a free side of the cable: the
   placement preview box shows where it would go.
3. Mekanism configurator (`mekanism:configurator`) in hand, aim at a machine side: that side's coloured overlay shows.
4. Sweep the crosshair quickly across these blocks: no flash of the vanilla outline on Mekanism blocks, no outline left
   behind on a block the crosshair left. Report the `java_outline_wait` mean/max.
5. F1: no outline at all (vanilla's gate). Adventure mode without a tool: no outline.
User steps: the same five, compared with a real NeoForge client if one is at hand.
