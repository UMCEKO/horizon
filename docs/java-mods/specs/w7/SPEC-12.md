# SPEC-12 (W7): environment-attribute routes: the window's values from Java's real probe

Read first: `DESIGN.md` §1.2 G10, §2.5, D9; `w12/DESIGN.md` §2 (routes, `routes.bin`, `Routing.route`, edge/body, gates, `deferred.tsv`), `w5/SPEC-7.md` §1 (client keys in `routes.bin`); `tools/java_routing/deferred.tsv` (the four `EnvironmentAttribute*` rows); vanilla `EnvironmentAttributeProbe` (`tick`, `getValue`, `ValueProbe.get` and `partialTickLerp`), `Camera.tick` (`attributeProbe.tick(level, position)`), `EnvironmentAttributes` (the visual attributes), each attribute type's `partialTickLerp` (`AttributeTypes`); native `crates/horizon-client-render/src/environment.rs` (`Sky`, `sample`, `apply_weather`), where the window builds its `Sky` each frame (`game.rs`/`render_tick.rs`).

Depends on: W12 SPEC-2 (route mechanism and client keys), SPEC-10 (the window's environment from its dimension type). Comments in code: one line max, only a non-obvious why.

## Goal

When a loaded mod patches the attribute system (lithostitched: `EnvironmentAttributeSystem#addDynamicLayers`, `ValueSampler#computeValueNotPositional`, `#computeValuePositional`, `EnvironmentAttributeProbe#tick`), the window's sky, fog, light and cloud values are the ones Java's real, patched probe computes for the camera, sampled per tick and lerped per frame exactly as vanilla's probe lerps them. With no such mod, nothing changes and nothing runs. The server's gameplay attributes stay with W12/W13.

## Detection

- Rust: `client_env_routed()` = any of the four keys is routed in this install (`routes()` from W12, client side). Java: `Routing.route(key)` for the same keys.
- These keys leave `deferred.tsv` (client) and gain a gate marker at the native site: `gui_route!`-style marker `env_route!("<key>")` (W12 SPEC-3 gate list) on the function that builds the window's `Sky`.

## Java (`Environments.java`, beside SPEC-11's, or a new `ClientAttributes.java`)

- Each Java client tick, when routed and a level exists, right after `minecraft.tick()`: `var probe = minecraft.gameRenderer.mainCamera().attributeProbe(); probe.tick(level, cam);` (`cam` the window camera as `WorldFrame` holds it: vanilla ticks the probe in `Camera.tick` at the camera's position).
- For each attribute in `VISUAL` (exactly the attributes native's `Sky` reads; list them from `environment.rs` `sample`: `minecraft:visual/sky_color`, `fog_color`, `sky_light_factor`, `sky_light_color`, `block_light_tint`, `ambient_light_color`, `sun_angle`, `moon_angle`, `star_angle`, `star_brightness`, `sunrise_sunset_color`, `cloud_color`, `fog_start_distance`, `fog_end_distance`, `cloud_fog_end_distance`, `cloud_height`, `water_fog_color`, `minecraft:gameplay/sky_light_level`): write `getValue(attr, 0f)` (last) and `getValue(attr, 1f)` (new). At 0 and 1 every vanilla lerp returns its end values, so the pair is exact.
- Status tail (after SPEC-11's): `boolean routed`, then `int n` and per attribute `text id`, its type tag (float, ARGB int, angle), last, new.

## Rust

- `environment.rs`: `pub struct ProbedValues { values: Vec<(String, Probed)> }` with `Probed::{Float(f32, f32), Color(u32, u32), Angle(f32, f32)}`; `fn lerp(&self, partial) -> ...` per type as vanilla's `partialTickLerp` (read `AttributeTypes`: plain lerp for floats, the colour type's lerp for ARGB (check whether it lerps in sRGB or linear and port that), the angle type's lerp with wrap-around); `pub fn sky_from_probe(&self, partial: f32) -> Sky` builds the same `Sky` fields `sample` builds.
- The window: when `client_env_routed()` and the status brought values, the frame's `Sky` is `sky_from_probe(partial)` (then `apply_weather` as today: check vanilla applies weather inside the attribute system in 26.3; if it does, skip native's `apply_weather` in this path, as the probe's values already include it). Otherwise unchanged.
- The values persist until the next status; on level change they clear and native sampling serves until Java's first tick reply.

## Tests

- `environment.rs`: `lerp` of a colour pair at 0, 0.5, 1 and an angle pair across 360° against hand-computed vanilla values (from the lerp formulas in vanilla's source, in the test comment); `sky_from_probe` maps each id to its `Sky` field.
- Java unit `ClientAttributesCheck`: on a level-less probe (vanilla returns defaults: `attribute.defaultValue()`), the written table has every `VISUAL` id with last == new == default.
- `java_routing.py gates --check`, `lint`: the four keys are gated, not deferred.

## Done bar

- `cargo test -p horizon-client-render environment`, `-p horizon-client --features java-mods --lib java_`; `java_mods.py unit`, `check`; `java_routing.py report` on the showcase shows these keys gated; clippy both ways.
- Showcase boot (`w12/install`, lithostitched present): the boot log says the keys are routed; a 1-minute session's profile shows one probe tick per client tick, no per-frame Java call.

## Live test (user)

1. Showcase pack, Overworld at sunrise and at noon: sky and fog colours equal those of the same seed, time and place on Forge with the same pack (screenshots side by side).
2. Without lithostitched (play pack): identical to before this spec (screenshots equal), and the boot log names no environment route.
