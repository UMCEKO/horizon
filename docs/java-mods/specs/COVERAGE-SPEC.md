# Java mods on Horizon: the spec to 100% coverage

**Goal:** every NeoForge 26.3 mod behaves on Horizon exactly as on a real NeoForge client and server, observable to players and to other mods, with all rendering native and no per-mod code.

**100% means:**
- every mod hook point the real game runs, runs in Horizon at the same moment with the same inputs;
- its effects reach the player through native code;
- this is proven by the measures in section 2.

## 1. Principles (binding on every workstream)
0. **Java is only a modloader** (user, 2026-11-04):
   - **Detected behaviours only.** Only the vanilla behaviours a loaded mod is detected to change run in Java. Detection reads the jars statically (the `replaces` report: mixins, overrides, event listeners, data overrides), and the routing is decided at boot. Every other behaviour stays native.
   - **The Java client stays only while it duplicates nothing native does.** Any duplicate gets removed, and native is fixed to cover it (see W0).
   - **Consequences:**
     - W5 draws mods' additions over native screens;
     - W12 routes a patched vanilla method to Java only when a loaded mod patches it.

**W0. Java client duplicate audit.** List what the Java client executes per tick and per frame (JFR on its thread). For each item that native already does (ticking entities, light, chunks, particles, sounds, HUD layers drawing to nothing, and so on), either remove it and fix native, or justify it with a detected mod need.
1. **Faithful, not approximate.** Replicate vanilla/NeoForge order, timing and inputs. No polling, mirroring, copy-back, fallbacks or workarounds. Refactors are allowed.
2. **Native owns rendering and vanilla gameplay.** Java owns mods' code. Every effect has exactly one owner, so nothing happens twice.
3. **Boot data lands in native at boot.** Only per-instance, per-tick or per-stack state crosses at runtime, and only when it changes.
4. **No per-mod code.** Decisions come from registries, ownership sets and own-code flags, never names.
5. **Process:**
   - a senior (me or an Opus agent) proves the root cause;
   - a junior (Sonnet) implements a precise spec;
   - the user tests anything visual or needing real input;
   - I run the full self-check, and commit.

## 2. Measuring coverage
- **M1. Event parity.** Run scripted gameplay on real NeoForge and on Horizon with the same mods and seed, and diff the counts and order of every event (JFR on `Event.<init>`), as the boot comparison did. Extend `/home/umceko/hz-poc/agents/bootcmp/` from boot to gameplay: join, walk, mine, place, craft, fight, use machines, change dimension, leave.
  - **Target:** every event class Forge fires also fires in Horizon, with matching counts within the run's natural variance.
