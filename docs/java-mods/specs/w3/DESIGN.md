# W3: mods' particles, ticked where their code is, drawn natively

Base: `java-mods` at cc1b066b6. Read with `RULES.md`, `COVERAGE-SPEC.md` (W3, principles 0–4), `finish/STATUS.md` (W3 row).
Path short-hands used in every spec:
- `B/` = `mods/horizon/src/java-mods-host/boot/dev/horizonlab/javamods/boot/`
- `B/mixin/` = its `mixin/` folder; `P/` = `mods/horizon/src/java-mods-host/plugin/dev/horizonlab/javamods/plugin/`
- `U/` = `mods/horizon/src/java-mods-host/unit/dev/horizonlab/javamods/boot/`
- `T/` = `mods/horizon/src/java-mods-host/testmod/dev/horizonlab/javamods/testmod/`, `TR/` = `mods/horizon/src/java-mods-host/testmod/res/`
- `G/` = `crates/horizon-client/src/game/`, `R/` = `crates/horizon-client-render/src/`
- Vanilla: `.local/minecraft/26.3/sources/client/net/minecraft/client/particle/`.

No live boot was run: `.local/java-mods/horizon-java-host.jar` was 3.7 KB at the time (another agent mid-rebuild, the stale-jar
problem of STATUS §0), so a boot would have measured someone else's tree. The gap below is proven from the code at
cc1b066b6, javap of the installs' jars and of NeoForge's patched client, the `replaces` reports in `finish/rep-*`, and
`play/game31.log`.

## 1. The gap, with evidence

### 1.1 What the code does today

| Path | Today | Evidence |
|---|---|---|
| Java client `ParticleEngine.createParticle` | Always returns null. Only particles made while `Animate.forwarding()` (Java-run `animateTick`) are copied to native as wire bytes; everything else is dropped. No particle ever lives in Java's engine. | `B/mixin/ParticleEngineMixin.java` (whole file), `B/ClientParticles.java` |
| Those forwarded particles in Rust | Decoded with Horizon's vanilla-only id table. A mod type (Java registry id after vanilla's) has no native particle: logged `Java particle <id> has no native particle yet (W3)` and dropped. | `G/java_screens.rs` `java_client_particles` (l. 1099), `read_particles` (l. 364); `horizon_protocol::entity_data::Particle::read` |
| `ServerLevel.sendParticles` (the 13-argument one, bridged) | Rust `particle_of` knows vanilla type names and the option kinds 0–6. A mod type (`ParticleType::from_name` fails) or any other options class (item, vibration, trail, shriek, sculk charge, every mod `ParticleOptions`: kind 9) logs `a Java mod's particle carries a payload Horizon can't send` and returns 0. | `G/java_bridges/effects.rs` `particle_of`, `level_send_particles`; `B/Effects.java` `particle` (kind 9); testmod pins it: `T/ChecksEffects.java` "sendParticles of an item sends none" |
| The per-player overload `sendParticles(ServerPlayer, ...)`, mods' own `ClientboundLevelParticlesPacket`s, mods' payloads that spawn particles | Not bridged: the packet reaches the player's Java client, whose `createParticle` drops it. Vanilla types included. | `ParticleEngineMixin` |
| Mod particles made outside `createParticle` (`new XParticle(...)` then `ParticleEngine.add`) | `add` is not filtered: they enter Java's engine and tick, but nothing draws them. | AE2 `CableBusBreakingParticle` has no provider (javap: no `Provider`/`Factory` inner class), made in `CableBusBlockClientExtensions.addDestroyEffects`/`addHitEffects` |
| Destroy and hit particles of a mod block whose client extension overrides `addDestroyEffects`/`addHitEffects` | Native draws vanilla's crumbs from the block's particle sprite; the mod's code never runs. | `G/game.rs` `destroy_particles` (l. 4095), `crack_particle` (l. 4109); javap: `appeng.client.block.cablebus.CableBusBlockClientExtensions` and `mekanism.client.render.RenderPropertiesProvider$2` override them; NeoForge's patched `ClientLevel.addDestroyBlockEffect` and `addBreakingBlockEffects` call `IClientBlockExtensions.addDestroyEffects`/`addHitEffects` (javap -c callee list, as W0 did) |
| NeoForge custom particle groups (`RegisterParticleGroupsEvent`) | The event fires at boot; their `extractRenderState` never runs (no frame), so nothing draws. | `finish/rep-*/replaces.json` events: Mekanism (`LaserParticleGroup`), AE2 (`LightningFXGroup`), puzzleslib register groups |
| `/particle <mod type>` | Parses, then `cmd_particle` fails `particle.notFound` (`ParticleType::from_name`). | `G/commands/world_cmds.rs` l. 693 |

