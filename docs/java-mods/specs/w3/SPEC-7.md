# SPEC-7 (W3): mods' block destroy and hit effects run their client extension in Java

Design: `DESIGN.md` §2 step 6. Paths: DESIGN's short-hands. Depends on: SPEC-1 (Rust facts module), SPEC-2 (scope,
mod particles live in Java); SPEC-4 to see quads, SPEC-2 alone forwards vanilla-type ones. Size: ~300 lines.
Comments in code: one line at most, only for a non-obvious why (RULES).

## Goal

NeoForge's patched `ClientLevel.addDestroyBlockEffect(pos, state)` first asks `IClientBlockExtensions.of(state).addDestroyEffects(state, level, pos, particleEngine)`,
and `addBreakingBlockEffects(pos, direction, playSound)` asks `addHitEffects(state, level, pos, direction, particleEngine)`
(javap -c callee list of the patched class); `true` replaces vanilla's crumbs. In the bar, AE2's
`CableBusBlockClientExtensions` overrides both (its own `CableBusBreakingParticle`s from the parts' textures) and
Mekanism's `RenderPropertiesProvider$2` overrides `addDestroyEffects` (bounding blocks show their main block's effect).
Native makes these effects itself today (`G/game.rs` `destroy_particles`, `crack_particle`).

After this spec: for a block state whose client extension's class is a mod's and declares the method, native skips its
own effect and the window's Java client runs vanilla's `ClientLevel` method at its next tick, so the mod's code decides,
and vanilla's fallback (when it returns false) is made in Java as well, as one owner. Every other state stays native.

## Boot export

### New `B/BlockEffects.java`
```java
/** Which block states' client extensions make their own destroy or hit particles, and running them for the window. */
final class BlockEffects {
    static final int DESTROY = 1, HIT = 2;
    static int bits(Class<?> extension)             // pure: DESTROY if it declares addDestroyEffects, HIT if addHitEffects, by `declares`
    static boolean declares(Class<?> type, String name, Class<?>... params) // walks type and its superclasses, true when a class Shells.isMods accepts declares it
    static void export(DataOutputStream out)       // per state, below
    static void run(DataInputStream in)            // one Input kind 30, below
}
```
- `export`: for every block in `BuiltInRegistries.BLOCK` (mods may give vanilla blocks extensions too), `bits(IClientBlockExtensions.of(block).getClass())`
  (cached per extension class); for each block with bits ≠ 0, each of its states:
  `int n`, then `n ×` (`int Block.BLOCK_STATE_REGISTRY.getId(state)`, `byte bits`).
  Parameter types for `declares`: `addDestroyEffects(BlockState, Level, BlockPos, ParticleEngine)`,
  `addHitEffects(BlockState, Level, BlockPos, Direction, ParticleEngine)`.
- `ClientResources.take()`: call `BlockEffects.export(out)` after the atlases loop, so the export ends with it. Log
  `Horizon: <k> block states make their own destroy or hit particles in Java` when k > 0.

### Rust: `G/java_models.rs`
`parse_resources`: after the atlases, read `effects: Vec<(i32, u8)>` (`input.int()`, `input.byte()`), field
`effects` on `ClientResources`; add `pub(in crate::game) fn client_block_effects() -> &'static [(i32, u8)]` (empty
without a client).

### Rust: `G/java_particles.rs` (SPEC-1's module)
```rust
/// By Horizon's state id: 1 when the state's mod makes its destroy particles in Java, 2 its hit particles.
pub(super) fn java_block_effects(state: BlockStateId) -> u8
```
Built once (OnceLock) from `client_block_effects()` through `horizon_java_host::bridge::STATES.get()?.to_horizon`.

## The window asks, Java runs

### Rust: `G/java_hooks.rs`
```rust
/// Hands a block's destroy (0) or hit (1) particles to the Java client when its mod makes them; true when it took them.
pub(super) fn java_hook_block_effect(&mut self, kind: u8, pos: [i32; 3], state: BlockStateId, face: [i32; 3]) -> bool
```
Other arm: `false`. java-mods arm:
- `false` unless the window's Java client has joined (`CLIENT.joined` in `java_screens.rs`) and
  `java_block_effects(state) & (1 << kind) != 0`;
