# W8: the remaining client details (design)

Status: design, 2026-11-06. Base: `java-mods` at cc1b066b6. Binding: COVERAGE-SPEC principle 0 and 1 to 5, RULES.md.
Scratch: `scan/refs.py` (constant-pool scan of the installs' jars: which classes reference a name; no code read).

Paths used in the specs: `B` = `mods/horizon/src/java-mods-host/boot/dev/horizonlab/javamods/boot`, `RES` =
`mods/horizon/src/java-mods-host/boot/res`, `U` = `mods/horizon/src/java-mods-host/unit/dev/horizonlab/javamods/boot`,
`TM` = `mods/horizon/src/java-mods-host/testmod/dev/horizonlab/javamods/testmod`, `L` = `crates/horizon-client/src`,
`G` = `crates/horizon-client/src/game`, `R` = `crates/horizon-client-render/src`, `I` = `crates/horizon-items/src`.

## 0. Summary

W8 owns what the Java client and native disagree on in audio, text measuring, the F3 screen, the camera mods are told
about, and the window/frame/loop hooks of the Java `Minecraft` that Horizon never runs. Nine gaps are proven (section 1),
seven of them player-visible in the bar's packs, two only M1/M2-visible. Each becomes one spec (section 4).

- **Audio data (SPEC-1):** native reads jukebox songs, biome music and biome ambience only from `data/minecraft/`. Biomes
  O' Plenty's music disc is silent (and on a server its jukebox never ends), and its biomes' music and ambient loops are
  never chosen.
- **Fonts (SPEC-2, SPEC-8):** Java measures every font with the default font's advances, so mod text in `minecraft:uniform`
  (GuideME, AE2's guide) or `minecraft:alt` (Waystones) is laid out at the wrong width; Jade's patch of
  `Font$PreparedTextBuilder` (its overlay text shadow and glyph filter) has no effect because native draws whole runs.
- **Camera (SPEC-3):** the camera Java's world frames and renderer probes get is the eye with FOV 70, also in third person.
- **F3 (SPEC-4, SPEC-9):** mods' `DebugScreenEntry`s (AppleSkin, Nature's Compass, lithostitched) register in Java and never
  show on native's F3; balm's and GlitchCore's patches of the F3 overlay and debug keys are native-owned.
- **Music (SPEC-5):** Java's `MusicManager.tick` is cancelled, so `SelectMusicEvent` never fires and lithostitched's patch of
  the tick (a music fade) is native-owned.
- **Window, frame and loop hooks (SPEC-6, SPEC-7):** `WindowResizeEvent` and `RenderFrameEvent` never fire; mod patches of
  `Minecraft#<init>`, `run`, `runTick` (Puzzles Lib keeps music from playing on through a pause) and `resizeGui` never run.

Mod post effects and mod shaders are **not** a gap in the bar's packs: no jar ships `post_effect/`, none references
`PostChain`, `ShaderManager` or `setPostEffect` (section 1.3).

## 1. The gap, with evidence

### 1.1 Method
- `javap` signatures and constant pools of the installs' jars (`/home/umceko/hz-poc/play/install-rs/mods`, 11 jars;
  `/home/umceko/hz-poc/agents/w12/install/mods`, 43 jars), through `scan/refs.py`.
- The fresh `replaces` reports in `../finish/rep-{rs,showcase}/` (events, methods, routing) and the install's mixin audit
  `../w12/install/analysis/mixins/mixins.json`; `tools/java_routing/deferred.tsv`.
- Real NeoForge stacks from `../bootcmp/real.jfr` (`jfr print --events jdk.MethodTrace`; JFR `Event.<init>` with stacks,
  11-03). Its Horizon side (`hz.json`) is older than most of the host and is not used for counts.
- Reading Horizon's code at cc1b066b6 and vanilla's decompiled 26.3 sources.
- **No live boot.** Every gap below is a deterministic fact of the code (a hard-coded `data/minecraft` path, a cancelled
  method, an ignored argument). STATUS §0 says the shared jars in `.local/java-mods` are stale, so a boot today would
  measure the wrong host. The lead's live steps are in each spec.

### 1.2 Gaps

