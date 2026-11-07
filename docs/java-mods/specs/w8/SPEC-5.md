# SPEC-5 (W8): the music tick runs in Java while a mod changes it

Read first: `CLAUDE.md`, RULES.md, DESIGN §1.2 G7, D1, D2 and Q2, `B/mixin/MusicManagerMixin.java`, `B/ClientSounds.java`
(`play`, `start`, `finished`: how a Java sound reaches native and `isActive` answers), `B/SoundOwners.java` (MusicManager is
not a native row; keep it so), `B/NativeSounds.java` and `B/WorldFrame.java` `listened` (listener detection), `B/Routing.java`
(`route`, `on`), `B/ClientBridge.java` (`connect`, `events`, `status`, the tick job), `G/music.rs` (`tick_music`,
`situational_music`, `now_playing_song`, `draw_music_toast`), `L/music.rs` (`MusicManager`, `Situation`), `G/java_screens.rs`
(`Input`, `java_client_tick_send`, the status parser, `CLIENT.sounds`), `G/game.rs` around the remote branch of `fn tick`
(`self.java_client_tick_send()` after `tick_remote`). Vanilla: `MusicManager.tick`, `startPlaying`, `stopPlaying`,
`setMinutesBetweenSongs`, `Minecraft.getSituationalMusic`, `getMusicVolume`, `ToastManager.showNowPlayingToast` /
`hideNowPlayingToast`. NeoForge (javap): `ClientHooks.selectMusic(Music, SoundInstance)`, `SelectMusicEvent`. Real stack
(DESIGN §1.2 G7): `SelectMusicEvent` ← `ClientHooks.selectMusic` ← `MusicManager.tick` line 45, every client tick.

Depends on: W12 SPEC-2 (`routes.bin`, `Routing.route`; landed). Independent of the other W8 specs.

Code comments: one line at most.

## Goal

