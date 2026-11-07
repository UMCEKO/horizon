# SPEC-1 (W8): jukebox songs, music and ambience from every namespace and from the world's biome table

Read first: `CLAUDE.md`, `/home/umceko/hz-poc/agents/RULES.md`, DESIGN §1.2 G1/G2, `I/items.rs` (`load_songs`, `load_instruments`,
`Song`), `L/music.rs` (`MusicTables`, `Layer`, `BackgroundMusic`), `L/ambience.rs` (`AmbientTables`), `G/music.rs`
(`tick_music`, `situational_music`, `biome_at`, `tick_ambience`), `G/jukebox.rs` (`start_song`, `tick_jukebox`), `R/biome.rs`
(`Biomes::with_packs`, `pack_file`), `horizon-world/src/json_folder.rs` (`read`). Vanilla: `RegistryDataLoader`,
`FileToIdConverter.json`, `JukeboxSong.lengthInTicks`, `EnvironmentAttributeSystem` (biome layer over dimension layer).

Depends on: none. Rust only, no Java. Mod biomes appear in the world only once W10 (SPEC-1..3) exports them through
`Biomes::with_packs`; this spec reads them from there the moment they do.

Code comments: one line at most, only for a non-obvious why.

## Goal

Vanilla's registry loader reads `data/<every namespace>/<registry dir>/**/*.json`, ids `namespace:path` (nested folders
kept in the path). Native reads three client tables from `data/minecraft/` only. Make them read as vanilla does, and make
music and ambience follow the world's biome table (data packs today, mod biomes after W10).

Accept:
- a Biomes O' Plenty `music_disc_wanderer` in a jukebox plays `biomesoplenty:music_disc.wanderer`, shows "Now Playing", and on a
  server the jukebox stops after 178 s + 1 s as vanilla (`JukeboxSong.hasFinished`: `ticks >= lengthInTicks + 20`);
- a world data pack that gives `minecraft:plains` another `background_music` plays it in plains;
- with W10: in `biomesoplenty:coniferous_forest` the music pool is `biomesoplenty:music.overworld.nostalgic`; in BOP nether
  biomes their `ambient_sounds` loop plays.

## Rust

### R1. Every namespace (`I/items.rs`, `L/music.rs`, `L/ambience.rs`)
- Add one helper in `I/items.rs` (exported for the other two): `pub fn registry_files(root: &Path, dir: &str) -> Vec<(String, Value)>`:
  for each namespace folder of `root/data` (sorted), `horizon_world::json_folder::read(&root.join("data").join(ns).join(dir))`
  (a missing folder is empty), each file id `format!("{ns}:{}", file.name)` (`name` keeps subfolders, as `FileToIdConverter`),
  parsed JSON; unreadable or invalid files are skipped with a `log::warn!` naming the file (vanilla logs and skips a broken entry).
- `load_songs(root)`: build from `registry_files(root, "jukebox_song")`, same parsing as today. `load_instruments` the same
  way (`"instrument"`), since it has the same defect and the same three lines.
- `MusicTables::load(root)` and `AmbientTables::load(root)`: `dimensions` from `registry_files(root, "dimension_type")`,
  `biomes` from `registry_files(root, "worldgen/biome")`, parsed as today (`Layer::parse`, `AmbientSounds::parse`).

### R2. The world's biome table (`R/biome.rs`, `L/music.rs`, `L/ambience.rs`, `G/music.rs`)
- `Biomes::pack_files(&self) -> impl Iterator<Item = (&str, &Value)>` over `self.packs` (the files `with_packs` was given).
- `MusicTables::with_pack_biomes(&self, biomes: &Biomes) -> Self`: a clone whose `biomes` entry is replaced by
  `Layer::parse(json)` for every pack file (a pack file replaces the whole biome, attributes included, as `RegistryDataLoader`
  does; a pack biome without the attribute therefore has no music layer). `AmbientTables::with_pack_biomes` the same with
  `AmbientSounds::parse` (no attribute: the entry is removed).
- `Game` gets `world_audio: Option<WorldAudio>` (`struct WorldAudio { biomes: std::sync::Weak<Biomes>, music: MusicTables, ambient: AmbientTables }`
  in `G/music.rs`). `fn audio_tables(&mut self)` at the top of `tick_music` and `tick_ambience`: when `self.world_biomes` is
  `Some(b)` with `b.has_packs()` and the cached `Weak` does not point at the same `Arc` (`Weak::ptr_eq`), rebuild from
  `self.res.music()` / `self.res.ambient()`; when `world_biomes` is `None` or has no packs, clear it. (Pointer identity:
  every site that sets `world_biomes` makes a new `Arc`: `game.rs` `packed_biomes`, `remote.rs` `client_biomes`,
  `integrated.rs`, `native_biome_colors.rs`.)