| # | Gap | Evidence | Who sees it | Spec |
|---|---|---|---|---|
| G1 | Jukebox songs only from `data/minecraft/jukebox_song` | `I/items.rs` `load_songs` reads `root.join("data/minecraft/jukebox_song")`; the content root carries `data/biomesoplenty/jukebox_song/wanderer.json` (`agents/mem/live1/.horizon-cache/mod-content/`); `G/jukebox.rs` `start_song` and `tick_jukebox` return early when `items().songs` lacks the song | BOP's `music_disc_wanderer` plays nothing, no "Now Playing"; on a server `tick_jukebox` never finishes the song, comparators read 0 | SPEC-1 |
| G2 | Biome and dimension music and ambience only from `data/minecraft/` | `L/music.rs` `MusicTables::load` and `L/ambience.rs` `AmbientTables::load` read `data/minecraft/{dimension_type,worldgen/biome}` only; the world's own biome table (`Biomes::pack_file`, which W10 fills with mod biomes, `w10/DESIGN.md` §2) is never read for audio. BOP: 68 biome files, 40 with `minecraft:audio/background_music` (e.g. `coniferous_forest` → `biomesoplenty:music.overworld.nostalgic`), 4 with `minecraft:audio/ambient_sounds` | Mod biomes play the dimension's default music and no biome loop | SPEC-1 |
| G3 | Java measures every font as the default font | `B/ClientLoader.java` `font()`: `glyphs(FontDescription)` ignores its argument; `G/java_screens.rs` `java_client_font` sends only `Font::advances()` of `minecraft:default`, once per process (`CLIENT.font`). Users: GuideME (`DefaultStyles.UNIFORM_FONT` = `minecraft:uniform`, `Style.withFont` in `RenderContext`), Waystones (`WarpPlateBlock.ALT_FONT` = `minecraft:alt`). Also: Force Unicode Font and the language change never reach Java | Mod screens and tooltips using those fonts wrap and align at the wrong widths (AE2's guide in install-rs) | SPEC-2 |
| G4 | Native-owned text drawing patched by Jade | Jade `PreparedTextBuilderMixin`: `@Inject accept(ILStyle;BakedGlyph;)Z` (filters glyphs, `JadeFont.isFilteredGlyph`) and `@WrapOperation getShadowColor → ARGB.scaleRGB` (theme alpha, light scheme), both only for `JadeFont`. Routing report: both `body`, `deferred (client: phase C)` | Jade's overlay text shadow follows its theme and fade on Forge, not on Horizon; M2 | SPEC-8 |
| G5 | The camera told to Java is the eye, FOV 70 | `G/java_world_renders.rs` `java_world_camera`: `eye_position()`, `self.yaw/pitch`, `70.0`; native's real camera is `self.view` (`R/camera.rs` `align_with_entity`, third person backed off, front view mirrored) and `fov_degrees` | BG2's preview, Mekanism's and AE2's camera-facing renders are placed for the eye in third person and for FOV 70 | SPEC-3 |
| G6 | Mods' F3 entries never show | Real: `RegisterDebugEntriesEvent` from `DebugScreenEntries.registerModdedDebugEntries` (JFR). Horizon fires it (`ClientLoader` builds `debugEntries`), but native F3 (`L/debug_entries.rs` `ENTRIES`, 46 vanilla rows; `G/debug_lines.rs`) knows only vanilla paths. Entries: AppleSkin `DebugInfoHandler$FoodStatsDebugEntry`, Nature's Compass `NaturesCompassOverlay`, lithostitched (structure attributes line), Puzzles Lib (dev) | AppleSkin's F3 food lines missing; the debug options screen lacks mod rows | SPEC-4 |
| G7 | `SelectMusicEvent` never fires; lithostitched's tick patch is native-owned | `B/mixin/MusicManagerMixin.java` cancels `MusicManager.tick` at HEAD. Real: `SelectMusicEvent` from `ClientHooks.selectMusic` ← `MusicManager.tick` line 45, every client tick (1141 in the comparison). lithostitched `MusicManagerMixin.applyVolumeMultiplier`: `@WrapOperation tick → Minecraft.getMusicVolume()` (structure attribute fade) | M1 (no listener in the packs); M2 for lithostitched (its structure data is absent from the packs, so no audible effect today) | SPEC-5 |
| G8 | Window and frame events | Real: `WindowResizeEvent` from `ClientHooks.fireWindowResize` ← `Minecraft.resizeGui` (←`<init>`, ←`framebufferSizeChanged`); `RenderFrameEvent.Pre/Post` per frame from `Minecraft.renderFrame`. Horizon: `ClientBridge.size` writes `Window` fields and never calls `resizeGui`; no frame event. Listeners: CreativeCore `RenderFrameEvent.Pre`; Konkrete patches `resizeGui` | M1; CreativeCore's own screens | SPEC-6 |
| G9 | Native volumes never reach Java's `Options` | `SoundEngine#refreshCategoryVolume` (Melody, edge) runs only from the volume option's listener, which Java never sees: native owns the options. Mods reading `Options.getSoundSourceVolume` read Java's stale defaults | M2; mods scaling their own sounds by a category volume | SPEC-6 |
| G10 | Minecraft loop methods Horizon never runs | `GameBridge.construct(Minecraft.class, …)` skips `Minecraft.<init>`'s body; `run` and `runTick` never run (`ClientBridge` calls `tick`). Patches: Konkrete `<init>` RETURN; balm `run` HEAD (balm's `CLIENT_STARTED`); Puzzles Lib `@ModifyArg runTick → SoundManager.pauseAllExcept` (drops `MUSIC` from the kept sources: music pauses with the game; `puzzleslib.common.mixins.json` lists it as a client mixin; a config plugin may still filter it, checked at runtime by SPEC-7) | Puzzles Lib (needed by Visual Workbench): music keeps playing on the pause screen on Horizon; M2 for the others | SPEC-7 |
| G11 | F3 overlay and debug-key patches | balm `DebugScreenOverlayMixin` (HEAD cancel/TAIL around `extractRenderState`, balm's `RENDER_GUI_DEBUG_PRE/POST`); GlitchCore `MixinKeyboardHandler` on `handleDebugKeys` (its `InputEvent.Key`). Horizon's `DebugKeysMixin` returns false at HEAD; native draws F3 | M2 only: no jar outside balm and GlitchCore references those events (`scan/refs.py`) | SPEC-9 |

### 1.3 Checked and not a gap (or someone else's)

| Item | Evidence | Verdict |
|---|---|---|
| Mod sounds (`sounds.json`, tickable sounds, `PlaySoundEvent`, `SoundEngineLoadEvent`) | carried `assets/<mod>/sounds.json`; `ClientSounds`, `NativeSounds`; `SoundEngineMixin.loadLibrary` posts `SoundEngineLoadEvent` (real: 1 per `SoundEngine.reload`) | works (W0 SPEC-1/1B/2) |
| Mod fonts (`assets/<mod>/font/`) | none in either install; native `R/font.rs` `font_ids` already reads every namespace's `font/**` | native takes them at boot; SPEC-2 makes Java measure them |
| Mod post effects and shaders | no `post_effect/` in any jar; no class references `PostChain`, `ShaderManager`, `setPostEffect`, `LevelTargetBundle`; Puzzles Lib's `RegisterEntitySpectatorShadersEvent` has no caller of its context in other jars; Mekanism `shaders/core/{mekasuit,sps}` are render pipelines | not W8: pipelines are W1 (SPS) and W2 (Meka-Suit) |
| `PlaySoundSourceEvent`, `PlayStreamingSourceEvent` | real fires from `SoundEngine.play` with an OpenAL `Channel`; no listener in either pack | out of scope (a listener would need an OpenAL channel; reported, section 5) |
| Melody (OpenAL clips), CreativeCore `SpecialSoundInstance.getStream` | `scan/refs.py`: no jar depends on melody or creativecore | out of scope: their own audio streams play only for dependents |
| Mekanism `texts/splashes.txt` | no class references it; vanilla `SplashManager` reads only `minecraft:texts/splashes.txt` | inert on Forge too |
| `LanguageManager#onResourceManagerReload` (Jade), `SoundEngine#reload` (Melody) | both run in Java's real client reload (`ClientLoader`) | executes in Java: a deferral reason, not a route (SPEC-7 §3) |
| `Hud#extractCameraOverlays` (GlitchCore), `FogRenderer#setupFog` (Jade) | routing report | W5 SPEC-10 and W4/W7 |
| Toasts, chat, client commands | events list | W6 |
| `ClientPlayerChangeGameTypeEvent` | `Menus` sends `UPDATE_GAME_MODE`, which posts it in the real `handlePlayerInfoUpdate` | works |
| Jade per-frame recording spikes | COVERAGE-SPEC | parked by the fidelity rule |

## 2. Approach

1. **Data first.** Native owns music, ambience, jukeboxes and fonts; what it lacks is data that is already in the content
   root or the world's biome table. Read it as vanilla's registry loader does (every namespace, nested paths), and from the
   world's table where W10 puts mod biomes (SPEC-1). Send native's per-font advances to Java as boot data, and again only
   when native's fonts change (SPEC-2).
2. **Java runs only what a mod is detected to change** (principle 0). Detection is static (`routes.bin` keys from W12) or
   the bus's listener lists at the moment the Java client starts (as `NativeSounds.decide`). Then:
   - the music tick runs in Java's real `MusicManager` while a mod patches it or listens to `SelectMusicEvent`, with native
     answering `getSituationalMusic` and `getMusicVolume` (SPEC-5);
   - text is recorded glyph by glyph from Java's real `PreparedTextBuilder` only while a mod patches it (SPEC-8);
   - mods' F3 entries run their own `display` in Java; native draws the lines (SPEC-4);
   - per-frame events fire per frame only while a mod listens (SPEC-6);
   - mods' handlers on loop methods Horizon never runs are invoked at the moment vanilla would reach them (SPEC-7).
3. **Vanilla's path, not a copy.** Resizes go through the real `Minecraft.resizeGui` (so NeoForge's event and every mod
   patch run); volumes through the real option setters; F3 lines through the real `DebugScreenEntry.display` with
   vanilla's own level and chunk getters.
