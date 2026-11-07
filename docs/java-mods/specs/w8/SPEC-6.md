# SPEC-6 (W8): window resizes, frame events and sound volumes through vanilla's own paths

Read first: `CLAUDE.md`, RULES.md, DESIGN §1.2 G8 and G9, D1, `B/ClientBridge.java` (`size`, the per-frame screen resize
checks near lines 1016 and 1068, `events`, `status`, `advance`/`timer`), `B/mixin/MousePositionMixin.java` (resync is already
cancelled), `G/java_screens.rs` (`Input`, `java_client_tick_send`, `java_client_window`, the size input), `G/game.rs`
`fn render`, `R/settings.rs` (`master_volume`, `volumes`, `SOUND_SOURCES`). Vanilla: `Minecraft.resizeGui` (line ~1411),
`framebufferSizeChanged`, `Window.calculateScale`, `setGuiScale`, `Options.getSoundSourceOptionInstance` and its listener
(`SoundManager.refreshCategoryVolume`). NeoForge (javap): `ClientHooks.fireWindowResize(Window)`,
`fireRenderFramePre/Post(DeltaTracker)`, `WindowResizeEvent`, `RenderFrameEvent$Pre/$Post`. Real stacks (DESIGN §1.2 G8):
`WindowResizeEvent` ← `fireWindowResize` ← `Minecraft.resizeGui` ← `framebufferSizeChanged` / `<init>`; `RenderFrameEvent$Pre`
← `fireRenderFramePre` ← `Minecraft.renderFrame`.

Depends on: none.

Code comments: one line at most.

## Goal

1. **Resize:** when the window's size or GUI scale changes (and once at the Java client's start, as `Minecraft.<init>` does),
   the Java client runs the real `Minecraft.resizeGui()`: NeoForge posts `WindowResizeEvent`, mods' `resizeGui` patches run
   (Konkrete's HEAD), and the open Java screen is resized by vanilla's own call.
2. **Frames (D1):** while a mod listens to `RenderFrameEvent.Pre` or `Post` (CreativeCore in the showcase), each window frame
   posts `Pre` before any Java frame work of that frame and `Post` after the last, with the client's `DeltaTracker`.
3. **Volumes:** when the player changes a sound volume, Java's `Options` get it through the option's own setter, whose
   listener calls `SoundManager.refreshCategoryVolume` (Melody's edge); mods reading `options.getSoundSourceVolume` read the
   player's volumes.

## Java

- **Resize (`B/ClientBridge.java` `size`, `mixin/WindowScaleMixin.java` new):**
  - After writing the `Window` fields as today, when width, height or scale differ from the last applied: call
    `minecraft.resizeGui()`. The first `size` input of the session always does (the `<init>` call).
  - `WindowScaleMixin`: HEAD-cancellable on `Window.calculateScale(IZ)I` returning the window's scale (native already chose it
    with vanilla's rule and the player's option); vanilla's `setGuiScale` then computes the same scaled sizes `size` wrote.
  - Delete the two per-frame `screen.resize` checks only if `resizeGui` now covers them (a screen opened after a resize is
    initialised with the current size by `Gui.setScreen`; keep them if a test shows a case they still catch, and say which).
- **Frames (`B/FrameEvents.java`, new; op `client_render_frame`):**
  - `decide()` at connect: `listened = modListens(RenderFrameEvent.Pre.class) || modListens(RenderFrameEvent.Post.class)` (the
    helper of SPEC-5, or W5's); log `Horizon: RenderFrameEvent for <n> mod listeners` or `none`.
  - Op `client_render_frame`: `byte phase` (0 pre, 1 post), `float partial`. Pre: `advance(minecraft, partial)` as a screen
    frame does, then `ClientHooks.fireRenderFramePre(minecraft.getDeltaTracker())`; post: `fireRenderFramePost(…)`. Reply:
    the sounds/effects it made, as other client frame ops reply (follow `client_frame`'s reply shape for sounds), or empty.
  - Status: appended `bool frameEvents` (the decision).
- **Volumes (`B/ClientBridge.java` `events`, case `VOLUMES` = 41):** `int n` x (`text source name`, `float value`); for each,
  `minecraft.options.getSoundSourceOptionInstance(source).set(value)` where `source` is the `SoundSource` whose
  `getName()` matches (`master` included). Unknown names are a loud error (the tables must agree).

## Rust

- `G/java_screens.rs`:
  - `Input::Volumes(Vec<(&'static str, f32)>)` kind 41: `master` from `settings.master_volume`, then `SOUND_SOURCES[i]` with
    `settings.volumes[i]`; sent in the tick batch when any value differs from the last sent (as `Particles`), and once at
    the session's start.
  - Parse the appended `frameEvents` bool into `CLIENT.frame_events`.
- `G/game.rs` `fn render`: when `CLIENT.frame_events` (a `java-mods` cfg helper in `java_screens.rs`, empty otherwise), call
  `java_render_frame(0, partial)` before the frame's first Java request (screen, HUD, world, shell) and `java_render_frame(1, partial)`
  after its last. Apply the reply's sounds as `client_frame`'s are applied.
- Profile names `client_render_frame` so `HORIZON_PROFILE` reports its mean.

## Tests
- Test mod (`TM/ClientWindowProbe.java`): listeners on `WindowResizeEvent` (records the window's `getGuiScaledWidth()`),
  `RenderFrameEvent.Pre` and `.Post` (counts, and that each `Post` follows a `Pre`), and on the `MASTER` volume via
  `ClientTickEvent.Post` reading `options.getSoundSourceVolume(SoundSource.MASTER)`. `bridges` checks:
  `window_resize_fired` (at least once, at start, and its scaled width equals `ceil(width / scale)` of the window's size
  input), `render_frame_paired` (pre count = post count > 0 after 20 frames), `master_volume_follows` (after `drive` sets the
  master volume to 0.25 with `horizon-ctl`'s option command or the options file, the value read is 0.25).
- Java unit `U/WindowScaleCheck.java`: with the mixin's answer 3 and framebuffer 1920x1080, `resizeGui` leaves
  `guiScaledWidth == 640` and `guiScaledHeight == 360` (vanilla `setGuiScale` arithmetic) and calls a stub listener of
  `WindowResizeEvent` once.
- Rust: `Input::Volumes` bytes; the volume input is sent once and again only after a change.
- `java_mods.py build` (private dir), `unit`, `check`, `bridges`; `cargo test -p horizon-client --features java-mods --lib java_screens`;
  clippy with and without the feature.

## Done bar
Tests pass; with no frame listener no `client_render_frame` request is made (profile shows 0); `java_routing.py lint` green
after setting `net/minecraft/client/Minecraft#resizeGui()V` to `special: screens` in `deferred.tsv` (it now runs in Java's
real path) and the two `SoundEngine` keys to `special: sounds` (SPEC-7 may do the latter if it lands first).

## Live test (lead; showcase install, `HORIZON_JAVA_DEBUG=1`, `HORIZON_PROFILE=OUT/profile`)
Scenario `tools/scenarios/w8_window.json`: floor, creative, `{"wait": 100}`; run it twice with `--size 854x480` and
`--size 1280x720`. Checks: `log` `Horizon: RenderFrameEvent for \d+ mod listeners` (CreativeCore), `no_exceptions`. Report the
`client_render_frame` mean from `profile_report.py` (expected well under 0.1 ms).
User steps: resize the window while a mod screen is open (a Mekanism machine): it re-lays out as before; change the master
volume: nothing else changes audibly.
