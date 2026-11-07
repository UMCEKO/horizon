# SPEC-8 (W3): `/particle` with a mod's particle type

Design: `DESIGN.md` §2 step 7. Paths: DESIGN's short-hands. Depends on: SPEC-1 (facts), SPEC-6 (Java clients take the
packets; per-player `sendParticles` runs vanilla's body). Size: ~200 lines.
Comments in code: one line at most, only for a non-obvious why (RULES).

## Goal

`/particle mekanism:jetpack_flame ~ ~1 ~ 0.2 0.2 0.2 0.01 20` works as on NeoForge: vanilla's `ParticleArgument`
decodes the options with the type's own codec, and `ParticleCommand.sendParticles` sends each viewer the packet
through `ServerLevel.sendParticles(player, ...)`. Native keeps parsing the command, choosing the viewers and writing the
feedback; Java does the part only it can (the mod's codec, the mod's type on the wire). It is vanilla parity, and the
deterministic trigger SPEC-9's scenario and NeoForge comparison use.

## Rust

### `G/commands/world_cmds.rs` `cmd_particle`
Today `ParticleType::from_name(name)` fails for a mod type (`particle.notFound`, l. 693). Before it:
```rust
if self.java_hook_particle_owned(name) {
    return self.java_command_particle(ctx, &spec);
}
```
### New method, `G/commands/world_cmds.rs`
`fn java_command_particle(&mut self, ctx: &Context, spec: &str) -> R<i32>`:
1. `pos`, `delta`, `speed`, `count`, `force`, the viewers and the 32/512-block filter exactly as `cmd_particle` computes
   them (move that block into a helper `particle_viewers(&self, ctx, pos, force) -> R<Vec<Target>>` used by both, no
   behaviour change); empty → `commands.particle.failed`, as today.
2. Each reached target's UUID (`Target::Player` → the acting player's, `Target::Guest(conn)` → that guest's; add
   `fn target_uuid(&self, t: Target) -> Option<u128>` beside `target_position` if it doesn't exist).
3. `self.java_hook_server_particles(...)` (below) with the dimension, `spec`, `pos`, `delta`, `speed`, `count`,
   `force` and the UUIDs.
4. Reply `Ok(n)` with `n >= 0`: feedback `commands.particle.success` with the name, as the native path, return `n`.
   `n == 0` → `commands.particle.failed`. `Err(message)` → `particle.invalidOptions` with the message.

### `G/java_hooks.rs`
```rust
/// `/particle` of a Java mod's type: vanilla's codec and sends in Java; the players reached, or the codec's error.
pub(super) fn java_hook_server_particles(&mut self, dim: &str, spec: &str, pos: [f64; 3], delta: [f64; 3], speed: f32, count: i32, force: bool, viewers: &[u128]) -> Result<i32, String>
```
Other arm: `Err("no Java".into())` (unreachable: `java_hook_particle_owned` is false there). java-mods arm: request
`server_particles` with `text dim, text spec, 3 double pos, 3 float delta, float speed, int count, bool force, int n, n × (long, long) uuid`;
reply `int n` (≥ 0) or `int −1` followed by `text message`.

### Parse time
`horizon-command` `ArgType::Particle` (`args.rs` l. 1568) accepts an id found in `reg.ids("minecraft:particle_type")`.
Add a Rust test in `G/java_particles.rs` that, in a java-mods session's registries (build them as the existing
java-mods command tests do, or from the content cache's registry list), `minecraft:particle_type` contains the mods'
types. If it doesn't, extend that registry list with `java_particle_facts()`' names where the command registries are
assembled (find with `grep -rn "fn ids" crates/horizon-command/src`). `particle_options` (l. 975) checks fields only for
vanilla ids, so a mod's options pass parsing and are checked by the mod's codec at execution (step 4), where vanilla
would report the same error at parse time: the message is the same, the moment differs (noted in `docs/java-mods.md`
"Not yet").

## Java

### `B/GameBridge.java`
`Crossing.game("server_particles", GameBridge::serverParticles);` and:
```java
/** `ParticleCommand.sendParticles` for players Horizon chose: the options by vanilla's `ParticleArgument`, each sent through the per-player `sendParticles`. */
private static void serverParticles(Crossing.In in, Crossing.Out out) throws Exception
```
1. `level = level(text(in))`; `spec = text(in)`; read the rest in the wire order.
2. `ParticleOptions options;` from `ParticleArgument.readParticle(new StringReader(spec), level.registryAccess())`; a
   `CommandSyntaxException` → `out.writeInt(-1); text(out, e.getMessage())`.
3. For each UUID: `ServerPlayer p = level.getServer().getPlayerList().getPlayer(uuid)`; skip null; `if (level.sendParticles(p, options, force, false, x, y, z, count, dx, dy, dz, speed)) n++`.
   That overload is vanilla's body (not bridged): the distance check is vanilla's, on the view's position.
4. `out.writeInt(n)`.
Follow the existing `game` ops for `level(...)`, text reading and `Crossing.Out` (`GameBridge.levelTick` is a model).

## Tests

- **Rust unit, `G/commands/world_cmds.rs`:** `particle_viewers` with the same inputs as before the refactor returns the
  same targets (one existing `/particle` test, or a new one with a guest at 31 and one at 33 blocks: only the first).
- **Rust unit, `G/java_hooks.rs` or `G/java_particles.rs`:** the `server_particles` request bytes for one call
  (`overworld`, `x:spark`, (1, 2, 3), (0, 0, 0), 0.5, 4, false, one UUID `1, 2`) equal bytes written by hand from the
  layout above; a reply `-1` + `bad` maps to `Err("bad")`.
- **`tools/java_mods.py` `bridges`' `drive`** (it sends commands through Horizon's own dispatcher, `send('command ...')`):
  after the slot steps, `send('command particle horizon_java_test:spark ~ ~1 ~ 0 0 0 0 3')`; the reply must contain the
  success text (`commands.particle.success`'s English, `Displaying particle`), else append `particle_mod_type` to
  `wrong`. Then `send('command particle horizon_java_test:nope ~ ~1 ~')` must contain `Unknown particle`, else
  `particle_unknown`. (Java's own Brigadier is not Horizon's, so the testmod can't run these from Java.)
- `cargo test -p horizon-client --features java-mods --lib java_`, `cargo test -p horizon-client --lib particle`,
  clippy both feature sets, `java_mods.py build`, `bridges`.

## Done bar (the implementer checks)

- Tests pass. Testmod world: `/particle horizon_java_test:spark ~ ~1 ~ 0 0 0 0 1` prints the success line and SPEC-2's
  debug line shows one spark `live`; `/particle horizon_java_test:nope` still prints `particle.notFound`; a vanilla
  `/particle minecraft:flame ~ ~1 ~` behaves as before (native path).

## Live test (lead)

Play pack, creative, `HORIZON_JAVA_DEBUG=1`:
1. `/particle mekanism:jetpack_flame ~ ~1 ~ 0.2 0.2 0.2 0.01 20`: success line; with SPEC-4, 20 jetpack flames.
2. `/particle ae2:vibrant_fx ~ ~1 ~ 0.5 0.5 0.5 0 40`: success; AE2's vibrant particles.
3. `/particle ae2:crafting{item:{id:"minecraft:diamond"}} ~ ~1 ~ 0 0 0 0 1`: success; AE2's crafting particle with the
   diamond's sprite (its options codec ran in Java).
4. `/particle mekanism:laser ~ ~1 ~`: the laser's options need fields; the error message equals NeoForge's for the same
   command (compare with SPEC-9's real client run).
5. Dedicated server with one client: 1–3 from the client's chat behave the same.
