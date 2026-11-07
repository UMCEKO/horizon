# SPEC-1 (W3): mods' particle types as boot facts

Design: `DESIGN.md` §2 step 1, D1. Paths: DESIGN's short-hands. Depends on: nothing. Size: ~200 lines.
Comments in code: one line at most, only for a non-obvious why (RULES).

## Goal

Both sides know, from the boot and never by namespace, which particle types are mods' and the Java registry's
id ↔ name table:
- Java: `ParticleFacts.owned(type)`, a bit per registry id, read by SPEC-2 (client) and SPEC-6 (server).
- Rust: `java_particle_owned(name)` and `java_particle_name(id)`, fetched once through a new op, read by SPEC-2's log,
  SPEC-7 and SPEC-8.
- The boot log names them, so the lead checks the set against the jars.

## Java

### `B/BootFacts.java`
Add `BuiltInRegistries.PARTICLE_TYPE` to `REGISTRIES` (line 30). Its snapshot is taken with the others right after
vanilla's bootstrap, so "owned" means added (or replaced) by a mod. Nothing else changes there.

### New `B/ParticleFacts.java`
```java
/** Which particle types are mods', by the boot's registry snapshot, and the registry's names for Rust. */
public final class ParticleFacts {
    static boolean owned(ParticleType<?> type);          // bit of OWNED by BuiltInRegistries.PARTICLE_TYPE.getId(type)
    static byte[] export();                              // the `particle_facts` reply
    static byte[] write(List<String> names, BitSet owned) // pure, for the unit check
    static void report();                                // the boot lines below; client and server
    static List<String> replaced(Map<String, ?> providers, Predicate<String> owned) // pure, for the unit check; sorted
}
```
- `OWNED` is a `BitSet` built on first use from `BootFacts.owns(BuiltInRegistries.PARTICLE_TYPE, key)` over every
  registered id; rebuilt never (registries are frozen after the boot).
- `write` format (big-endian, `DataOutputStream`): `int n`, then per id `0..n-1`: `int len` + UTF-8 name, `bool owned`.
  This is the `text` form `ClientResources.text` writes and `In::text` reads.
- `report()` prints once, after the mods have loaded:
  - `Horizon: <k> particle types are mods', ticked in Java: <sorted names, comma-separated>`
  - client only (when `Minecraft.getInstance() != null` and its particle engine exists): for each id the vanilla
    snapshot had (`!owned`) whose provider in `ParticleResources.getProviders()` has a class `Shells.isMods` accepts,
    one line `Horizon: <provider class> replaced the provider of <id>; native keeps drawing it (W3 does not route it)`.
    Read the providers through a new accessor `B/mixin/ParticleEngineAccess.java`:
    ```java
    @Mixin(ParticleEngine.class)
    public interface ParticleEngineAccess {
        @Accessor("resourceManager") ParticleResources horizon$resources();
    }
    ```
    (SPEC-2 adds the other two accessors to the same interface.) Register it in `B/../res/horizon_java.client.mixins.json`.
- Call `report()` from `ClientResources.take()` at its end (client), and from `GameBridge`'s server start
  (`lifecycle(input, true)`, first start only) when no client exists (`HORIZON_JAVA_DIST=server`).

### `B/GameBridge.java` `register()`
`Crossing.game("particle_facts", input -> ParticleFacts.export());` beside `entity_facts`.

## Rust

### New `G/java_particles.rs` (declared in `crates/horizon-client/src/game.rs` beside `mod java_world_renders;`, same `cfg(feature = "java-mods")`)
```rust
/// The Java registry's particle types: names by id, and the ones mods registered.
pub(super) struct ParticleFacts { names: Vec<String>, owned: std::collections::HashSet<String> }

fn parse(reply: &[u8]) -> Option<ParticleFacts>;
/// Fetched once from the booted JVM; None without one or when the reply doesn't read.
pub(super) fn java_particle_facts() -> Option<&'static ParticleFacts>;   // OnceLock, host.request("particle_facts", &[])
pub(super) fn java_particle_owned(name: &str) -> bool;
pub(super) fn java_particle_name(id: i32) -> Option<&'static str>;
```
- The fetch happens at first use, which is always inside a world (the boot has finished before a world opens), so it is
  one request per run. A malformed reply logs `Horizon: the Java particle facts were malformed` once and answers
  "not owned" for every name.