- else push `(kind, pos, to_java[state], direction(face))` onto a new `Client.block_effects: Vec<(u8, [i32; 3], i32, u8)>`
  and return `true`. `direction`: vanilla's 3D data value: down `[0,-1,0]` 0, up 1, north `[0,0,-1]` 2, south 3,
  west `[-1,0,0]` 4, east 5; destroy sends 0.

### Rust: `G/java_screens.rs`
- `Input::BlockEffect(u8, [i32; 3], i32, u8)`, kind byte **30**; fields written as `byte kind, int x, int y, int z, int javaState, byte face`.
- `java_client_tick_send`: after the `Input::Particles` entry, write every queued `block_effects` entry in order as
  `Input::BlockEffect`, then clear the queue.

### Rust: `G/game.rs`
- `destroy_particles`: after the `hide_block_particles` return, `if self.java_hook_block_effect(0, pos, state, [0; 3]) { return; }`.
- `crack_particle`: after the same return, `if self.java_hook_block_effect(1, pos, state, face) { return; }`.
The hide setting keeps applying first, as for native effects.

### Java: `B/ClientBridge.java` `events(in)`
Kind 30: `BlockEffects.run(in)`:
```java
var level = Minecraft.getInstance().level;
int kind = in.readByte(); var pos = new BlockPos(in.readInt(), in.readInt(), in.readInt()); int state = in.readInt(); int face = in.readByte();
if (level == null) return;
ClientParticles.enter();
try {
    if (kind == 0) level.addDestroyBlockEffect(pos, Block.stateById(state));
    else level.addBreakingBlockEffects(pos, Direction.from3DDataValue(face), false);
} finally {
    ClientParticles.exit();
}
```
`playSound` false: the hit sound stays native's, as it is today. The scope makes vanilla's fallback crumbs (if the
mod's method returns false) Java's own, so they reach native (vanilla `TerrainParticle`s through `add`, kept and
recorded by SPEC-3) rather than being dropped.

## Testmod

`T/ClientSpark.java`: `RegisterClientExtensionsEvent` registers for `TestMod`'s battery block an
`IClientBlockExtensions` (a nested class of the testmod) whose `addDestroyEffects` adds four `TestMod.SPARK` particles
at the block's centre (`level.addParticle`) and returns `true`; it does not override `addHitEffects`.

## Tests

- **Java unit, new `U/BlockEffectsCheck.java`:** `bits(fakemod.Effects.class) == DESTROY` where
  `mods/.../unit/fakemod/Effects.java` declares `public boolean addDestroyEffects(BlockState, Level, BlockPos, ParticleEngine)`
  (any body; the check only reflects on it); `bits` of a `fakemod` class declaring both is 3; of a class in
  `dev.horizonlab.javamods.boot` declaring both is 0 (not a mod's); of `Object.class` 0.
- **Rust unit, `G/java_particles.rs`:** `direction` for the six faces gives 0..5 as listed; `java_block_effects` over a
  hand-built table maps Java ids through a test `to_horizon`.
- **Rust unit, `G/java_models.rs`:** `parse_resources` of a hand-built export with two effect entries reads them; one
  with `int 0` effects reads with `effects` empty; one cut inside an entry gives `None`. Existing tests that build an
  export append `int 0`.
- `cargo test -p horizon-client --features java-mods --lib java_`, clippy both feature sets, `java_mods.py build`,
  `unit`, `bridges`.

## Done bar (the implementer checks)

- Tests pass. Testmod world: the export log line says at least the battery's states. Breaking a battery in survival
  makes no native crumbs, and SPEC-2's debug line shows 4 sparks `live` (with SPEC-4, four flame sprites drifting +x).
- Breaking a vanilla stone block still makes native crumbs (no `BlockEffect` sent: check with `HORIZON_JAVA_DEBUG`
  that the Java side logs nothing for it).

## Live test (user: visual)

Play pack, survival:
1. Place an AE2 cable with a part (an ME Terminal on a glass cable). Mine it slowly: the crumbs show the cable's and
   part's textures, as on Forge, not one flat texture. Break it: the burst shows them too.
2. Build a Mekanism multiblock with bounding blocks (a Mekanism Digital Miner or a Seismic Vibrator, which occupy more
   than one block) and break it by its upper part: the particles are the machine's, not a missing texture.
3. Mine and break stone, dirt and a Mekanism ore: native crumbs as before, nothing doubled.
