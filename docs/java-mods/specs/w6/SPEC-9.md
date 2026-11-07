# SPEC-9 (W6): mods' toasts drawn natively

Design: `DESIGN.md` §1.3, D8. Depends on nothing. Read first: vanilla `ToastManager` (all of it, with `ToastInstance`) and
`Toast`, `javap -c` of the patched `ToastManager.addToast` (where NeoForge's `onToastAdd` returns early), `horizon_client::toasts`
(`Slide`), `G/advancements.rs` (`toast_slots_used`, `draw_toasts`: slot allocation, sounds, slide, draw), `G/java_screens.rs`
(`live`: the status reader, `Input` kinds, `Job`, the HUD frame request and its animating rule, `paint_frame`), `B/ClientBridge.java`
(`tick`, `status`, `hud`/`hudFrame`, `noted`), `B/HudAnimation.java`, `B/Recorder.java`. Comments in code: one line at most.

## Goal

A toast a mod adds with `ToastManager.addToast` (AE2's finished crafting job, RS's task completed, Jade's and RS's
`SystemToast`s) appears in the window's top-right toast column as on NeoForge: it waits for free slots with native's toasts,
plays its sound when it shows, slides in, stays while its own `update` wants it, slides out with vanilla's in/out sounds, and
frees its slots. `getToast` and `SystemToast.addOrUpdate` keep working (the toast stays in Java's `queued` until native finishes it).

## Java

### J1. `B/Toasts.java` (new) and `B/mixin/ToastManagerMixin.java` (client mixins)

- Mixin: `@Inject(method = "addToast", at = @At("TAIL"))` → `Toasts.added(toast)` (TAIL is after `queued.add`; NeoForge's
  cancel returns earlier, confirm with `javap -c`), `@Inject(method = "clear", at = @At("TAIL"))` → `Toasts.cleared()`, and an
  `@Accessor("queued")`.
- `Toasts`: `LinkedHashMap<Integer, Entry>` (`Entry`: the toast, last wanted visibility, last recording bytes,
  `HudAnimation` state), ids from a counter; `fresh` list of ids not yet reported; `cleared` flag.
  - `added`: new entry; log once per class `Horizon: toast <class> places at its own x/y; drawn at the default place` when the
    class declares `xPos` or `yPos` (reflection on declared methods).
  - `apply(int id, boolean finished, long fullyVisibleFor, boolean record)` per window entry: finished → `queued.remove(toast)`,
    `toast.onFinishedRendering()`, forget, and with `HORIZON_JAVA_DEBUG` print `Horizon: toast finished <class>`; else `toast.update(manager, fullyVisibleFor)`, read `getWantedVisibility()`, and when
    `record`, draw `toast.extractRenderState(graphics, font, fullyVisibleFor)` into a fresh `Recorder` at the origin (as the HUD
    recording is made) and keep the bytes; `changed` when they differ from the last; animating by `HudAnimation`'s rule.
- Each toast call in `GameBridge.safely("toast", ...)`: a throw finishes that toast (logged).

### J2. Wire

- Tick input kind 26 `Toasts` (`ClientBridge.events`): `int n` x (`int id`, `bool finished`, `long fullyVisibleFor`), sent with the
  tick batch when native has any Java toast; applied after `minecraft.tick()` (as the HUD is recorded), `record` true.
- Status, appended: `int n` x fresh (`int id`, `int width`, `int height`, `int slots` (`occcupiedSlotCount()`), `text sound`
  (`getSoundEvent()`'s location or "")); `bool cleared`; `int n` x (`int id`, `byte visibility` 0 SHOW 1 HIDE, `bool changed`
  (+ `int len` + recording)); `bool toastsAnimating`.
- Op `client_toast_frame` (`ClientBridge.OPS`): `int n` x (`int id`, `long fullyVisibleFor`); replies like the status's per-toast
  list plus `bool animating`. Asked by the frame job only while `toastsAnimating`, gated exactly as `hud_frame_wanted`.

## Rust

### R1. `G/java_toasts.rs` (new, `cfg(feature = "java-mods")`), state in the `CLIENT` thread-local of `java_screens.rs`

```rust
struct JavaToast { id: i32, width: f32, height: f32, slots: u32, sound: Option<String>, slot: Option<u32>, slide: Slide, show: bool, frame: Vec<u8>, finished: bool }
```
- From status: fresh → queued `JavaToast`s (in arrival order); `cleared` → drop all; per-toast visibility and frames update
  `show` and `frame` (an unchanged entry keeps its frame).
- Tick input: every known toast's `(id, finished, slide.fully_for)`; finished ones are removed once sent.
- `pub(super) fn java_toast_slots(&self) -> u32` (the slot bits of shown Java toasts) and
  `pub(super) fn draw_java_toasts(&mut self, batch: &mut HudBatch, hidden: bool, multiplier: f64)`:
  1. each shown toast: `slide.update_now(|_| show)`; a visibility change plays `ui.toast.in`/`ui.toast.out` (once per call, as
     native's `draw_toasts` does for its own); finished → free its slots, mark finished;
  2. queued toasts take free slots in order with vanilla's `findFreeSlotsIndex` (first run of `slots` consecutive free slots among 5,
     against `toast_slots_used()` which includes native's kinds); on taking one, play its `sound` unless that sound already played
     in this call (`playedToastSounds`);
  3. unless `hidden`: paint each shown toast's frame at `x = gui_width - width * slide.portion`, `y = slot * 32`
     (`Toast.xPos`/`yPos` defaults). `paint_frame` has no origin: add an `origin: [f32; 2]` parameter (all current callers pass
     `[0.0, 0.0]`) that offsets every QUAD and TEXT position and scissor.
- The `multiplier` (Notification Time) is not used here: a mod toast's own `update` reads `getNotificationDisplayTimeMultiplier()`
  from Java's options. Push the window's value into the Java client's `options.notificationDisplayTime()` when it changes
  (with the other options the window already sends, if there is such a path; else note it in the report).

### R2. Hooks (`G/java_hooks.rs` pairs; off arms return 0 / do nothing)

- `G/advancements.rs` `toast_slots_used`: `| self.java_hook_toast_slots()`.
- `G/advancements.rs` `draw_toasts`: at its end, `self.java_hook_draw_toasts(batch, hidden, multiplier)`, so native's kinds take
  free slots first in each update (vanilla has one queue; DESIGN §3 D8 accepts this order).

## Tests

- **Rust (`java_toasts.rs`)**: slot allocation as a pure function `free_run(used: u32, slots: u32) -> Option<u32>` against vanilla's
  `findFreeSlotsIndex` cases (used `0b00101`, slots 2 → `Some(3)`; slots 3 → `None`; used 0, slots 5 → `Some(0)`); the status reader
  on hand-written bytes (one fresh toast, one update with a frame, `cleared` false); a toast whose `show` turns false finishes
  after 600 ms of `Slide` time and frees its slot (drive `Slide::update` with explicit `now` values).
- **Java unit `ToastsCheck`**: `Toasts.apply` on a stub `Toast` (an anonymous class counting `update` calls and returning
  `HIDE` after 100 ms) gives SHOW at 50, HIDE at 150, and `finished` removes it from a plain `ArrayDeque` standing for `queued`
  and calls `onFinishedRendering` once. (Pass the deque in; no `ToastManager` needed.)
- **Test mod**: the client command `horizon_java_test_client toast` (SPEC-5's root; if SPEC-5 hasn't landed, a key press of
  `ClientKeys`' probe key with Shift) adds `SystemToast.add(toastManager, SystemToastId.PERIODIC_NOTIFICATION, title, msg)`; a
  second call of it with another message goes through `SystemToast.addOrUpdate` and must not add a second entry
  (`Checks.check("toast_updates_in_place", () -> manager.getToast(SystemToast.class, id) != null && Toasts.count() == 1)`).
- **`java_mods.py bridges`** drive: run the toast command twice one tick apart, `wait 10`, `screenshot` into the scratch folder (kept
  for the lead), `wait 120` (5 s display at 1.0 plus the slide), then require the CHECK and that the status log line
  `Horizon: toast finished` (debug) appeared once.

## Done bar

`cargo test -p horizon-client --features java-mods --lib java_` and the lib's `toasts` tests; `java_mods.py unit`, `bridges`
green; clippy both ways; no Java call for toasts while none exists (no `Toasts` input, no `client_toast_frame`).

## Live test for the lead (user steps: visual)

Play pack, single player: start an AE2 autocrafting job (or RS's) that finishes in a few seconds, with an advancement earned at
the same time (craft a crafting table in a new world): both toasts show, stacked, neither over the other; the mod's slides in
from the right with its sound, stays about 5 s, slides out. Jade: change a setting that shows its `SystemToast` (its config
screen's reset) and see the toast. Screenshot with both toasts on screen.
