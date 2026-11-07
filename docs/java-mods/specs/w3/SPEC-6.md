# SPEC-6 (W3): particles a Java server sends that native can't carry run vanilla's own `sendParticles`

Design: `DESIGN.md` §2 step 5, D5. Paths: DESIGN's short-hands. Depends on: SPEC-1 (`ParticleFacts.owned`), SPEC-2
(the Java client takes them). Size: ~200 lines.
Comments in code: one line at most, only for a non-obvious why (RULES).

## Goal

`ServerLevel.sendParticles(options, overrideLimiter, alwaysShow, x, y, z, count, xDist, yDist, zDist, xSpeed, ySpeed, zSpeed, randomization)`
is bridged to Rust (`G/java_bridges/effects.rs` `level_send_particles`), which can carry only vanilla types with
options kinds 0–6 (`B/Effects.java` `particle`). For anything else (a mod's type, or a vanilla options class Horizon
doesn't flatten: item, vibration, trail, shriek, sculk charge), vanilla's own body runs in Java instead: it builds
`ClientboundLevelParticlesPacket` and sends it to each player of the level's view within 32 blocks (512 with the
limiter override) through `player.connection.send`, which reaches that player's Java client (in the window's process,
or over `horizon:java` for a LAN guest or a dedicated server's player). There SPEC-2 takes it: a mod's type into
Java's engine, a vanilla type to native under the packet scope. The return value is vanilla's (players reached).

The overloads that delegate to this one follow it; the per-player overload `sendParticles(ServerPlayer, ...)` was never
bridged and already runs in Java.

## `P/BridgeProcessor.java`: guards that see the arguments

Today an `@Class.method` guard is called as `(Object receiver)Z`. Add the form `@Class.method+`: the guard also receives
the bridged method's own parameters, in order, with their own types: `(Ljava/lang/Object;` + the method's parameter
descriptors + `)Z`.
- `nativeCall` (l. 157): in the `entry.guard.startsWith("@")` branch, strip a trailing `+`, load `this`, and when it was
  there, load each parameter with `param.getOpcode(Opcodes.ILOAD)` and its local slot (`1 + ` the sizes before it), then
  `INVOKESTATIC` with the longer descriptor.
- `Entry.holder()` and Rust's `Bridge::holder` (`crates/horizon-java-host/src/bridge.rs` l. 59) split at the last `.`,
  which a trailing `+` doesn't change; strip it in both where the guard's class or method name is used.
- `tools/java_mods.py` l. 659 already skips `@` guards in its check.
- Document the form in `docs/java-mods-map.md` where `@` guards are described (one sentence), and in the
  `java_bridges!` macro's doc comment.

## Java

### `B/Effects.java`
- Factor the kind switch of `particle(options)` into `static int kind(ParticleOptions options)` (same numbering: 0 plain,
  1 block, 2 dust, 3 dust transition, 4 colour, 5 spell, 6 power, 9 none of these); `particle` uses it.
- The guard:
  ```java
  /** Whether Horizon's port sends these particles: a vanilla type whose options it carries. */
  public static boolean nativeParticles(Object level, ParticleOptions options, boolean overrideLimiter, boolean alwaysShow,
          double x, double y, double z, int count, double xDist, double yDist, double zDist, double xSpeed, double ySpeed, double zSpeed,
          ClientboundLevelParticlesPacket.RandomizationType randomization) {
      return Flat.ON && !ParticleFacts.owned(options.getType()) && kind(options) != 9;
  }
  ```
  `Flat.ON` is what the bridge tested before (it had no `if`).

## Rust: `G/java_bridges/effects.rs`

The `sendParticles` row gains the guard:
```
"net/minecraft/server/level/ServerLevel".sendParticles
"(...)I" if "@dev/horizonlab/javamods/boot/Effects.nativeParticles+" =>
```
Its body is unchanged. Its `log::warn!("... a payload Horizon can't send")` now means the guard and `particle_of`
disagree: keep it.

## Testmod: `T/ChecksEffects.java`

- Replace `sendParticles of an item sends none` by `sendParticles of an item reaches the players near it`, whose
  expected value the check computes by vanilla's rule, re-stated: the count of `server.players()` with
  `p.level() == server && p.blockPosition().closerToCenterThan(particleAt, 32.0)`.
  `server.sendParticles(new ItemParticleOption(ParticleTypes.ITEM, Items.DIAMOND), ...)` must equal it.
- New `sendParticles of a mod's type reaches the players near it`: the same with
  `(SimpleParticleType) TestMod.SPARK.get()`.
- New `sendParticles of a vanilla type still goes through Horizon`: the existing `sendParticles` check stays as it is
  (vanilla types keep the port).

## Tests

- **Java unit, new `mods/horizon/src/java-mods-host/unit/dev/horizonlab/javamods/plugin/ArgsGuardCheck.java`**, built
  like `RouteStubCheck` in the same folder (a class made with ASM in the check, run through the processor's
  package-private steps, defined by a child `ClassLoader`): an instance method `(Ljava/lang/String;ID)I` whose Java body
  returns 1, bridged to a static stand-in for the native that returns 2, guarded by `@<check's nested class>.guard+`.
  The guard `static boolean guard(Object self, String s, int i, double d)` records its arguments and returns the
  check's flag. Flag false: the call returns 1 and the guard saw `("a", 3, 4.5)`; flag true: it returns 2. A `long` or
  `double` before an `int` in a second method checks the slot arithmetic.
- `cargo test -p horizon-java-host bridge` (the holder with `+`).
- `java_mods.py build`, `unit`, `check` (0 new problems), `bridges` (all CHECKs, the three above included).

## Done bar (the implementer checks)

- Checks pass. `bridges` logs no `a payload Horizon can't send`.
- With the testmod in a world and `HORIZON_JAVA_DEBUG=1`: the `sendParticles` checks run near the player; SPEC-2's
  debug line shows the spark `live` and the diamond item particles `forwarded` in the window's Java client.

## Live test (lead)

1. Play pack, single player: AE2 Matter Cannon (`ae2:matter_cannon`) loaded with ammo, fire at a block. The server's
   AE2 code sends its effect (a packet or `sendParticles`): with SPEC-4, the trail shows. Without SPEC-4: SPEC-2's debug
   line shows `live > 0` after the shot.
2. Dedicated server (`java_mods.py launch --install install-rs --dedicated --nogui`) and one client with the same
   install: the same shot from the client shows the same particles in the client window.
3. Two clients (LAN or dedicated): client B sees the particles of A's shot when within 32 blocks, not when 40 blocks
   away (vanilla's distance rule, now applied by vanilla's code).
