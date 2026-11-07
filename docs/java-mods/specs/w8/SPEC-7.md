# SPEC-7 (W8): mods' handlers on the Minecraft loop methods Horizon never runs

Read first: `CLAUDE.md`, RULES.md, DESIGN §1.2 G10, D6, `B/Routing.java` (`shapes`, `Applied`, `mergedName`), `B/ClientLoader.java`
(where it builds `Minecraft` with `GameBridge.construct` and runs the constructor's steps), `B/GameBridge.java` `safely`,
`G/music.rs` `tick_sound_engine` (the pause), `R/audio.rs` `pause`, `R/sound_engine.rs` `pause_all_except`,
`crates/horizon-java-host/src/routes.rs` (`get`, `Routes::id`, `Route`), `G/java_routing.rs` (`gui_route!`),
`tools/java_routing.py` (`REASON`), `tools/java_routing/deferred.tsv`, `tests/test_java_routing.py`.
Vanilla: `Minecraft.<init>` (ends after starting the reload), `Main.main` (`minecraft.run()` right after), `Minecraft.runTick`
line ~1215 (`pauseAllExcept(SoundSource.MUSIC, SoundSource.UI)` when the game becomes paused).
Mixins in the showcase (from `../w12/install/analysis/mixins/mixins.json`): Konkrete `MixinMinecraft.afterConstruct_Konkrete`
(`@Inject <init> RETURN`), balm `MinecraftMixin.run` (`@Inject run HEAD`), Puzzles Lib `MinecraftMixin.runTick`
(`@ModifyArg runTick INVOKE SoundManager.pauseAllExcept([SoundSource)V`).

Depends on: W12 SPEC-4 (lint, `deferred.tsv`; landed).

Code comments: one line at most.

## Goal

Horizon's Java `Minecraft` is built without its constructor's body and never runs `run` or `runTick`. Mods' handlers on those
methods must still run at vanilla's moment (D6):
1. `<init>` RETURN/TAIL injections: once, when the Java client's construction steps end (before the boot reload finishes, as
   the real constructor returns while the reload runs).