4. **No polling.** Every crossing is either boot data, an input the window already has (size, options, a pause), the client
   tick, or a frame while a listener is detected.

## 3. Decisions

- **D1. Events posted at a native site fire only while a mod listens** (precedent: `NativeSounds` for `PlaySoundEvent`).
  `SelectMusicEvent` and `RenderFrameEvent` follow it. *Reason:* principle 0; an event nobody listens to is unobservable,
  and posting it per tick or frame costs a crossing. *Consequence:* the M1 diff must compare event classes **listened in the
  install**; propose that to the lead (open question Q1).
- **D2. Music: Java owns the whole tick when detected, native owns it otherwise.** Detection at the Java client's start:
  `routes.bin` has `net/minecraft/client/sounds/MusicManager#tick()V` on, or a mod listener on `SelectMusicEvent`. *Reason:*
  lithostitched's patch reads `MusicManager.currentGain`, Java's own field; evaluating the handler alone against native's
  manager would read stale state. One owner per session, so nothing plays twice (principle 2). Horizon's Music page filter
  and native mods' music override apply only to native's manager; native mods' music *layers* still play (Q2).
- **D3. Mod F3 lines run per client tick** (the HUD rule), not per frame. AppleSkin's values change per tick anyway.
- **D4. Glyph recording only while a mod patches `PreparedTextBuilder`.** Otherwise text stays a run of characters
  (cheaper, and native's own drawing is the reference).
- **D5. Font advances are boot data**, re-sent only when native's font set, Force Unicode, Japanese glyph variants or the
  language change. *Reason:* principle 3.
- **D6. Loop-method handlers (`<init>` RETURN, `run` HEAD, `runTick` `@ModifyArg`) are invoked from their merged methods**
  found by `Routing.shapes` (W12), at vanilla's moment. A kind that cannot be invoked alone (a `@Redirect` or `@Overwrite` of
  a loop method) is reported at boot, not routed.