While a loaded mod patches `MusicManager#tick` (lithostitched's `@WrapOperation` on `getMusicVolume`) or a mod listens to
`SelectMusicEvent`, the Java client's real `MusicManager.tick` runs the music for the session (D2): NeoForge posts
`SelectMusicEvent` at its real site, mods' patches run, and the track it starts plays through `ClientSounds` as every Java
sound does. Native answers the two questions the tick asks the game (`getSituationalMusic`, `getMusicVolume`) and stops
running its own music manager. With neither, nothing changes: native's manager runs and Java's tick stays cancelled.

## Detection (`B/ClientMusic.java`, new)

`ClientMusic.decide()` once, when the Java client connects (`ClientBridge.connect`, before the first tick):
`owned = Routing.route("net/minecraft/client/sounds/MusicManager#tick()V") >= 0 || modListens(SelectMusicEvent.class)`.
`modListens`: the bus's listener list for the class (as `NativeSounds.listened`) counting only listeners whose owner class is
a mod's (not `net.neoforged.`, `net.minecraft.`, `com.mojang.`, `dev.horizonlab.`); if W5 already has such a helper, use it.
Log: `Horizon: music runs in Java (<route|SelectMusicEvent listener>)` or `Horizon: music stays native`.

## Wire

- `Input` kind **40** `Music`, queued every client tick while owned, just before the tick request: `bool present`; if present
  `text sound` (the event id), `int minDelay`, `int maxDelay`, `bool replace`; then `float volume` (`getMusicVolume`'s
  answer), `bool loading` (native shows its level loading screen), `byte frequency` (`MusicFrequency`: 0 default, 1 frequent,
  2 constant).
- Tick status, appended block `music`: `bool owned`; if owned: `int handle` (`ClientSounds`' handle of
  `MusicManager.currentMusic`, -1 none), `int n` x `bool` (now-playing toast requests, `true` show, `false` hide, in order).

## Java

- `ClientMusic`: holds the last `Music` (built with `Music(Holder<SoundEvent>, min, max, replace)`; the holder from
  `BuiltInRegistries.SOUND_EVENT.get(id)` when registered, else `Holder.direct(SoundEvent.createVariableRangeEvent(id))`,
  as vanilla resolves an unregistered event in data), `volume`, `loading`, `frequency`; `toasts` queue.
- `ClientBridge.events` case `MUSIC` → `ClientMusic.read(in)`; when the frequency changed, call
  `minecraft.getMusicManager().setMinutesBetweenSongs(freq)` (what the option's listener does).
- `mixin/MusicManagerMixin.java`: the HEAD cancel only when `!ClientMusic.owned`. Add
  `@ModifyExpressionValue(method = "tick", at = @At(value = "CONSTANT", args = "classValue=net/minecraft/client/gui/screens/LevelLoadingScreen"))`
  returning `original || ClientMusic.loading` (native's loading screen is not a Java screen).
- `mixin/MinecraftMusicMixin.java` (new, client): HEAD-cancellable on `Minecraft.getSituationalMusic` and `getMusicVolume`,
  answering `ClientMusic`'s values only when owned.
- `mixin/ToastNowPlayingMixin.java` (new): HEAD-cancellable on `ToastManager.showNowPlayingToast` and `hideNowPlayingToast`
  when owned: append to `ClientMusic.toasts` and cancel (native draws its own Now Playing toast).
- `status`: append the `music` block; the handle from `ClientSounds`' `PLAYING` map for the manager's `currentMusic`
  (an `@Accessor` on `MusicManager.currentMusic`).
- The track itself: `MusicManager.startPlaying` → `SoundManager.play` → `ClientSounds.play`; `SoundOwners` must not treat
  `MusicManager` as native (it has no row; add a test so it stays so).

## Rust

- `G/music.rs`:
  - `fn java_music_input(&mut self)`: when `CLIENT.music.owned`, compute `situational_music()` and apply `native_music`
    (native mods' choice stays part of "the game's" answer), and queue `Input::Music` with `loading` = native's loading
    condition from `tick_music` and the frequency option. Called from `game.rs`'s remote branch right before
    `self.java_client_tick_send()` (so the answer is this tick's, after the level tick, as vanilla's order).
  - `tick_music`: when owned, skip `self.music.set_frequency`, `set_off`, `self.music.tick(...)` and the manager's
    `toast_requests`; keep `music_layers.tick` and the toast-state option. Apply the status' toast requests to
    `self.now_playing` (`show(multiplier)` / `hide()`) in order.
  - `now_playing_song`: when owned, the instance is `CLIENT.sounds[handle]` instead of `self.music.current_instance()`.
- `G/java_screens.rs`: `Input::Music` kind 40 and its writer; the status parser reads the appended block into
  `CLIENT.music: MusicState { owned, handle, toasts }`.
- Mark the native music tick with `gui_route!("net/minecraft/client/sounds/MusicManager#tick()V")` in `tick_music` and remove
  that key from `tools/java_routing/deferred.tsv`.

## Tests
- Test mod (`TM/ClientMusicProbe.java`): a `SelectMusicEvent` listener that records `getOriginalMusic().sound()` and, while a
  test flag is set, calls `overrideMusic(new Music(SoundEvents.MUSIC_CREDITS, 0, 0, true))`. Checks in `bridges`:
  `select_music_fired` (fired on a client tick), `select_music_original_is_natives` (in a creative flat world in the
  overworld the original is `minecraft:music.creative`, from vanilla's own `dimension_type/overworld.json` `creative` entry,
  read in the check from the client's registry, not from Horizon), `music_override_played` (with the flag, a `PLAY` event for
  `minecraft:music.credits` from the MUSIC source appears in `ClientSounds`' output within 2 ticks: `replace` + delay 0).
- Java unit `U/ClientMusicCheck.java`: `ClientMusic.read` of a crafted input gives a `Music` with the given delays and replace
  flag; an unregistered id gives a direct holder with that location.
- Rust: the `Input::Music` bytes; status block parse; with `owned`, `tick_music` leaves `self.music` untouched (its delay and
  current instance unchanged after a tick).
- Java: `SoundOwnersCheck` gains `music_manager_not_native`.
- `java_mods.py build` (private dir), `unit`, `check`, `bridges`; `cargo test -p horizon-client --features java-mods --lib java_screens music`;
  clippy with and without the feature; `python3 tools/parity.py situational_music music` through `verify/parity.sh` (native path
  unchanged); `python3 tools/java_routing.py lint`.

## Done bar
Tests and harnesses pass; without the test mod's listener and without lithostitched, the log says `music stays native` and
the `music` harness is unchanged; with the test mod, `bridges` shows the three checks ok.

## Live test (lead)
1. install-rs (no listener, no route): log `Horizon: music stays native`.
2. Showcase (lithostitched patches the tick): log `Horizon: music runs in Java (route)`. Scenario `tools/scenarios/w8_music.json`:
   floor, creative, `{"wait": 200}`, `{"screenshot": "toast"}`; checks `no_exceptions`. Before the run, set the music
   frequency to Constant and the music toast to "pause menu and toast" in the copied home's `options.txt` (use the keys and
   value forms native's `options_file.rs` writes for `musicFrequency` and `musicToast`).
   Expected: the Now Playing toast with a creative track name.
User steps (showcase): play a few minutes in creative and survival; music starts after the usual delay, never two tracks at
once; the pause menu shows the song name; entering a world while a menu track plays fades it as before.