2. `run` HEAD injections: right after (vanilla's `Main` calls `run()` next).
3. `runTick`'s `@ModifyArg` on `SoundManager.pauseAllExcept`: when native's game becomes paused, native asks Java which sources
   keep playing; Java applies the mods' handlers to vanilla's `[MUSIC, UI]` and native pauses every other source.
Other kinds on these three methods (a `@Redirect`, `@Overwrite`, `@ModifyVariable`, an inject at an `INVOKE`) are reported at
boot as not routed: `Horizon: <mod> <kind> on Minecraft#<method> at <at>: not routed (Horizon runs no <method>)`.

Also the deferral bookkeeping for W8 keys that run in Java's real code already (DESIGN §1.3).

## Java (`B/LoopEdges.java`, new)

- Extend `Routing.Applied` with `List<String> atTargets` (each `@At`'s `target()`, empty string when none), read where `at`'s
  `value` is read. No other user changes.
- `LoopEdges.start(Minecraft minecraft)`, called by `ClientLoader` right after the constructor steps:
  - handlers = `Routing.shapes(Minecraft.class)` whose `mixin` is not Horizon's (`dev.horizonlab.`);
  - order: `@MixinMerged.priority()` ascending, then the order Mixin merged them (the counter in the merged name,
    `…$zz<letter><digits>$…`, ascending); two handlers whose order can't be told → a warning naming both;
  - `<init>` with `at` `RETURN`/`TAIL`, kind `inject`: invoke the merged method on `minecraft` with
    `(null GameConfig, new CallbackInfo("<init>", false))` (Horizon has no `GameConfig`; say so in the log line);
  - then `run` with `at` `HEAD`, kind `inject`: `(new CallbackInfo("run", true))`; a handler that cancels is logged loudly
    (vanilla would not run the game) and otherwise ignored;
  - each through `GameBridge.safely("Minecraft#<method> handler <mixin>", …)`; one log line per handler run;
  - every other handler on `<init>`, `run`, `runTick` that is not the `pauseAllExcept` `@ModifyArg` is reported as above.
- Op `client_pause_sources` (`LoopEdges.pauseSources()`): `SoundSource[] kept = {MUSIC, UI}`; for each `modify_arg` handler
  on `runTick` whose `atTargets` contains `Lnet/minecraft/client/sounds/SoundManager;pauseAllExcept([Lnet/minecraft/sounds/SoundSource;)V`,
  in order: `kept = (SoundSource[]) handler.invoke(minecraft, (Object) kept)`. Reply `int n` x `int ordinal`. When the route is
  on but no such merged handler exists (a config plugin filtered the mixin), log it once and reply vanilla's list: that is what
  the real client does without the mixin.

## Rust

- `G/music.rs` `tick_sound_engine`: when `self.pause` turns true, if the routes have
  `net/minecraft/client/Minecraft#runTick(Z)V` on (`horizon_java_host::routes::get()` → `id(key)` and the route is on), request
  `client_pause_sources` once for this transition (`java-mods` cfg; not built without the feature) and call
  `self.audio.pause_except(&sources)` (new: `R/audio.rs`, the body of `pause` with the list; `pause` calls it with
  `[MUSIC, UI]`); else `self.audio.pause()` as today. Mark the site `gui_route!("net/minecraft/client/Minecraft#runTick(Z)V")`.
- Map ordinals to the engine's source indices by name (`SoundSource.getName()` = `SOUND_SOURCES` names), not by number.

## Deferral bookkeeping (`tools/java_routing.py`, `tools/java_routing/deferred.tsv`, `tests/test_java_routing.py`)
- `REASON` gains `special: (loop|reload)`. `loop`: invoked by `LoopEdges` at vanilla's moment; `reload`: runs inside Java's
  real client resource reload (`ClientLoader`).
- Rows: `net/minecraft/client/Minecraft#<init>(Lnet/minecraft/client/main/GameConfig;)V` and `…#run()V` → `special: loop`;
  `…#runTick(Z)V` → removed (gated by the marker); `net/minecraft/client/resources/language/LanguageManager#onResourceManagerReload(Lnet/minecraft/server/packs/resources/ResourceManager;)V`
  → `special: reload`; `net/minecraft/client/sounds/SoundEngine#reload()V` and `#refreshCategoryVolume(Lnet/minecraft/sounds/SoundSource;)V`
  → `special: sounds` (unless SPEC-6 already did). Copy each key exactly as `java_routing.py report` prints it.
- `tests/test_java_routing.py`: the two new reasons parse; an unknown `special:` still fails.

## Tests
- Test mod mixins (`TM/mixin/MinecraftLoopMixin.java`, client config): `@Inject(method = "<init>", at = @At("RETURN"))` sets
  `RouteState.initRan`; `@Inject(method = "run", at = @At("HEAD"))` sets `runRan` and records that `initRan` was already set;
  `@ModifyArg(method = "runTick", at = @At(value = "INVOKE", target = "…pauseAllExcept…"))` returning the array without `UI`.
  `bridges` checks: `loop_init_then_run` (both ran, in that order), `pause_sources_from_handler` (the op's reply is `[MUSIC]`:
  vanilla's `[MUSIC, UI]` minus what the test handler removed, an expected value written by hand).
- Java unit: `Applied.atTargets` holds the `@ModifyArg`'s target for the test mixin (in `RoutingCheck`).
- Rust: `pause_except(&[MUSIC])` pauses UI sounds and keeps music (the existing `R/sound_engine.rs` pause tests, one more case).
- `python3 -m unittest tests.test_java_routing`; `python3 tools/java_routing.py lint` and `gates --check` on both reference packs;
  `java_mods.py build` (private dir), `unit`, `check`, `bridges`; `cargo test -p horizon-client-render sound_engine`,
  `cargo test -p horizon-client --features java-mods --lib music`; clippy with and without the feature.

## Done bar
Tests pass; `java_routing.py report` on the showcase shows no `client: phase C` row for the keys this spec names; boot log of
the showcase lists Konkrete's and balm's handlers as run and no "not routed" line for them.

## Live test (lead; showcase install, `HORIZON_JAVA_DEBUG=1`)
Boot the showcase once (`hz_scenario.py` with a scenario that only waits 100 ticks): log lines for Konkrete's `<init>` and balm's
`run` handlers, and either `pause sources: [UI]`-style debug from the first pause or the "filtered" line (Puzzles Lib's plugin).
User steps (showcase): with music playing (Options → Music frequency "Constant"), press Esc: on Forge with Puzzles Lib the music
pauses with the game (if the boot log said the handler is merged), and resumes when you return; without Puzzles Lib (install-rs)
it keeps playing, as vanilla.