- **D7. Library hooks with no consumer in the pack** (balm debug events, GlitchCore debug key event) are still routed
  (SPEC-9) because the bar's M2 says no native-owned target is left unrouted; SPEC-9 is last and may be dropped if the
  lead accepts "inert: no consumer" as a deferral reason (Q3).

### Open questions for the user
- **Q1.** M1 for events whose site is native (D1): compare only the classes a loaded mod listens to?
- **Q2.** While Java owns the music (D2), Horizon's Music page (off-track filter, an extra that is off by default) does not
  apply. Accept, or should native filter tracks Java asks it to play from the MUSIC source?
- **Q3.** SPEC-9 routes two library hooks nothing in the pack consumes. Route them (bar-literal M2), or add a deferral reason
  `inert: no consumer` that `java_routing.py lint` checks against the install's jars?

## 4. Specs

| Spec | Scope | Depends on | Accept (lead or user) |
|---|---|---|---|
| [SPEC-1](SPEC-1.md) | Jukebox songs, biome/dimension music and ambience from every namespace and from the world's biome table | none (BOP biomes generating needs W10 SPEC-1..3) | BOP disc plays with "Now Playing"; a data-pack biome's music plays; with W10, BOP biome music and loops |
| [SPEC-2](SPEC-2.md) | Java measures each `FontDescription` with native's advances for that font; re-sent on font changes | none | GuideME/AE2 guide text in `uniform` laid out without overlap; Waystones' `alt` names fit |
| [SPEC-3](SPEC-3.md) | The camera told to Java is native's real camera (third person, front mirror, real FOV) | none | BG2 preview stays on its blocks in F5 |
| [SPEC-4](SPEC-4.md) | Mods' F3 entries: boot export, debug options rows, per-tick lines in Identifier order | none | AppleSkin's food lines on F3 |
| [SPEC-5](SPEC-5.md) | Java-owned music tick when a mod patches it or listens to `SelectMusicEvent` | W12 SPEC-2 (`routes.bin`, landed) | test mod's `SelectMusicEvent` override plays; Now Playing toast |
| [SPEC-6](SPEC-6.md) | `resizeGui` path (`WindowResizeEvent`), `RenderFrameEvent` per frame while listened, volumes into Java's `Options` | none | M1: both events fire; Melody's edge runs on a volume change |
| [SPEC-7](SPEC-7.md) | Loop-method handlers: `<init>` RETURN, `run` HEAD, `runTick` `@ModifyArg` on the pause; deferral reasons for Java-run keys | W12 SPEC-4 (lint, landed) | With Puzzles Lib: music pauses on the pause screen as on Forge |
| [SPEC-8](SPEC-8.md) | Glyph recording (`GLYPHS`) while a mod patches `PreparedTextBuilder` | SPEC-2 | Jade's text shadow follows its theme (light scheme) |
| [SPEC-9](SPEC-9.md) | F3 overlay edges and debug-key edges through W5's GUI bridges | SPEC-4, W5 SPEC-7 §1 | M2: no W8 key left `client: phase C` |