- **M2. Mixin audit.** At boot, list every mod mixin target (Mixin's own config gives the target classes and methods). Classify each target as **executes in Java** (it has effect) or **native-owned** (the mixin silently does nothing).
  - Ship it as a report (`java_mods.py mixins`) and a boot-log warning.
  - **Target:** no native-owned target left unrouted (see W12).
- **M3. Behaviour parity harness.** Forge against Horizon comparisons of state (`data get`, inventories, energy, positions), as the energy agent's suite did. Grow it into a parity harness per mod family: storage I/O, machines, power, mining, tools, entities, worldgen.
- **M4. Mod pack breadth.** Run the top 50 NeoForge 26.3 mods by downloads, one by one and then together. Each must boot, join, pass a smoke script, and survive a user playtest.
  - Port pre-26.3 mods only with isolated porters, for testing only (see `mod-ports-for-testing`).
- **M5. User playtests.** One round per batch of fixes, with screenshots, then the 10–15 player multiplayer playtest with always-on profiling.

## 3. Workstreams
**Status key:** ✅ done and user-verified · 🟡 partial · ❌ missing · ❓ unverified.

### Client side

**W1. Mod world rendering ❌ (being designed now).**
- Level render stage events, `SubmitCustomGeometryEvent`, `PrepareRenderBuffersEvent`, `AfterTranslucentBlocks`/OIT, `RenderHighlightEvent`, and the block outline override.
- **Design:** fire them in vanilla frame order on the Java client with the real camera and frustum. Record the submitted world-space geometry (pipeline, textures, blend, lines) into a world recorder. Native draws it in the matching pass.
- Record per tick, and per frame only while the recording changes, as for the HUD.
- **Accept:** Building Gadgets' preview and destruction overlay, and mods' block highlights.

**W2. Entities in the Java client ❌, plus mod entity rendering ❌.**
- The Java client's level has no entities.
- **Design:**
  - The server's Java side sends the client real entity packets (add, data, move, remove) for the entities a player tracks, as `ServerEntity` does. That needs W9.
  - The Java `ClientLevel` then holds real entities, so mods' client logic sees them: entity clicks with mod handling, `EntityJoinLevelEvent` on the client, and mob rendering hooks.
  - Mod entity renderers (`EntityRenderersEvent`, layers through `AddLayers`) are recorded per entity, like block entity renderers: a pose program per tick, interpolated natively.
  - Vanilla entities stay native-drawn, but mod **layers** on them (armour models from `IClientItemExtensions`, Curios-style render layers) are recorded and composed natively.
- **Accept:**
  - a mod mob draws and animates;
  - the Meka-Suit armour model shows on the player;
  - Jade shows mobs;
  - a click on an entity goes through mods.

**W3. Particles ❌.**
- **Design:**
  - At boot, export each mod particle type's sprite set and whether its provider is a plain sprite particle, so simple ones go native.
  - Particles with custom behaviour tick in Java's `ParticleEngine` on the client tick. Record their billboards per tick and interpolate them natively per frame. Spawns come from the server's level events and mod payloads, as vanilla.
- **Accept:** active Mekanism machines emit their particles, and mod spell or effect particles move correctly.

**W4. Camera, FOV, fog and movement hooks 🟡.**
- Missing: `ComputeFovModifierEvent`, `ViewportEvent` (angles, fog colour and density, FOV), `MovementInputUpdateEvent`, `CalculatePlayerTurnEvent`, and client-side motion changes in `LocalPlayer.tick`/`aiStep` from mods (jetpacks, step assist).
- **Design:**
  - The window's motion step asks the Java client per tick for the mod-modified input and motion deltas from the real `LocalPlayer` path, so native stays the motion authority but applies vanilla's mod hooks. Only when a listener or override exists, detected at boot.
  - FOV, camera and fog values come from the real events each tick and are applied natively.
- **Accept:** Meka-Suit jetpack and step assist, a zoom mod, and mod dimension fog.

**W5. Mods modifying vanilla screens and HUD ❌.**
- Covers `ScreenEvent.Init`/`Render` on vanilla screens (extra buttons, recipe overlays such as JEI and EMI, Curios slots), cancelling or wrapping vanilla HUD layers, and container screen decorations.
- **Design:** when any mod listens for a vanilla screen class (detected at boot from event listeners or mixins), open the **Java** version of that vanilla screen and draw it like mod screens, instead of the native one. Native screens stay where no mod touches them. Do the same for vanilla HUD layers a mod cancels or wraps: record that layer from Java instead of drawing it natively.
- **Accept:** JEI or EMI overlays on the inventory, and Curios slots.

**W6. Client commands, chat and toasts ❓.**
- `RegisterClientCommandsEvent`, `ClientChatReceivedEvent` and `ClientChatEvent` changes, and mod toasts (`ToastAddEvent`, custom `Toast`s).
- **Design:**
  - chat typed in the window goes through the Java client's real send path first, so client commands and mods' chat edits run;
  - received chat passes the real receive events before native display;
  - mod toasts are recorded and drawn natively, as the HUD is.

**W7. Mod fluids, sky, weather and dimension effects ❓.**
- `IClientFluidTypeExtensions` (textures, tint, fog), `DimensionSpecialEffects`, custom sky and weather renderers.
- Mod fluid textures and tints are exported at boot. Custom sky and weather renderers fall under W1's world recorder.

**W8. Remaining client details 🟡.**
- custom font providers from mods (check native fonts take them at boot);
- `SelectMusicEvent` and `PlaySoundEvent` replacement;
- mods' post effects and shaders (W1 plus a native post pass, low priority);
- third-person camera in renderer probes (the renderer camera uses the eye only today);
- the Jade per-frame recording spikes (13–21 ms; the fidelity rule parks this).

### Server side

**W9. Entities on the server ❌ (the largest gap).**
- Today entities are Rust only, so Java never sees them.
- **Design:**
  - **Vanilla entities:** Java views of native entities (as for players: views plus bridged natives, with write-through, no copy-back). Fire the entity events mods hook in vanilla order: join and leave level, tick, `LivingHurt`/`Damage`/`Death`/`Drops`, `LivingChangeTarget`, mob spawn and finalize, item pickup, interact, mount, effects, and the rest from the boot comparison's list.
  - **Mod entity types:** these are simulated by their own Java code on the server (ticked like mod block entities), with movement and collision through bridged natives, and tracked to clients by vanilla's entity tracking (W2).
  - Entity capabilities and data attachments, persisted.
- **Accept:**
  - a mod mob spawns, moves, fights, drops and saves;
  - `LivingDropsEvent` changes vanilla mob drops;
  - AE2's or Mekanism's entity-touching features work.

**W10. Worldgen from mods ❓.**
- Mod biomes, biome modifiers, configured and placed features (including mod ores), structures, carvers and dimensions.
- **Design:**
  - Data-only features using vanilla feature types go native at boot (they're already data).
  - Features with mod `Feature`/`StructurePiece` code run in Java during the matching generation stage on a chunk view, with block writes bridged.
  - Mod dimensions get a native level whose generator and effects come from the registry.
- **Accept:** Mekanism's ores generate in new chunks at Forge's rates (compared by M3 on the same seed), and a mod biome and a mod structure appear.

**W11. Recipes, loot, enchantments and crafting with mod code ❓.**
- Covers mod recipe types used in **vanilla** stations (crafting table, furnace, smithing: special recipes with code such as NBT-transfer upgrades), Global Loot Modifiers and loot conditions or functions in code, enchantment effect components with mod types, `ItemCrafted`/`ItemSmelted`/`AnvilUpdate` and similar events, and villager trades from mods.
- **Design:** a vanilla station whose recipe is a mod recipe class runs `matches` and `assemble` in Java. Loot with mod modifiers or functions is evaluated in Java's real loot tables. Fire the crafting events. All when present, detected at boot.

**W12. Mixins into vanilla gameplay ❌ (the long tail).**
- A mod mixin targeting vanilla server code that Horizon runs natively has no effect today.
- **Design:**
  - Use the M2 report.
  - For each native subsystem with a mod mixin target, route that subsystem's call through Java's real method for that game session. That is, the native implementation delegates to Java when the boot report says the method is patched: per method, decided at boot.
  - Start with the commonest targets in the M4 pack: hoppers, crop growth, mob AI goals, explosions, item entities, and player tick.
- **Accept:** the M2 report shows no native-owned target that has an effect.

**W13. Remaining server areas 🟡.**
- mod commands through Brigadier (`RegisterCommandsEvent`; check they parse and run, and that suggestions reach clients);
- `RegisterConfigurationTasksEvent` and mods' configuration tasks;
- player events (`LoadFromFile`/`SaveToFile`, `NameFormat`, clone, respawn and change dimension);
- game rules and data maps from mods;
- custom advancement criteria triggers;
- block events (`NeighborNotify`, `EntityPlaceEvent`, `FarmlandTrample`, `CropGrow`, `BonemealEvent`, sapling growth, fluid placement);
- explosions (`ExplosionEvent` start and detonate).
- For each: fire from the real vanilla path at the right moment, and verify with M1.

### Platform

**W14. Multiplayer at scale 🟡.**
- Dedicated server and remote clients work with 2 players.
- Remaining: LAN guests without a JVM, server-pushed packs and versions for guests, the 10–15 player playtest with profiling, and removing the client/server `GameBridge.LOCK` (parked by the fidelity rule until behaviour is complete).

**W15. Player install and launcher 🟡.**
- The install path and launcher panel work on Linux.
- Remaining: test on Windows and macOS; the opt-in switch (no Java needed when off: load the JVM at runtime, not linked at build); a release build with java-mods and the shipped Horizon jars; signed checksums.

**W16. Mod set and ports.**
- Official 26.3 builds where they exist.
- Isolated test ports otherwise: Building Gadgets 2 ✅ ported; Refined Storage 2 ✅ ported; Create 🟡 (libraries ported, Create about 3.5k errors). Treat a port's own bugs as a port issue, not a Horizon issue, until it's checked against Forge with the same jar.

### Horizon itself

**W17. What vanilla to port, and why.**
- **Source of truth:** the port ledger. Its verdict map has 1,700 methods marked `missing`, plus about 3,600 marked `skip:` with reasons. The verdicts live in `tools/port_map/*.tsv`.
- **Rule:** a `missing` method is ported only with evidence for at least one reason below. Every decision is recorded in the verdict map with its reason, so it's decided once.
  - **P1, players see it:** a vanilla-observable difference in gameplay, rendering, sound, saves, protocol or commands. The evidence is a failing harness, a user report, or a comparison diff.
  - **P2, mods need it:** the behaviour is native-owned in Horizon, and at least one of these holds:
    - a mod event is posted from it;
    - a mod mixin targets it;
    - a mod class overrides it (the override never runs if native never calls the vanilla method);
    - a mod calls it on a path where Java can't serve it.

    The evidence is a mod in the test pack or the top-50 list.
  - **P3, a dependency:** another P1 or P2 port needs it.
- **Not ported:** a recorded `skip:` reason:
  - `java` (Java infrastructure);
  - `data` (served by data);
  - `debug`;
  - `service` (telemetry, realms, auth);
  - `ui` (Horizon's own design);
  - `server` (Java server infrastructure);
  - a new one, `java-serves`: Java's own copy runs it for the mods that touch it, and native gains nothing.
- **Deferred:** `missing` with no evidence stays as it is and isn't worked on until evidence appears. That's the default, not a decision to port.
- **Ranking within P1 and P2:** how many pack mods reference it; how often the real client runs it in the comparison session; how visible it is to the player (from a crash, to a wrong result, to cosmetic).
- **Tool, `port_triage.py`:** for each `missing` method it gives the signals, a suggested verdict and the reason. The signals come from:
  - a bytecode scan of the pack's mod jars (calls, overrides, mixin targets);
  - the explain index (events posted from the method);
  - comparison runs (execution samples on the real client);
  - harness coverage.

  The user decides the borderline cases.
- **Already found by the first comparison:**
  - chunks are saved only when modified, where vanilla saves every generated chunk (P1);
  - quitting doesn't unload chunks (P1, P2);
  - `ServerLevel.gameEvent` isn't ported as a method (check P1 for sculk);
  - attribute-modifier item predicates are absent (P1 for commands and advancements);
  - loot tables don't load through vanilla's path (P2 for loot table events).

## 4. Already done (user-verified this week)
- **Boot:** registries; boot facts and data into native; the content cache; the failed-boot refusal; the real resource reload; atlases and models.
- **Java client:** a faithful client (login, configuration, real tick, input, key bindings, clicks with mods able to cancel, screens, sounds, the world-session lifecycle).
- **Drawing:**
  - creative tabs;
  - mod HUD layers and stacking;
  - Jade with its timing and animation;
  - item names and tooltips;
  - per-block models;
  - block entity renderers, including camera-dependent ones;
  - mod GUI textures, tiling and blur (in progress);
  - radial menus (in progress).
- **World and gameplay:**
  - block entities live and written through (no item duplication or voiding);
  - chunk and level events with full chunk data;
  - mining speed, harvest, instant breaks and vanilla's full destroy sequence for mod blocks;
  - energy parity with Forge;
  - the bucket and place-sound fixes.
- **Platform:** the memory cap (Java about 1 GB); always-on server profiling; dedicated multiplayer (2 clients); player installs and the launcher.

## 5. Order of work
1. **Now:** finish W1 (world rendering), then W9 together with W2 (entities, server and client). Together they're the largest player-visible gaps.
2. **Next:**
   - W3 particles and W4 camera and movement;
   - W5 vanilla screen modification, which JEI and EMI need, so it's important for packs;
   - start the W10 worldgen check, with M3 on ores.
3. **Then:** W11, W13, W6, W7 and W8, driven by the M1 event-parity diffs and M4 breadth runs.
4. **Long tail:** W12 mixin routing by the M2 report; W14 scale and lock removal; W15 platforms.

Each workstream goes through the same steps:
1. a senior proves the gap with M1, M2 or M3;
2. a spec;
3. junior implementation with unit and Java checks;
4. my full self-check;
5. a user playtest round;
6. a commit.

Each item counts as covered when its M1 events match and the user signs off.

## Optimization debt (must be paid, at the root cause)
- **Rust→Java call cost (about 8 µs per request, measured 2026-11-04 by the entities design).** Per-entity tick events fire in exact vanilla order, so 15 players with about 190 mobs each would spend about 23 ms per tick in calls. Fix the crossing itself, so every bridge gets cheaper; never reorder what mods see. The user said: "DO NOT FORGET TO OPTIMIZE AT THE ROOT CAUSE". This blocks the 10–15 player playtest.

## Milestone: big-pack overhead run (after the crossing fix and the W0 removals land)
The pack is the top-50 set plus Mekanism, AE2, Jade and Building Gadgets 2. Profile Java per tick on the server and on the client (`HORIZON_PROFILE`, `profile_report.py`) at 1, 5 and 15 players (bots), with JVM memory. Rank Java time by system. Anything vanilla that native could own goes back to native. The goal, from the user: "offload as much as we can to the native".