The native engine already supports two things W3 reuses: drawing quads by sprite name from the shared atlas
(`R/particles.rs` `render_inner`, `Sprite::Named`), and native mods' own types (`ParticleOptions::named`, `mod_sets`,
`horizon:behaviour`). Java mods' types never reach the second: their `particles/*.json` have no behaviour block and their
providers are code.

### 1.2 What the bar's mods register (javap and jar listings, `scan/particle-json-showcase.txt`)

| Mod | Types | Provider / particle classes | Drawing |
|---|---|---|---|
| Mekanism | `jetpack_flame`, `jetpack_smoke`, `laser`, `radiation`, `scuba_bubble` | own `Factory` classes; particles extend vanilla `FlameParticle`, `SmokeParticle`, `BubbleParticle` with overridden `extract`, `getLightCoords`, `tick`; `LaserParticle extends Particle` with its own `ParticleRenderType` | quads, plus the custom `LaserParticleGroup` |
| AE2 | `crafting` (`ItemParticleOption`), `energy_fx`, `lightning_arc_fx`, `lightning_fx`, `matter_cannon_fx`, `vibrant_fx` | own `Factory` classes; `EnergyFx` overrides `extractRotatedQuad`, `CraftingParticle` overrides `extract`/`getQuadSize`; `LightningFX` has its own group | quads, plus `LightningFXGroup` |
| Building Gadgets 2 | `itemflowparticle`, `fluidflowparticle` (options carry an `ItemStack`/`FluidStack`) | `registerSpecial`-style static `FACTORY`; particles extend `BreakingItemParticle` (item and block atlas sprites) | quads |
| Showcase also | BOP 37 types, lootr 2, sophisticatedcore 1, plus library helpers (architectury, balm, glitchcore, puzzleslib) | all own classes | quads |

- **No mod in either install uses one of vanilla's provider classes**: a constant-pool scan of every jar finds only the
  `ParticleProvider` interface under `net/minecraft/client/particle/*Provider*`. So COVERAGE-SPEC's "plain sprite
  particles go native" tier has no customer in the bar (decision D1).