- `situational_music` and `tick_ambience` read `world_audio`'s tables when present, else `self.res.music()` / `ambient()`.
  Add `fn music_tables(&self) -> &MusicTables` and `fn ambient_tables(&self) -> &AmbientTables` for that choice.

### R3. Nothing else changes
`start_song`, `tick_jukebox`, `jukebox_signals` keep reading `items().songs`; the remote path (`jukebox_song_name` by the
server's registry id) already yields `biomesoplenty:wanderer`, now found. No Java change.

## Tests (independent expected values)
- `I/items.rs` tests, with a temp root (`tempfile`, as the module's other tests):
  - `data/testns/jukebox_song/sub/x.json` = `{"sound_event":"testns:disc.x","length_in_seconds":178.0,"comparator_output":3,"description":{"text":"X"}}`
    and `data/minecraft/jukebox_song/cat.json` → both `minecraft:cat` and `testns:sub/x` present; `testns:sub/x` has
    `length_ticks == 3560` (vanilla `Mth.ceil(178 * 20)`), `comparator == 3`, `sound == "testns:disc.x"`.
  - A non-JSON file in `data/testns/jukebox_song` is skipped, the others load.
- `L/music.rs` tests:
  - `data/minecraft/dimension_type/overworld.json` with vanilla's own `background_music` (`creative` → `minecraft:music.creative`,
    `default` → `minecraft:music.game`, copy the two entries from `.local/horizon-client/26.3/data/minecraft/dimension_type/overworld.json`),
    `data/modns/worldgen/biome/grove.json` with `background_music.default.sound = "modns:music.grove"`:
    `background("minecraft:overworld", "modns:grove")` is the grove's; `background("minecraft:overworld", "minecraft:plains")`
    is the dimension's.
  - `with_pack_biomes`: a `Biomes` built with `with_packs` from a `minecraft:plains` file whose music is `modns:music.p` → plains
    plays `modns:music.p`; a pack file for `minecraft:desert` with no attribute → desert falls back to the dimension's.
- `L/ambience.rs`: a mod biome with `ambient_sounds.loop = "minecraft:ambient.crimson_forest.loop"` → `get(dim, "modns:b").loop`
  is that event.
- Keep the harnesses green: `python3 tools/parity.py situational_music audio_attributes` through
  `/home/umceko/hz-poc/verify/parity.sh` (their fixtures call `MusicTables::load`/`AmbientTables::load` on vanilla's root:
  same answers).
- `cargo test -p horizon-items songs`, `cargo test -p horizon-client --lib music:: ambience::`, `cargo test -p horizon-client-render biome`,
  clippy `-D warnings` on the three crates, fmt.

## Done bar (implementer)
All tests above pass; `cargo clippy -p horizon-client --features java-mods` and without the feature are clean; the two
harnesses pass; `tools/java_routing/deferred.tsv` unchanged (no Java keys involved).

## Live test (lead; showcase install `/home/umceko/hz-poc/agents/w12/install`, through `hz_scenario.py` with the gate)
Scenario `tools/scenarios/w8_songs.json`: `/forceload add -8 -8 8 8`, `{"wait": 40}`, `/fill -8 99 -8 8 99 8 minecraft:stone`,
`/fill -8 100 -8 8 110 8 minecraft:air`, `{"tp": [0.5, 100, 0.5]}`, check `near`, `/gamemode survival`,
`/setblock 0 100 2 minecraft:jukebox`, `/clear @s`, `/give @s biomesoplenty:music_disc_wanderer`, `{"slot": 0}`,
`{"lookat": [0.5, 100.5, 2.5]}`, `{"use": 2}`, `{"wait": 40}`, `{"screenshot": "song"}`, checks `block [0,100,2]` state
`jukebox[has_record=true]` and `no_exceptions`. Expected: the overlay "Now Playing: …Wanderer" in the screenshot.
On `--dedicated` with one client: the same overlay; then `/tick sprint 3600` and `/data get block 0 100 2`: the song is no
longer playing (`ticks_since_song_started` absent or reset, as vanilla's `JukeboxSongPlayer` saves it).
User: listen to the disc; with W10 landed, walk into a coniferous forest with music frequency "Constant" (Options → Music):
BOP's "Nostalgic" track plays; in a BOP nether biome the biome loop is audible.
