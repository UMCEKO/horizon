# SPEC-10 (W7): the window's sky, light and environment from its level's dimension type

Read first: `DESIGN.md` §1.2 G8, §2.5, D8, Q1; `crates/horizon-protocol/src/registry.rs` (`DimensionType`, `Registries::dimension`, `data`); `crates/horizon-client/src/game/remote.rs` (~2257: `r.dimension = r.registries.dimension(spawn.dimension_type)`); `crates/horizon-client-render/src/environment.rs` (`Environment::load_dimension`, `sample`, `sky_darken`); every render-side use of `Dim` for the look: `game.rs:16888` (`skybox: self.dim == Dim::Overworld && ...`), `game/render_tick.rs:133`, the End sky choice, `eye_brightness`'s Nether ambient (`render_tick.rs` ~372), the mesher's `nether_shading`, cloud height, `has_fixed_time` uses (`level_api.rs:52` is the server's, leave it); vanilla `DimensionType` (§0 of DESIGN), `SkyRenderer.extractRenderState`, `LightmapRenderStateExtractor`, `CardinalLighting`.

Depends on: W10 SPEC-2 for mod dimension types to exist on the server (vanilla needs nothing). Ask W10 to add `timeline` and `world_clock` to its export list; this spec reads them from the synced registries, so it lands without them. Comments in code: one line max, only a non-obvious why.

## Goal

Everything the window draws that depends on the dimension comes from the dimension type the server sent for the window's level (synced registry data, as a vanilla client reads it), not from `Dim`. Output for vanilla's three dimensions is identical; a data pack's or a mod's dimension type (AE2's `spatial_storage`, once its level exists: Q1) looks as on NeoForge.

## Protocol (`registry.rs`)

`DimensionType` gains, read in `Registries::dimension` from the entry's NBT (vanilla's `NETWORK_CODEC` field names, see `DimensionType.createDirectCodec`):
- `skybox: Skybox` (`none`/`overworld`/`end`, default `overworld`);
- `cardinal_light: CardinalLight` (vanilla's `CardinalLighting.Type` names; default its `DEFAULT`);
- `has_fixed_time: bool` (default false);
- `attributes: Option<horizon_world::nbt::Tag>` (the network form of the attribute map);
- `timelines: Vec<String>` (the holder set resolved to ids: a tag through the synced tags of `minecraft:timeline`, or the listed ids).
`DimensionType::vanilla(name)` fills them as vanilla's three JSON files say (read `.local/horizon-client/26.3/data/minecraft/dimension_type/*.json`; constants only, cited in a one-line comment each).

## Environment (`environment.rs`)

- `Environment::from_registry(dimension: &DimensionType, timelines: &[(String, Tag)], period: i64) -> Result<Self>`: the same model `load_dimension` builds from JSON, from the NBT (convert the NBT to `serde_json::Value` with the existing NBT→JSON helper if there is one, else write `fn nbt_to_json`, and feed `load_dimension`'s parser so there is one parser). Timelines: every timeline of the type, each entry's tracks, in the type's order (vanilla's `EnvironmentAttributeSystem` layers them: read `addDynamicLayers`/the timeline layer order and keep it).
- `load_dimension` stays for callers without a server (title screen panorama, probes).

## Game (render side only)

- The window builds its `Environment` from `r.dimension` and `r.registries`' `minecraft:timeline` entries at each level join/respawn (where `r.dimension` is set), not from `Dim`.
- Replace the look's `Dim` tests by the type's data:
  - sky: `skybox == Overworld` (and the settings flag) for the Overworld sky, `End` for the End sky, `None` clears to fog colour (`gfx.rs:869`);
  - `eye_brightness`'s ambient: `dimension.ambient_light` in place of `if Dim::Nether { 0.1 }`;
  - the mesher's Nether face shading: `cardinal_light` (vanilla `CardinalLighting.Type.NETHER` vs default);
  - `has_fixed_time`: wherever the window's sky angle or day time assumes the Overworld clock, use the type's `default_clock`/`has_fixed_time` (search `Dim::` in `render_tick.rs`, `gfx` callers, `clouds`, `fog`, `weather`).
- Gameplay uses of `Dim` (spawning, portals, saves, server) are untouched: they are mod-dimension-level work (Q1).

## Tests

- `registry.rs`: parse the synced entries of vanilla's three types from a fixture (copy them as the base root's registry report gives them); fields equal `DimensionType::vanilla(name)`.
- `environment.rs`: for each vanilla type, `from_registry(...)` and `load_dimension(root, ...)` give equal `sample(t, None, None)` at t = 0, 6000, 12000, 13000, 18000, 23999 (the file loader is the independent path; skip when `.local` is absent, as the module's existing test does).
- `game` render test (or a unit on the extracted helper): a type with `skybox: end`, `ambient_light: 0.5` → End sky chosen, eye brightness uses 0.5 (expected from vanilla `Lightmap.getBrightness(ambient, level)`: compute in the test comment).

## Done bar

- `cargo test -p horizon-protocol registry`, `-p horizon-client-render environment`, the game test; clippy both ways; `pr_ready.py --quick`; `python3 tools/parity.py --changed-files paths.txt --list` and the first two listed sky/fog/light harnesses through `verify/parity.sh`.

## Live test (user)

1. Overworld, Nether, End on a vanilla world: screenshots equal to `main`'s at the same time of day and place (sky, fog, light).
2. A world with a data pack overriding `minecraft:overworld`'s dimension type with `"skybox": "end", "ambient_light": 0.5` (vanilla allows it): the Overworld shows the End sky and brighter darkness, as vanilla 26.3 does with the same pack.
3. With Q1 done: AE2's spatial storage shows no sky, fixed time, full ambient light.