- **No mod mixes into vanilla particle code** (`finish/rep-showcase`, `finish/rep-rs`: every `client/particle` row is
  `override`/`implement` in the mod's own classes, routing `java-own-type`/`java-already`). M2 has nothing native-owned
  to route for W3. lootr's `MixinFrustum` targets `Frustum`, a Java-only class: it executes.
- Particle events: only boot-time `RegisterParticleProvidersEvent` and `RegisterParticleGroupsEvent`, which the real
  reload already fires (`docs/java-mods.md`, "The client's resource reload"). M1 adds no gameplay event for W3.
  `EffectParticleModificationEvent` is a `LivingEvent` (W9).

### 1.3 Vanilla 26.3's particle pipeline (the shape the design follows)

- `ParticleEngine.createParticle` → provider (`ParticleResources.providers`, keyed by `Identifier` in NeoForge's patch) →
  `add` → `particlesToAdd`, joined to a `ParticleGroup` per `ParticleRenderType` on the next `tick`.
- Drawing is already a recording: `Particle.extract(QuadParticleRenderState, Camera, partial)` adds one entry per quad
  (layer, camera-relative x/y/z, rotation quaternion, size, atlas UVs, ARGB, light coords); `QuadParticleRenderState`
  turns each into four corners (`renderRotatedQuad`). A non-quad group's `extractRenderState` returns a
  `ParticleGroupRenderState` that `submit`s geometry to a `SubmitNodeCollector`, at `LevelRenderer.submitFeatures`.
- The camera's part: `FacingCameraMode.LOOKAT_XYZ` (the camera rotation), `LOOKAT_Y` (its yaw part), then `rotateZ(roll)`.

## 2. The approach

Mods' particle code runs only in the Java client's real `ParticleEngine` (their providers, constructors, `tick`s and
`extract`s), on the client tick it already runs. Native draws them from a per-tick recording, as it draws block entity
renderers and the HUD. Vanilla particles stay native, and Java hands native the vanilla particles only its own code made.

1. **Boot facts (SPEC-1).** Which particle types are mods' (`BootFacts`, the registry entries added after vanilla's
   bootstrap, never a namespace) and the Java registry's id ↔ name table, exported once to Rust. A vanilla type whose
   provider a mod replaced is reported at boot (none in the packs) and not routed.
2. **Who owns a particle made in Java (SPEC-2).** A mod type always lives in Java's engine. A vanilla type is copied to
   native when Java's code made it and native did not: under `Animate.forwarding()`, while the Java client handles a
   particle packet (all Java-server sourced), while it runs a routed block effect (SPEC-7), or when a mod's class is on
   the call stack (mod event handlers, mod particles' ticks, mods' payload handlers, mods' block entity tickers). Otherwise
   it is dropped, since native makes its own copy (vanilla block entity tickers, the Java `LocalPlayer`'s vanilla tick).
   `ParticleEngine.add` follows the same rule.
3. **Recording (SPEC-3) and drawing (SPEC-4).** After the tick, every live quad particle is extracted twice (partial 0
   and partial 1) with a recording camera at the window's eye and identity rotation, and once more with a probe rotation
   to classify how it faces the camera. Sprites are named by texture file with sprite-relative UVs (`Sprites.locate`,
   `SubmitCapture.file`, as world recordings do). Native interpolates the two samples with the frame's partial and
   applies the frame's camera rotation per facing mode, in its particle pass, with its culling. No Java call per frame.
4. **Custom groups (SPEC-5).** NeoForge's registered groups (`extractRenderState` + `submit`) are submitted into W1's world
   recording at vanilla's place (`submitFeatures`), so W1's passes, meshes and timing draw them.
5. **Server sends (SPEC-6).** `ServerLevel.sendParticles` keeps its Rust port for what native can carry; a mod type or an
   options class native can't carry runs vanilla's own body, whose packets reach each player's Java client (in the
   window's process or over `horizon:java`), where SPEC-2 takes them. Fixes vanilla item/vibration/trail particles sent by
   mods too.
6. **Mod blocks' destroy and hit effects (SPEC-7).** States whose client extension overrides `addDestroyEffects` or
   `addHitEffects` are exported at boot; native's `destroy_particles`/`crack_particle` hand those to the Java client
   (`ClientLevel.addDestroyBlockEffect` / `addBreakingBlockEffects(pos, dir, false)`), which runs NeoForge's hook.
7. **`/particle` (SPEC-8).** A Java-owned type runs through vanilla's `ParticleArgument` and per-player `sendParticles` in
   Java: vanilla parity and the deterministic trigger the live tests and the oracle comparison use.
8. **Acceptance (SPEC-9).** Scenario, debug counters, NeoForge comparison of created-particle counts.

Single player, LAN and the dedicated server share all of it: the server side is the Java server view (SPEC-6, 8), the
client side is each window's own Java client (SPEC-2..5, 7).

## 3. Decisions (with reasons)

- **D1. Every mod type ticks in Java; no native "plain sprite" tier.** 0 of the 13 (play pack) and 0 of the showcase's 53 with particle JSONs
  mod types use a vanilla provider class (§1.2), so the tier would serve nobody. Its later addition needs only SPEC-1's
  facts plus one provider-class check.
- **D2. Ownership of vanilla-type particles made in Java by scopes plus a stack check.** Alternatives rejected: forwarding
  everything duplicates vanilla block entity tickers (campfire smoke: the Java client keeps a copy of every block entity
  with its ticker, `ClientBridge.blockEntity`) and the Java `LocalPlayer`'s vanilla particles; scopes alone miss mod event
  handlers and payload handlers. The stack check (`StackWalker`, `Shells.isMods` on each frame's declaring class, the
  same ownership test W5 uses) runs only for vanilla types outside the scopes, a few per tick since W0 removed Java's
  `animateTick`. Mods' mixin handlers merged into vanilla classes don't count as mod frames; no pack has one in client
  particle paths (§1.2).
- **D3. Two samples a tick, interpolated natively.** Vanilla lerps position and roll linearly in partial; the
  size curves some particles use (`FlameParticle`: `1 - s²/2`) are approximated by a line over one tick (error below
  `quadSize/(8·lifetime²)`). Facing is classified per quad from a probe camera, so the frame's real camera still turns
  each billboard. A quad whose rotation depends on the camera in another way is drawn as recorded at the tick (the
  precedent of block entity renderers that follow the camera).
- **D4. Custom groups through W1's world recording.** W1 already classifies pipelines into native passes (additive
  lightning, translucent beams, lines) and its timing rule (per tick, per frame only while the recording changes) is
  the user's. Cost: while a Mekanism laser or AE2 lightning lives, the world records per frame. Alternative for the
  user: two-sample raw quads like D3 (no per-frame Java, but pass classification and lines duplicated in W3).
- **D5. Server: vanilla's own `sendParticles` body for what native can't carry.** Rust building Java packets would need
  every mod's options codec; Java has them. The guard needs the method's arguments, so `BridgeProcessor`'s `@` guards
  gain an argument-taking form (SPEC-6), reusable by other families.
- **D6. Timing.** The recording comes with the tick's status (the tick job may finish a frame or two into the tick), so
  Java particles lag native ones by at most what the HUD and world recordings lag: they move smoothly, never jump.

## 4. Specs

| Spec | Scope | Depends on | Lands alone because |
|---|---|---|---|
| [SPEC-1](SPEC-1.md) | Particle types in `BootFacts`; `particle_facts` op; Rust `java_particles.rs` facts; boot report | none | facts only, read by later specs |
| [SPEC-2](SPEC-2.md) | `ParticleEngineMixin` ownership: mod types live in Java, vanilla ones forwarded only when Java-sourced; packet scope; `add` filter; testmod `spark` type | SPEC-1 | Java particles tick (not yet drawn); vanilla particles from mod payloads/handlers now draw |
| [SPEC-3](SPEC-3.md) | `ParticleRecorder`: two samples, facing, sprites; status field; Rust parse into the engine | SPEC-2 | recorded and counted, drawn by SPEC-4 |
| [SPEC-4](SPEC-4.md) | Native drawing of the recording: interpolation, facing, light, layers, culling | SPEC-3 | first visible step |
| [SPEC-5](SPEC-5.md) | Custom particle groups into W1's world recording; testmod `beam` group | SPEC-2, W1 (merged) | independent of SPEC-3/4 |
| [SPEC-6](SPEC-6.md) | `sendParticles` guard with arguments; Java body for mod types and uncarriable options | SPEC-1, SPEC-2 | server-sent mod particles reach Java clients |
| [SPEC-7](SPEC-7.md) | Mod blocks' `addDestroyEffects`/`addHitEffects` routed to the Java client | SPEC-1, SPEC-2 (SPEC-4 to see them) | native skips only flagged states |
| [SPEC-8](SPEC-8.md) | `/particle` with Java-owned types | SPEC-1, SPEC-6 | command only |
| [SPEC-9](SPEC-9.md) | Scenario `w3_particles.json`, NeoForge comparison, user checklist | all | tests only |

Order: SPEC-1, SPEC-2, then SPEC-3 → SPEC-4, SPEC-5, SPEC-6 → SPEC-8, SPEC-7 in any order after SPEC-2; SPEC-9 last.
Wire allocations: status gains one field after the particle list (SPEC-3); `Input` kind 30 `BlockEffect` (SPEC-7;
W3 reserves 30–31, W5 owns 16–25); Java ops `particle_facts` (SPEC-1), `server_particles` (SPEC-8).

## 5. Out of scope (with where it belongs)

- Particles bound to entities in the Java client (`TrackingEmitter`, `ItemPickupParticle`, other players' Mekanism
  jetpack flames, mods' entity particles): the Java client has no entities (W2). The local player's own jetpack works.
- Mod biomes' ambient particles (BOP's end biomes list `biomesoplenty:end_spore`, `wisp_bubble`, `binary` under
  `minecraft:visual/ambient_particles`): `animate_tick.rs` `AnimateTables::load` reads only `data/minecraft/...` biome
  files, so mod biomes have no ambient particles at all (W10 loads mod biomes' attributes; a native-picked Java-owned type
  then needs a spawn input like SPEC-7's).
- Mod fluids' drip particles (BOP `Fluid#getDripParticle` overrides): native's drip loop; cosmetic, W0 SPEC-1 follow-up.
- A mod replacing a vanilla type's provider: reported at boot by SPEC-1, not routed (no pack does it).
- Data-driven server particles with mod types (enchantment `spawn_particles`, mob effect particles): none in the packs'
  data (scan of `data/*/enchantment`); `EffectParticleModificationEvent` is W9.
- Clients and LAN guests without a JVM see no mod particles (W14).
- Mods' particle shaders or render pipelines W1 can't classify (W1/W8).
- Vanilla `ItemPickup`/`ElderGuardian` groups holding a mod-made particle: not recorded (no pack does it).
