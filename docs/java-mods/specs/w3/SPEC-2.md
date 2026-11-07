# SPEC-2 (W3): mods' particles live in the Java client's engine; vanilla ones only from Java's own sources

Design: `DESIGN.md` §2 step 2, D2. Paths: DESIGN's short-hands. Depends on: SPEC-1. Size: ~300 lines.
Comments in code: one line at most, only for a non-obvious why (RULES).

## Goal

The Java client's real `ParticleEngine` creates, holds and ticks every particle of a mod's type (provider, constructor,
`tick`, sub-particles), and every particle a mod's code builds itself and `add`s. A particle of a vanilla type made in
the Java client reaches native (today's `ClientParticles.add` path) exactly when Java's code made it and native did not;
otherwise it is dropped because native draws its own copy. Nothing is drawn yet for mods' types (SPEC-3/4).

The rule, in `ClientParticles.sourced()`, true when any holds:
1. `Animate.forwarding()` (unchanged: Java-run `animateTick`, W0 SPEC-1B);
2. a scope is open: the Java client handles a particle packet (`ClientPacketListener.tryAddParticle`), or runs a routed
   block effect (SPEC-7);
3. a mod's class is on the current thread's stack (`StackWalker`, `Shells.isMods(frame.getDeclaringClass())`).

Why: native mirrors none of 1–3 (the Java client's packets all come from the Java server; mods' handlers, tickers and
particles are mod code), while vanilla code the Java client runs on its own (the vanilla block entities' tickers it keeps
in `ClientBridge.blockEntity`, the `LocalPlayer`'s vanilla tick) has a native copy. Case 3 is checked last and only for
vanilla types or vanilla particle classes, so it costs a stack walk for a few particles a tick.

## Java

### `B/ClientParticles.java`
Add:
```java
private static final StackWalker STACK = StackWalker.getInstance(StackWalker.Option.RETAIN_CLASS_REFERENCE);
private static int scopes;
/** Opens a scope whose particles are Java's own: pair with `exit` in a finally. */
public static void enter()
public static void exit()
/** Whether a particle made now is Java's own, with no native copy: see SPEC-2's rule. */
public static boolean sourced()
/** Whether `ParticleEngine.add` keeps a particle: a mod's class, or made while `sourced`. */
public static boolean keeps(Class<?> particle)
static boolean modOnStack()   // STACK.walk(s -> s.anyMatch(f -> Shells.isMods(f.getDeclaringClass())))
/** The particle `createParticle` is adding for a mod's type, which `add` keeps. */
public static Particle admitted;
```
Counters for the debug line: `forwarded`, `dropped` (ints, reset when printed).
`scopes` is touched only on the render thread (the Java client's thread), so a plain `int` is right.

### `B/mixin/ParticleEngineMixin.java` (replace both injections)
```java
@Inject(method = "createParticle", at = @At("HEAD"), cancellable = true)
private void horizon$particle(ParticleOptions options, double x, double y, double z, double xa, double ya, double za, CallbackInfoReturnable<Particle> result) {
    if (ParticleFacts.owned(options.getType())) return;
    if (ClientParticles.sourced()) ClientParticles.add(options, x, y, z, xa, ya, za);
    else ClientParticles.dropped++;
    result.setReturnValue(null);
}

@Inject(method = "createParticle", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/particle/ParticleEngine;add(Lnet/minecraft/client/particle/Particle;)V"),
        locals = LocalCapture.CAPTURE_FAILHARD)
private void horizon$admit(ParticleOptions options, double x, double y, double z, double xa, double ya, double za, CallbackInfoReturnable<Particle> result, Particle particle) {
    ClientParticles.admitted = particle;
}

@Inject(method = "add", at = @At("HEAD"), cancellable = true)
private void horizon$add(Particle particle, CallbackInfo info) {
    if (particle == ClientParticles.admitted) ClientParticles.admitted = null;
    else if (!ClientParticles.keeps(particle.getClass())) info.cancel();
}
```
- A mod's type falls through to vanilla's body: `makeParticle` (the mod's provider), then `add`, which keeps exactly the
  particle `createParticle` made (`admitted`, set right before the call: the provider's frame is gone by then, so the
  stack can't tell), then the group on the next `tick`. A mod type without a provider makes nothing, as in vanilla.
- Only a mod type reaches the `add` call inside `createParticle`: the HEAD injection cancels every other one.
- `ClientParticles.add` counts `forwarded`.
- Update the class comment: one line, `Mods' particles live here; a vanilla one goes to the window only when Java's own code made it.`

### New `B/mixin/ParticlePacketMixin.java`
```java
@Mixin(ClientPacketListener.class)
abstract class ParticlePacketMixin {
    @Inject(method = "tryAddParticle", at = @At("HEAD"))
    private void horizon$enter(ClientboundLevelParticlesPacket packet, double x, double y, double z, double xa, double ya, double za, CallbackInfoReturnable<Boolean> result) { ClientParticles.enter(); }
    @Inject(method = "tryAddParticle", at = @At("RETURN"))
    private void horizon$exit(...same..., CallbackInfoReturnable<Boolean> result) { ClientParticles.exit(); }
}
```
`tryAddParticle` catches every `Throwable` itself and has two returns, so `RETURN` closes the scope on every path
(vanilla `ClientPacketListener` l. 2345). Register in `horizon_java.client.mixins.json`.

### `B/mixin/ParticleEngineAccess.java` (from SPEC-1)
Add `@Accessor("particles") Map<ParticleRenderType, ParticleGroup<?>> horizon$groups();` and
`@Accessor("particleRenderOrder") List<ParticleRenderType> horizon$order();` (NeoForge's field in the patched class).
New `B/mixin/ParticleGroupAccess.java`: `@Mixin(ParticleGroup.class) interface ParticleGroupAccess { @Accessor("particles") Queue<Particle> horizon$particles(); }`.
SPEC-3 and SPEC-5 read them; here only the debug line does.

### Debug line (`HORIZON_JAVA_DEBUG`)
In `ClientBridge.tick`, after the ticks loop, every 100 window ticks when anything is non-zero:
`Horizon: Java particles: <live> live in <groups> groups, <forwarded> forwarded, <dropped> dropped` (`live` sums the
groups' sizes through the accessors). The scenario of SPEC-9 reads it.

## Testmod (client side; `T/`)

New `T/ClientSpark.java`, registered from `TestMod` beside `ClientWorldBox.register()`:
- `RegisterParticleProvidersEvent` (mod bus): `event.registerSpriteSet(TestMod.SPARK.get(), sprites -> (options, level, x, y, z, xa, ya, za, random) -> new Spark(level, x, y, z, sprites.first()))`.
- `static final class Spark extends SingleQuadParticle`: constructor sets `lifetime = 20`, `gravity = 0`, `friction = 1`,
  `hasPhysics = false`, `xd = 0.1`, `yd = zd = 0`, `quadSize = 0.25`; `getLayer()` returns `SingleQuadParticle.Layer.OPAQUE`.
  It moves +0.1 on x each tick through vanilla's own `Particle.tick`.
- `ClientTickEvent.Post` (game bus): while the main hand holds `TestMod.Wand` and `level.getGameTime() % 20 == 0`:
  `level.addParticle(TestMod.SPARK.get(), 8.5, 102, 8.5, 0, 0, 0)` and `level.addParticle(ParticleTypes.FLAME, 9.5, 102, 8.5, 0, 0, 0)`.
- `TR/assets/horizon_java_test/particles/spark.json`: `{"textures":["minecraft:flame"]}` (a vanilla sprite: no new texture).

## Tests

- **Java unit, new `U/ClientParticlesCheck.java`:**
  - `sourced()` is false with no scope (called from the check's own class, which is Horizon's package);
  - true between `enter()` and `exit()`, false after; nested enters need as many exits;
  - true when called through `fakemod.Caller.call(ClientParticles::sourced)` (new class in the `fakemod` package of
    SPEC-1: `static <T> T call(Supplier<T> s) { return s.get(); }`), so a mod frame on the stack counts;
  - `keeps(fakemod.FakeProvider.class)` true; `keeps(String.class)` false outside a scope.
- `java_mods.py build`, `unit`, `check` (no new problems), `bridges` (unchanged checks pass: particles of a Java-run
  `animateTick` still forward).
- `cargo test -p horizon-client --features java-mods --lib java_` (the renamed log only).

## Done bar (the implementer checks)

- Unit checks pass; `bridges` passes.
- In a testmod world with `HORIZON_JAVA_DEBUG=1`, holding the wand for 10 s: the debug line shows `live` 1 (one spark
  every 20 ticks, living 20 ticks) and `forwarded` 5 per 100 ticks (the flames), and no `reached native, which can't
  draw it` line. The flame shows natively at (9.5, 102, 8.5) once a second.

## Live test (lead)

Play pack, `HORIZON_JAVA_DEBUG=1`, survival world:
1. Stand by a lit campfire 30 s. Expect native smoke as before and the debug line's `forwarded` not rising from it
   (vanilla block entity ticker: dropped, native draws its own). Native smoke unchanged (no doubling).
2. A Mekanism Energized Smelter next to a Creative Energy Cube, smelting raw iron: its active `animateTick` smoke and
   dust (vanilla types, W0 SPEC-1B) still show, and `forwarded` rises.
3. An AE2 Vibration Chamber burning coal (its class overrides `animateTick`, W0 AUDIT; by name its effect is AE2's
   `vibrant_fx`, unverified until this run): the debug line shows
   `live > 0`. Nothing new is drawn yet (SPEC-4).
4. No `a Java mod threw` lines, no `reached native, which can't draw it` lines.