**Order:** SPEC-1, 2, 3, 4 in any order (independent, player-visible); then 6, 7, 5; then 8 (after 2); SPEC-9 last.
Each spec moves its own keys out of `tools/java_routing/deferred.tsv` (a `gui_route!` marker at the native site, or a
`special:` reason), so `java_routing.py lint` stays green after every one.

### Wire allocations (W8)
- `Input` kinds (window → Java client, in the tick batch): **40** `Music` (SPEC-5), **41** `Volumes` (SPEC-6),
  **42** `DebugLines` (SPEC-4). 26–39 are left for W4/W6.
- Tick status: each spec **appends** its block after the last field present when it lands, Java and Rust in the same
  change: `music` (SPEC-5), `debugLines` (SPEC-4), `frameEvents` (SPEC-6); SPEC-9 appends `f3Skipped` to W5 SPEC-9's HUD block.
- Ops: `client_font` (format changed, SPEC-2), `client_debug_entries` (SPEC-4), `client_render_frame` (SPEC-6),
  `client_pause_sources` (SPEC-7), `client_debug_key` (SPEC-9).
- Recorder entry kind **5** `GLYPHS` (SPEC-8; 4 left free for W5 if it needs one).

### Rules repeated in every spec
Clean room (javap signatures and runtime traces only for NeoForge and mods); no per-mod code; no polling or fallbacks;
comments in code one line at most; no commits; private `HORIZON_JAVA_MODS_DIR` while a Java change is unproven; the test
budget of RULES.md (touched modules, clippy, fmt, `pr_ready.py --quick`, at most the named harnesses).

## 5. Out of scope (beyond the bar, or another workstream's)
- `PlaySoundSourceEvent` / `PlayStreamingSourceEvent` listeners (sound physics mods): they act on OpenAL channels. None in the
  packs. When one appears, report it at boot as unserved.
- Mods' own audio streams (`SoundInstance#getStream`, Melody's OpenAL clips): only for dependents absent from the packs.
- Mod post-effect chains and entity spectator shaders: none in the packs (section 1.3); would be a native post pass fed by
  boot data (the effect JSON and its shaders translated), designed when a pack needs it.
- Text with `FontDescription.AtlasSprite` / `PlayerSprite` inside Java-recorded text (object components): Java measures
  them (SPEC-2, 8.0 as vanilla), but the TEXT entry carries no sprite; nothing in the packs draws them.
- Mod dimensions' music (`ae2:spatial_storage`, Terralith's dimension types): native has no level for them until W10;
  SPEC-1 reads their tables so they work once W10 adds the levels.
- The Jade per-frame recording spikes (parked by the fidelity rule), screens (W5), toasts and chat (W6), fog and sky (W7),
  FOV and camera *events* (W4: SPEC-3 only sends native's camera, whatever W4 makes it).