### `G/java_hooks.rs`
A pair, as the file's other hooks:
```rust
/// Whether a particle type is a Java mod's, whose code runs in Java.
pub(super) fn java_hook_particle_owned(&self, name: &str) -> bool
```
java-mods arm: `java_particles::java_particle_owned(name)`; other arm: `false`.

### `G/java_screens.rs` `java_client_particles`
Replace the log `Horizon: Java particle {id} has no native particle yet (W3)` by
`Horizon: Java particle {name} ({id}) reached native, which can't draw it` with `name` from `java_particle_name(id)`
(`?` when unknown). After SPEC-2 this line means a bug, so keep it once per id.

## Testmod

`T/TestMod.java`: a `DeferredRegister<ParticleType<?>>` for `horizon_java_test`, registering
`SPARK = PARTICLE_TYPES.register("spark", () -> new SimpleParticleType(false))`, registered on the mod bus on both dists.
SPEC-2 gives it a provider; this spec needs only the registration.

## Tests

- **Java unit, new `U/ParticleFactsCheck.java`** (run by `java_mods.py unit`):
  - `write(["minecraft:flame", "x:spark"], {1})` equals bytes built by hand in the check: `int 2`, `int 15` +
    `minecraft:flame`, `false`, `int 7` + `x:spark`, `true`.
  - `replaced`: the map `{"minecraft:flame" → new fakemod.FakeProvider(), "minecraft:smoke" → Integer.valueOf(1)}`
    (a `java.lang` class stands for a non-mod provider; only the value's class is tested) with nothing owned gives
    exactly `["minecraft:flame"]`; with `minecraft:flame` owned it gives `[]`.
  - New class `mods/horizon/src/java-mods-host/unit/fakemod/FakeProvider.java` (package `fakemod`): an empty class; its
    package is outside every package `Shells.isMods` excludes, so it stands for a mod's class. SPEC-2, 7 reuse the package.
  - `java_mods.py unit` finds `unit/**/*Check.java` by name; nothing to register. Plain JVMs, no boot: the checks must not
    touch `BuiltInRegistries` or `Minecraft`.
- **Rust unit, `G/java_particles.rs` `#[cfg(test)]`:** `parse` of a hand-built reply with 3 types (`minecraft:flame`
  false, `mekanism:laser` true, `ae2:vibrant_fx` true) answers `owned("mekanism:laser")`, not `owned("minecraft:flame")`,
  `name(2) == Some("ae2:vibrant_fx")`, `name(3) == None`; a reply cut after the second name gives `None`.
- `cargo test -p horizon-client --features java-mods --lib java_particles`, clippy `-D warnings` on `horizon-client`
  with and without `--features java-mods`, `java_mods.py build`, `java_mods.py unit`.

## Done bar (the implementer checks)

- Unit checks and Rust tests pass; clippy clean in both feature sets.
- `java_mods.py bridges` (testmod run) logs `particle types are mods'` with `horizon_java_test:spark` in the list and
  no `no ownership kept for` error.

## Live test (lead)

1. Boot the play pack (`install-rs`), any world, `HORIZON_JAVA_DEBUG` not needed.
2. `python3 tools/logsum.py LOG --grep "particle types are mods'|replaced the provider"`.
3. Expect exactly these 13 names, independent of the code (from the jars' `assets/*/particles/*.json` and BG2's
   `ModParticles`): `ae2:crafting, ae2:energy_fx, ae2:lightning_arc_fx, ae2:lightning_fx, ae2:matter_cannon_fx,
   ae2:vibrant_fx, buildinggadgets2:fluidflowparticle, buildinggadgets2:itemflowparticle, mekanism:jetpack_flame,
   mekanism:jetpack_smoke, mekanism:laser, mekanism:radiation, mekanism:scuba_bubble`, and no `replaced the provider` line.
4. Dedicated (`java_mods.py launch --install install-rs --dedicated --nogui`): the same line in the server log.
