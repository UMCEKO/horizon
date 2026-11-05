# Java mods: a map for agents

Where things live in the embedded-JVM path. What it does and why: [java-mods.md](java-mods.md) (read it first, it is not
repeated here). Rules for agents on this branch: the agents' `RULES.md` (outside the repo; see Gotchas).

Paths: `H/` = `crates/horizon-java-host/src/`, `G/` = `crates/horizon-client/src/game/`,
`B/` = `mods/horizon/src/java-mods-host/boot/dev/horizonlab/javamods/boot/` (the Java the JVM runs),
`J/` = `mods/horizon/src/java-mods-host/`, `R/` = `crates/horizon-client-render/src/`,
`T/` = `tools/`. Everything under `G/java_*.rs` except `java_hooks.rs` and `java_screens.rs` exists only with
`--features java-mods` (`game.rs` lines 127 to 145); those two compile always, with empty stubs when it is off.

## 1. Overview

### Processes and threads

One process: the game, with the JVM inside it (JNI). No second process, except a dedicated server's own game.

| Thread | Who | What runs there |
|---|---|---|
| `Java mods` | Rust, `H/lib.rs` `start` (16 MB stack) | The boot: `create_vm`, `register_natives`, `try_run_fml` (`Host.run` then `Boot.main`), then `prepare`, `boot_facts`, the lists the host keeps |
| `Server thread` | Rust, the integrated or dedicated server's `Game` | Every server-side call: `Host::request` attaches it to FML's server `ThreadGroup` (`Host::attach`) so `EffectiveSide` says server |
| Window thread | Rust, the render loop | `client_*` calls that must be answered now (clicks, frames of an open screen, input) |
| `Java client` | Rust, `G/java_screens.rs` `Worker::start` | One client tick's calls as a `Job` (`client_tick`, then the geometry, items and text the status asks for); the window never waits on it |
| `Render thread` | Java, `B/ClientBridge.java` `install` (single-thread executor) | Every `client_*` op, whichever Rust thread asked: `ClientBridge.call` submits to it. It is `Minecraft.gameThread` |
| `Java mods' facts` | Rust, `G/java_content.rs` `content` | After a cached launch, compares the live boot's facts with the cache (`verify`) |

- One lock: `GameBridge.locked` takes `LOCK`, so the server's calls and the window's `client_*` calls run one at a time.
  Exempt: `component_read`, `component_write`, `profile_take`, `profile_dump` (a network thread must not wait out a tick).
- Rust enters Java through `Host::request(op, bytes)` or `Host::call_with(op, write, read)` → `Host.dispatch(int op, ByteBuffer arena, int length)`
  → `Crossing.dispatch` (`B/Crossing.java`): the op is an index into the table `Host.ops()` returns at the boot's end, and
  the request and reply travel in a Java-owned direct buffer, one per thread and nesting level (`H/lib.rs` `Arena`,
  `Crossing`, `Nesting`). About 100 ns for an empty request, from 1,100 ns by name (`java_mods.py crossing`). The boot's own
  string ops go `Host::call` → `Host.call` → `Boot.op`. Java enters Rust through bridged natives (section 3) and
  `Host.onEvent`, `shapeFlags`, `shapeHandle`, `shapeFree`, `link` (`H/lib.rs` `register_natives`).
- Server side: real `ServerLevel`, `ServerPlayer`, `MinecraftServer` built without a world (`GameBridge.prepare`,
  `ServerView.create`), filled from Horizon for each call. Client side: a real `Minecraft` without a window
  (`ClientLoader`, `ClientBridge`), fed only by the server's packets (`Clients` queues, or `Wire` over `horizon:java`).

### Phases

| Phase | Where | What happens |
|---|---|---|
| Install | `T/java_mods.py` `install_pack` | JDK 25, NeoForge libraries, the three host jars, mods, asset index; writes `horizon-java.args` (`jvm`, `cp`, `prog`, `dist`, `mod`, `analysis` lines) and `analysis/entity-facts.json` (`T/mod_replaces.py` `install_analysis`, redone only when the jars or the analysis tools change) |
| Launch | `java_mods.py` `launch`, `launch_env` | Sets `HORIZON_JAVA_RUN` (the run folder), `HORIZON_JAVA_HOST_JAR`, `JAVA_HOME`, scratch homes |
| Boot | `G/game.rs` `run` → `java_content::start(java_bridges::bridges())` | JVM starts behind the title screen; section 2 |
| Content | `G/mods.rs` → `java_content::carried` | The first launch waits ("Loading Java mods", `java_content::note`); cached launches don't |
| World load | `G/java_hooks.rs` `java_hook_world` → `java_world_phase` | `server_start` (`GameBridge.lifecycle`): levels, `ServerAboutToStart`, `ServerStarting`, `LevelEvent.Load`, `ServerStarted`; then `java_chunks_changed` for loaded columns |
| Runtime | `native_hooks.rs`, `game.rs` | Hook points call Java (sections 3 and 4) |
| World close | `integrated.rs`, `dedicated.rs` → `java_server_stopping`, `java_close_levels` (`G/levels.rs`) | `server_stopping`, `players_removed`, every column `unload`ed, saved data written |

## 2. Boot: what lands in native, and where

| What | Java side | Rust side | Used for |
|---|---|---|---|
| Registry snapshot, owned set | `B/BootFacts.java` `snapshot`, `seal`, `owns`, `ownsBlock`, `ownsItem`, `ownsBlockEntityType`; `B/mixin/MapperBootstrapMixin.java` (client mappers) | `H/lib.rs` `Items`, `Host::owns_item` | "Is this a mod's?" in Java and Rust. Never a namespace |
| Facts JSON (`schema` 3) | `BootFacts.export` over `mods/horizon/src/facts-shared/java/dev/horizonlab/facts/FactsWriter.java` | `horizon_port_pack::pipeline::carry_boot` (`oracle::Facts::read`) | Blocks, items, components, loot, tags as content mods |
| Item clicks that run mod code | `B/ItemCode.java` `use`, `useOn`, `entity`, `mine`; op `items` (`GameBridge.items`) | `Host::item_code_runs`, `item_entity_code`, `item_mine_code` | Java runs a click only for code the mod overrides |
| Block state table | `GameBridge.states` | `G/java_mods.rs` `java_bind_states` → `bridge::STATES` (`StateMap`: `to_horizon`, `to_java`, `owned`, `entity`, `animate`, `animate_whole`) | Every `State` argument, `java_block`, `java_animate` |
| Which `animateTick` Java runs | `B/Animate.java` `route`, `ownBlockCode`, `matches`; `T/animate_closure.py` → `J/boot/res/animate_closure.tsv` (loop, block, fluid rows read from the vanilla jar); `B/mixin/AnimateTickMixin.java` | `G/render_tick.rs` `tick_particles` (`AnimateContext.java`), `G/java_hooks.rs` `java_animates`, `java_animate_route`, `java_animate_picked`, `java_animate_whole` | Native picks the positions and skips the routed parts; Java runs a mod's own override (bit 1 block, bit 2 fluid), a vanilla class a mod's mixin patches, or vanilla's whole loop when a mixin patches the loop (`wholeLoop`). `T/mod_replaces.py` `animate_classes`, `animate_routing` give the same answer from a report. Fluid drips stay native |
| Which Java sounds are native's | `B/SoundOwners.java` (first non-plumbing stack frame against `J/boot/res/sound_sources.tsv` and the closure's block/fluid rows), `ClientSounds.play`, `queueTicking` | `T/java_sound_sources.py` (call graph of the vanilla jar joined with the port ledger; `tests/test_sound_sources.py`) | The Java client's copy of a sound native plays is dropped before `PlaySoundEvent`; mod frames and unlisted vanilla frames play; `Animate.forwarding()` plays |
| `PlaySoundEvent` for native sounds | `B/NativeSounds.java` (`decide` reads the bus's listener list at install; ops `client_sound_listened`, `client_play_sound`) | `G/java_mods.rs` `java_ask_play_sound_listeners` sets `Audio::set_play_hook`; `R/sound_engine.rs` `play_inner` asks right before it plays (`PlayVerdict` cancel or change); `external` sounds are not asked again | Synchronous, window thread, only when a loaded mod listens; static view of who listens is `mod_replaces` `events` |
| Baked models of code-built blocks | `B/ClientResources.java` `take`, `baked`; `mods/horizon/src/facts-shared-client/java/dev/horizonlab/facts/BakedWriter.java` | `Host::baked` file → `carry_boot` (`horizon:baked` models) | These blocks draw from the first frame |
| Client resources | `ClientResources.export` (states, code-drawn items, decorated items, atlases and their sprites) | `G/java_models.rs` `parse_resources`, `client_resources` | The GUI atlas gets mods' sprites; `java_client_resources` |
| Key mappings | `B/ModKeyMappings.java` `export`; `B/mixin/KeyMappingsMixin.java` | `Host::keys` file → `java_content::load_keys` → `horizon-client-input/src/keybinds.rs` `ModKeys::parse_export`, `set_mod_keys` | Controls screen lists mods' keys before any world |
| Creative tabs | `B/CreativeTabs.java` (at the client's login, not boot) | `G/java_tabs.rs` | Native creative screen |
| Entity facts (`entities.bin`, `entity-types.bin`) | `B/EntityFacts.java` `confirm` (in `Boot.main` after `seal`; ops `entity_facts`, `entity_types`) reads the file the `analysis` line names (system property `horizon.java.analysis`) and checks it against the bus and the merged classes | `H/entity_facts.rs` `EntityFacts` (`method`, `event`, `route`, `admits`, `reads`, `caps`), set by `boot()`; `Host::entity_facts` file; `Host::entity_types` at the first server start (`G/java_content.rs` `entity_types`, from `java_mods.rs` `java_world_phase`) | Which entity methods route to Java, which events fire for which entity types, and which state groups views carry. A pack with a coremod or class transformer is refused: the boot throws and names the mod |

- The cache: `G/java_content.rs`. `fingerprint_of` hashes every mod jar (name, size, sha256), `horizon-java.args`, each
  file under `config/`, the game's tables (`items.json`, `facts.json`, `block-behaviours.json`,
  `gameplay-facts.json`) and `GENERATION` (bump it when carry writes something else from the same facts). Folder:
  `.horizon-cache/java-mods/<fingerprint>/` (`done`, `facts.sha256`, `keys.tsv`, `entities.bin`, `entity-types.bin`, `mods/`). Older folders are deleted. The fingerprint also hashes the `analysis` file named in `horizon-java.args`.
- A cache hit loads at once and `verify` runs behind it; a different facts hash warns loudly and writes `<key>.next`
  for the next launch (`content`). A failed boot refuses to open a world (`game.rs` calls `horizon_java_host::ready()`).
- Ownership set: `BootFacts.seal` keeps the entries added or replaced after vanilla's bootstrap (and client id-mapper
  entries vanilla's client bootstrap didn't make). Java asks `BootFacts.owns*`; Rust gets it as the `states` and
  `items` lists above.
- Boot order in `Boot.main`: `Bootstrap.bootStrap`, `BootFacts.snapshot`, `ClientLoader.load` (client) or
  `ServerModLoader.load` (server), load errors fail the boot, `BootFacts.seal`, handlers installed
  (`Host.entry = Boot::op`, `GameBridge.register()` fills the op table, `Host.dispatcher = Crossing.DISPATCHER`).
- The cache folder is under the process's working directory (`cache_dir`).

## 3. Server bridges

### Entry and views

| Piece | File | Key names |
|---|---|---|
| Dispatch | `B/Crossing.java`, `B/GameBridge.java` | `Crossing.dispatch`, `In`, `Out`, `Frame`; `GameBridge.register` (one `Crossing.game`/`free` per op), `locked` (lock), `game` (`depth`, `abandonCall`); `ClientBridge.OPS` registers the `client_*` ops |
| Views | `GameBridge.java`, `B/ServerView.java`, `J/plugin/.../ViewInits.java`, `B/Inits.java` | `level`, `player`, `ServerView.create`, `openWorld`; a view skips its constructor and mods' constructor additions run via `Inits` |
| Player view fill | `GameBridge.fill(input, refresh)` | Reads pose, position, mode, hand, slot, xp, `PlayerSync.fill`, motion, inventory; fires `PlayerLoggedIn` on first sight; `refresh` leaves xp and stacks alone |
| Player state | `B/PlayerSync.java`, `B/mixin/FoodSyncMixin`, `PlayerSyncMixin`, `G/java_bridges/player.rs` | `acting`, `fill`, `settle`, `sendAttributes`; setters write through to Horizon's player |
| Held use | `B/Using.java` | A view keeps no use between calls: `clear`, `resume`, `tick` |
| Entities | `B/EntityViews.java`, `B/Backed.java`, `B/mixin/EntityBackedMixin.java` and the `*Access` mixins, `G/java_entities.rs`, `G/java_bridges/entities.rs`, `G/entity_move.rs` (`Entity#move`: `KindMoveWorld` over `MoveState`, one `Game::entity_move` arm per kind) | One live view per native entity from `level_add` to `level_remove` (`entity_view`, `entity_left`), refreshed raw by group bytes (`write_groups` / `EntityViews.apply`), written through by the bridges (guard `@...EntityViews.backed`, receiver `Ent`); Java-owned keys asked at native saves (`entities_save`); Java spawns reach native at once (`EntityViews.spawn`) |
| Block entities | `B/BlockEntities.java`, `G/java_bridges/block_entities.rs`, `G/java_mods.rs` | `ours`, `join`, `loadFresh`, `blockChanged`, `drop`, `markUnsaved`; Rust: `java_block_entity_tick`, `java_send_block_entity`, `java_push_stale`, `java_flush_block_entities` |
| Chunks | `B/ChunkViews.java`, `B/ChunkEvents.java`, `G/java_chunks.rs` | Event kinds `DATA_LOAD`..`UNLOADED` (0 to 6); `java_chunks_read`, `java_chunks_full`, `java_chunks_leaving`, `java_chunks_saving`, `java_level_saved` |
| Tracking | `B/Tracking.java`, `G/java_mods.rs` `java_tracking`, `lan/hub.rs` | Who sees which chunk and entity; fires `ChunkWatchEvent`s |
| Menus | `B/Menus.java` | The player's real server menu, `serverTick`, `login` |
| Level API | `B/LevelApi.java`, `G/java_bridges/level_api.rs` | Results of level bridges |
| Effects | `B/Effects.java`, `GameBridge.writeEffects`, `G/java_mods.rs` `java_apply_effects` | The sounds a call made, in every reply |

### Bridged natives

`H/bridge.rs` and `H/lib.rs`; the Java side is `J/plugin/dev/horizonlab/javamods/plugin/BridgeProcessor.java` (class processor, runs before Mixin).

- `java_bridges!` (macro in `H/bridge.rs`): `"class".method "desc" [if "receiver"] => fn name(game, args) -> ret { body }`.
  It makes the `extern "system"` native and a `Bridge` row; each family's file has `table()`, joined in
  `G/java_bridges/mod.rs` `bridges()`. Families: `block_entities`, `client_world`, `effects`, `entities`, `level_api`,
  `player`, `rules`, `server`, `values`, `world`.
- `Host.publish` sends the table before FML loads a class; `BridgeProcessor` gives each listed method a first step
  that jumps to its native (`Host.link`, `bridge::n_link`).
- Arguments: `Arg` types in `H/bridge.rs` (`Pos`, `State`, `Dir`, `Text`, `Bytes`, `Point`, `Bounds`, `Doubles`,
  `Longs`, `Shape`, `Skip`); results: `Ret`. The Java side is `B/Flat.java`: `raw(T)` flattens an argument, `to<T>`
  rebuilds a result. `java_mods.py check` verifies both ends against the real jars.
- `bridge::with_active(game, f)` makes `&mut Game` reachable from natives during one `Host::request`
  (`G/java_mods.rs` `java_request`); `after_native` registers a hook after each native (`java_push_stale`).
- A bridge that changes what a getter reads must refill the view (`java_refresh_player`), or Java sees the old value.

### Firing events

- Rust queues what happened; it never calls Java from inside a level cascade (`GameLevel` holds the world).
  `G/java_mods.rs` `note_block_changed`, `note_neighbor_changed`, `note_block_tick` push to `BLOCK_EVENTS`;
  `java_drain_block_events` sends them after the cascade as op `block_events`; `GameBridge.blockEvents` runs
  `BlockEntities.blockChanged`, `handleNeighborChanged`, `tick`, `randomTick`.
- Hook points: `G/java_hooks.rs` (`java_hook_*`, each a `cfg(java-mods)`/`cfg(not)` pair) are called from
  `G/native_hooks.rs` (world, server tick, break, attack, block entities) and `game.rs` (clicks, mining,
  `java_level_tick`, saves).
- Entity loops: the 14 native loops that take an entity vector out of `Game` cross between two entities with
  `G/crossing.rs` `cross_point(CrossPoint, &mut [Lent])`, which puts the vectors back in `Game` (`lend`) for the hook.
  Fixed bound, removal marks (`removed`) and fresh state after a crossing: `Touched`, `Marks`, `mob_shared`/`mob_world`.
  The hook does nothing yet outside `crossing_tests.rs`.
- Acting-player ops go through `java_call_use` (one request shape: `java_use_request` header plus a tail) into
  `GameBridge.use(input, kind)`; kinds: 0 `use_item`, 1 `use_item_on`, 2 `click_block`, 3 `event_click_block`,
  4 `event_use_item`, 5 `event_break`, 6 `event_attack`, 7 `player_tick`, 8 `use_tick`, 9 `release_using`,
  10 `stop_using`, 11 `interact_entity`, 12 `mine_block`, 13 `destroy_block`.
  Level ops go through `java_level_call(op, body)` (dimension id and game time first).
- In Java, `GameBridge.safely(what, step)` wraps every NeoForge hook call: it catches, prints
  `Horizon: a Java mod threw during <what>` and records a `Profile.event`. Use it for each new event so one mod's
  exception doesn't end the call.

## 4. Client

### The Java client

| Piece | File | Key names |
|---|---|---|
| Entry | `B/ClientBridge.java` | `install`, `call` (to the Render thread), `run` (`case "client_*"`), `tick`, `frame`, `hudFrame`, `input`, `events`, `click`, `scroll`, `status` |
| Login and packets | `B/Clients.java`, `B/RemoteClient.java`, `B/Wire.java`, `B/ClientLoader.java` | `Clients.toServer`, `joining`, `blockChanged`, `sendShapes`; `RemoteClient.connect`, `take`; `Wire.toServer` |
| Resource reload | `B/ClientLoader.java`, `B/ClientResources.java`, `B/Sprites.java` | Textures without a GPU: atlases stand for their texture by id; `Sprites.locate`, `file` |
| Client level | `B/ClientWorld.java`, `G/java_bridges/client_world.rs`, `G/java_screens.rs` `java_publish_client_world` | The level's chunks are the window's own columns |
| Rust driver | `G/java_screens.rs` (`live` module) | `java_client_tick_begin` (collect, start of the window's tick) and `java_client_tick_send` (the batch, end of `fn tick`'s remote branch, so the tick's picks and movement are this tick's), `java_client_frame`, `java_client_apply`, `java_client_left`, `Worker`, `Job`, `Done`, `CLIENT` thread-local |
| Boot-time hooks | `G/java_components.rs` `install`, `java_client_login`, `java_link` | `net::on_local_login`, `net::on_java_link` |

### Input, clicks, key bindings

- Window events: `G/java_screens.rs` `java_window_input`, `java_key_input`, `java_queue` build `Input` entries
  (`Input::write`); the tick sends them; `ClientBridge.events` replays them through `KeyboardHandler.keyPress`,
  `charTyped`, `MouseHandler.onButton`, `onMove`, `onScroll`.
- Clicks: `java_click_canceled` (op `client_click`, `ClientBridge.click`) runs the real `startAttack`,
  `startUseItem`, `pickBlockOrEntity` before the window acts; `java_block_prediction`, `java_client_destroy_progress`.
  `B/mixin/MinecraftActions.java` exposes the private methods.
- Key bindings: `B/ModKeyMappings.java` (`export` at boot, `apply` for rebinds), `KeyMappingsMixin` (snapshot of
  vanilla's keys), Rust `java_apply_mod_keys` (called on `Event::ModKeys` from `G/options.rs`), settings
  in `horizon-client-render/src/options_file.rs` (`mod_keys`) and `settings.rs`, UI in `options_ui.rs`.
  `InputConstants.isKeyDown` answers from the window's held keys (`B/mixin/KeysMixin.java`, `ClientBridge.keyDown`).

### Screens and the recorder

- `B/Recorder.java` extends `GuiRenderState`: the real `GuiGraphicsExtractor` draws into it and it keeps the pieces
  in call order. Rust: `java_screens.rs` `paint_frame`, `quad`, `text`, `java_screen_item`; drawn by
  `draw_java_screen`, `draw_java_hud`.
- Entry kinds (`Recorder` `QUAD`..`BLUR`, Rust consts of the same names): `0 QUAD`, `1 TEXT`, `2 ITEM`, `3 BLUR`.
  Anything else, or a short read, logs "a Java frame ended in pieces" and drops the rest.
- QUAD: the `wrap` byte is 1 only for a four-corner quad of a plain texture file (not a stitched-atlas sprite)
  that the pipeline repeats; Rust then splits it per tile (`repeat_cells`). The `kind` byte before it: 0 translucent,
  1 invert, 2 additive (`Recorder.blend`; other blends draw as translucent with a one-time warning). Triangles, strips
  and fans are sent as quads whose last corner repeats (`Recorder.primitives`); Rust draws them once.
- A stitched atlas's sprite is found by the primitive's UV middle (`Sprites.locate`) and sent as its file; Rust looks
  the file up in the GUI atlas (`atlas_key`) and warns once per missing texture.
- `ClientBridge.record` records mods' HUD layers; vanilla layers still run into a discarding target
  (`B/HudLayers.java`, `B/mixin/HudLayersMixin.java`) so layout bookkeeping matches.

### HUD timing

- The timer advances as `Minecraft.runTick` does: `ClientBridge.advance`, `timer`, `DeltaTrackerAccess`.
- `ClientBridge.noted` decides "animating": a recording differing from the last starts it; two alike end it only if
  real time passed (`B/HudAnimation.java` `ends`). Status carries `animating`.
- While animating, the window asks for one recording per frame (`client_hud_frame`, `ClientBridge.hudFrame`;
  Rust `java_hud_frame`, `hud_frame_wanted`, `java_hud_frame_apply`); a frame that took a tick's status or while a
  tick runs asks for none.

### The world recorder

- What mods draw in the level's frame (`B/WorldFrame.java`): the real client's events (`ExtractLevelRenderStateEvent`,
  `SubmitCustomGeometryEvent`, `PrepareRenderBuffersEvent`, the `RenderLevelStageEvent` stages, `OitTranslucent` with improved
  transparency) fire in `fire` against vanilla's state objects, filled from the window's camera (`CAMERA` input event: position,
  yaw, pitch, fov, far; `B/mixin/CameraAccess.java`). `FrameGraphSetupEvent` is not fired (it needs a frame graph and targets).
- Runs only if a loaded mod listens to one of those events: `WorldFrame.decide()` reads the bus's listener lists once, at the
  end of `ClientBridge.install`; with none, `take` returns at once and nothing is recorded or sent.
- Two capture points: submitted geometry (`B/SubmitCapture.java`, shared with `BlockRenders`; world mode keeps the pose
  applied, lines, shape outlines, strips) and draws on a recording GPU (`B/RecordingDevice.java`, `RecordingBuffer`,
  `RecordingEncoder`, `RecordingRenderPass`, `RecordingPipeline`, `RecordingTransform`) that `B/mixin/RenderSystemMixin.java` and
  `DynamicGpuDataMixin.java` hand out only while `WorldFrame.recording`. A texture view is already `Sprites.View` (its id).
- A pipeline's own state picks the native pass (`WorldFrame.classify`); meshes are keyed by content or by buffer id and version
  and sent once; pieces place them (`origin + linear * local`, world space, quantized).
- Timing as the HUD: `ClientBridge.tick` takes a recording after the HUD's (`WorldFrame.take`); status carries
  `bool world` (+ blob) and `bool worldAnimating`; while animating the window asks `client_world_frame`
  (`G/java_world_renders.rs` `java_world_frame`, gated like `hud_frame_wanted`). Rust: `R/world_renders.rs` (`apply`, `draw`),
  drawn by `game.rs` beside `dynamic_renders::draw`; quads after translucent terrain use `gfx::MOD_AFTER_TRANSLUCENT_OVERLAY` (36).

### Models, renders, items, text, sounds

| Feature | Java | Rust | Draws |
|---|---|---|---|
| Geometry of code-built models (cables, cells) | `B/ClientModels.java` `take`, `dynamic`, `dirty`, `pending`; `B/mixin/ModelManagerMixin.java` | `G/java_models.rs` `java_models_apply`, `parse_models`; `R/dynamic_models.rs` | `R/mesher.rs` (~line 901) meshes it as any model |
| Block entity renderers | `B/BlockRenders.java` `take`, `probe`, `samples`, `has`; `B/mixin/PoseMixin.java` | `java_renders_apply`, `parse_renders`; `R/dynamic_renders.rs` `set`, `Frame`, `draw`, `items` | `game.rs` (~line 9352) draws every frame from the recording (motion as `Param`: const, linear, sine) |
| Items only mod code can draw | `BlockRenders.items`, `submitItem`; `ClientModels.foreignItems` | `java_items_request`, `java_items_apply`, `java_item_state` (called from `G/item_draw.rs`) | `horizon_client::item_render` |
| Item decorations (counts, bars) | `BlockRenders.decorations` | `java_decorations_request`, `java_decorations_apply`, `draw_java_item_decorations` | HUD batch |
| Names and tooltips | `B/ItemText.java` `take` | `G/java_item_text.rs` `request`, `java_item_text`; hook `java_hook_item_text` | Cached per stack; asked with the next tick |
| Outlines and collisions of block-entity blocks | `ClientBridge.shape`, `collisions`, `Clients.sendShapes` | `G/java_shapes.rs` `java_outline`, `java_collision_shape`, `java_client_collisions` | `game.rs` outline path (`java_outline`) |
| Creative tabs | `B/CreativeTabs.java` `take`, `ready` | `G/java_tabs.rs` `parse`; `game.rs` `java_creative_tabs_take` | Native creative screen |
| Sounds | `B/ClientSounds.java`, `B/mixin/SoundEngineMixin.java` | `java_screens.rs` `java_client_sounds`, `java_client_finished_sounds` | `audio.play_external`, `update_external`, `stop_and_release` |
| Mods' item components on the wire | `B/StackComponents.java` | `G/java_components.rs` `Codecs`; `horizon_protocol::item::foreign` | Stacks from servers |

- Geometry and renderer output recorded for a state the window no longer holds is ignored (`recorded_for`,
  `java_models.rs`); changes are applied per tick from the status counts (`status_pending`).
- `R/java_text.rs` is unrelated: Java's `Double.toString` and Gson conventions for options files.

## 5. Wire formats

Big-endian, as Java's `DataOutputStream`. Rust: `H/wire.rs` `Out` (`long`, `int`, `float`, `double`, `bool`, `bytes`,
`text`) and `wire::In` (the same, returning `None` when short). `text` and `bytes` are an `int` length then the
payload. `tag` = NBT written by `GameBridge.writeTag`, read by `java_mods.rs` `read_tag`.

| Name (op) | Writer | Reader | Layout |
|---|---|---|---|
| Item list (`items`) | `GameBridge.items` | `H/lib.rs` `Items::read` | `int n`; n x (`text id`, `bool use`, `bool useOn`, `bool entity`, `bool mine`) |
| State table (`states`) | `GameBridge.states` | `G/java_mods.rs` `java_bind_states` | `int count`, `int airId`, `bool wholeLoop`; count x (`int id`, `text block`, `int n`, n x (`text name`, `text value`), `bool hasBlockEntity`, `bool owned`, `byte animate`) |
| Use request (`use_item` and the other kinds) | `Game::java_use_request` + tail | `GameBridge.fill`, `use` | uuid (2 `long`), `text name`, `text dim`, `long gameTime`, `text pose`, 3 `double` pos, 2 `float` look, `bool sneak`, `int mode`, `int hand`, `int slot`, xp (`int`, `float`, `int`), `PlayerSync.fill` block, 3 `double` motion, `bool onGround`, `int n` x (`int slot`, `bytes tag`), then the kind's tail |
| Use reply | `GameBridge.use` end | `java_apply_use` | `bool handled`, `bool using`, `int ticks`, xp (`int`, `float`, `int`), `int n` x (`int slot`, tag or 0), effects |
| Effects | `GameBridge.writeEffects` | `java_apply_effects` | `int n` x (`text sound`, 3 `double`, 2 `float`) |
| Block events (`block_events`) | `java_drain_block_events` | `GameBridge.blockEvents` | `text dim`, `long time`, `int n` x (`int kind`, `long pos`, `int a`, `int b`, `int c`); kind 0 changed (old, new, flags), 1 neighbour (state, moved, changed), 2 tick, 3 random tick |
| Chunk events (`chunk_events`) | `java_chunks.rs` `java_chunk_events` | `GameBridge.chunkEvents`, `B/ChunkEvents.java` | Event kind (`DATA_LOAD`..`UNLOADED`) and the chunk's full tag; layouts in `java_chunks.rs` and its test `the_full_chunk_tag_survives_the_wire` |
| Client tick (`client_tick`) | `java_screens.rs` `java_client_tick_send` | `ClientBridge.tick` | `int ticks`, `float partial`, `bool` size changed (+3 `int`), `int n` + `Input` entries (kind 14 `Animate`: `int n` x 3 `int` picked positions, one per window tick owed; 15 `Particles`: `int` option), `bool moved` (+ pos, look, velocity, `bool onGround`), `int n` x `int soundHandle` finished |
| Client status (tick reply) | `ClientBridge.status` | `java_client_status`, `status_pending` | `bool screen`, `int opened`, `bool pause`, `int pendingModels`, `int pendingRenders`, `int n` x (`int x`, `int z`) told, sound events (`int n` + bytes), particles (`int n` x 6 `double` place and speed, `bytes` the type as `ClientboundLevelParticlesPacket` writes it), `int shapes`, `bool hud` (+ `int len` + frame), `bool animating`, `bool world` (+ `int len` + blob), `bool worldAnimating`, `bool tabsReady` |
| World recording (inside status, `client_world_frame`) | `WorldFrame.blob` | `world_renders::apply` | `int generation`; `int n` x `long` dropped mesh; `int n` x mesh (`long id`, `int quads` x (`text texture`, 20 `float` xyzuv, 4 `int` argb, 4 `int` light, `bool` + 3 `float` normal), `int lines` x (6 `float`, `int argb`, `float width`)); `int n` x piece (`long mesh`, `byte pass` 0 opaque, 1 translucent, 2 after translucent, 3 additive, 4 see-through, 5 lines, 6 lines on top; `byte flags` 1 no cull, 2 writes depth; 3 `double` origin, 9 `float` linear, 4 `float` modulator). `client_world_frame` takes size (3 `int`), `float` partial, 3 `double` and 4 `float` camera and replies `bool animating`, `bool changed`, `int len` + blob |
| GUI frame (`client_frame`, HUD frame) | `Recorder.bytes` | `paint_frame` | `int n`; n x entry. QUAD: `byte 0`, `text texture`, `byte kind`, `byte wrap`, 4 x (`float x,y,u,v`, `int color`), scissor. TEXT: `byte 1`, `float x,y,sx,sy`, `int color`, `bool shadow`, `int background`, `int runs` x (`text`, `int rgb` or -1, `byte flags`, `text font`), scissor. ITEM: `byte 2`, 4 `float`, tag, scissor. BLUR: `byte 3`. scissor = `bool` + 4 `int`. `client_hud_frame` replies `bool animating`, `bool changed`, frame |
| Block-entity geometry (`client_models_take`) | `ClientModels.take` | `parse_models` | `int n`; n x (`long pos`, `int state`, `int key`, `bool fresh`, if fresh `int quads` x quad). Quad: 4 x 5 `float` (xyz, uv), `text sprite`, `byte face`, `byte cull` (6 none), `int tint`, `int color`, `bool shade`, `byte emission`, `byte layer` |
| Renderer recordings (`client_renders_take`) | `BlockRenders.take`, `entry` | `parse_renders` | `int n`; n x (`long pos`, `int state`, `int draws` (-1 none), draws x (`bool translucent`, `int ops` x op (`T S R Q M` + params), `int quads` x (`text`, 4 x (5 `float`, `int color`, `int light`)), `bool item` + tag, `byte context`, `int seed`)) |
| Items and decorations | `BlockRenders.items`, `decorations` | `java_items_apply`, `java_decorations_apply` | Keyed replies to the keys `java_items_request` and `java_decorations_request` sent |
| Item text (`client_item_text`) | `ItemText.take` | `java_item_text.rs` `parse` | Request `int n` x (`bytes tag`, `bool advanced`, `bool creative`); reply `int n` x (`bool ok`, `text nameJson`, `int lines` x `text json`) |
| Creative tabs (`client_creative_tabs`) | `CreativeTabs.take` | `java_tabs.rs` `parse` | Strings, indexes and stacks per tab; the test `a_reply_from_the_java_client_reads_back` builds one |
| Client resources (`client_resources`) | `ClientResources.take` | `java_models.rs` `parse_resources` | `int n` x `int state`; `int n` x `text` (code-drawn items); `int n` x `text` (decorated); `int atlases` x (`text id`, `text texture`, `int w`, `int h`, `int n` x `text sprite file`) |
| Sound events (inside status) | `ClientSounds.take` | `java_client_sounds` | `int n` x (`byte kind` 0 play / 1 update / 2 stop, `int handle`, then play: `text event`, `int source`, 2 `float`, 3 `double`, `bool loop`, `int delay`, `bool linear`, `bool relative`, `bool silentStart`, `bool tickable`; update: 2 `float`, 3 `double`) |
| Boot facts (file) | `BootFacts.export` | `oracle::Facts::read` | JSON, `schema` 3 |
| Baked models (file) | `ClientResources.baked` | `oracle::BakedModels::read` | JSON |
| Key list (file, `client_keys`) | `ModKeyMappings.export` | `ModKeys::parse_export` | A line per key, tab separated: name, `1` if vanilla's, category, default key, default modifier, conflict context |
| Key rebinds (`client_keys_apply`) | `java_apply_mod_keys` | `ModKeyMappings.apply` | `int n` x (`text name`, `text key`, `text modifier`) |
| Java frames (`horizon:java`) | `B/Wire.java`, `RemoteClient.take` | `java_wire_flush`, `java_remote_in`, `java_components.rs` `java_link` | Packets of the Java protocols in the channel `horizon_protocol::java::CHANNEL`; the server probes a client in configuration (`lan/conn.rs`) |
| Component codecs (`component_read`, `component_write`) | `B/StackComponents.java` | `java_components.rs` `Codecs` | A mod type's value as its own stream codec writes it |

## 6. How to

Names below are real; copy the nearest existing one. Commit `0ebe3cd78` ("vanilla's break sequence") shows most of
these end to end (`git show --stat 0ebe3cd78`).

### Add a new event firing (a NeoForge event for something Horizon does)

1. Find the native site. Add `java_hook_<thing>` to `G/java_hooks.rs` as a `cfg(java-mods)` / `cfg(not)` pair (model:
   `java_hook_mine_block`) and call it from the site (`native_hooks.rs`, `game.rs`).
2. Implement `Game::java_<thing>` in `G/java_mods.rs`. Guard with `horizon_java_host::get()`. For a player's action
   build the tail with `Out` and call `java_call_use(what, op, tail, &mut claimed)` (model: `java_mine_block`,
   `java_attack_event`); for a level event call `java_level_call(op, |out| ...)`.
3. Java: add `Crossing.game("<op>", ...)` in `GameBridge.register` (a `byte[] -> byte[]` method, or `(In, Out)` to read and write the buffer directly). For an acting-player op add a `kind` case in `GameBridge.use`
   and a `case` mapping the op to it. Wrap the hook call in `safely("<Event>", ...)`.
4. Reply: return `effects()` (level ops) or fall through `use`'s tail; a cancel is the `handled` bool, which Rust reads as
   `claimed` (model: `event_break`, `CommonHooks.fireBlockBreak`).
   A request over 64 KB or a reply over its arena grows it once; a reply over 16 MB always takes `Host.overflow()`.
5. Test: observe it from the test mod (`Checks.check` in a `Checks*` class, section 6 below) and run
   `java_mods.py bridges`.

### Add a new boot fact

1. Decide where it comes from: the facts JSON (`BootFacts.export`, `FactsWriter`, read by `oracle::Facts` in
   `horizon-port-pack`), a file the host keeps (`Host::keys`, `Host::baked`), or a request at boot (`Host::request`
   in `boot()` in `H/lib.rs`, like `client_resources`).
2. Write it in Java at the end of the boot: a field in `BootFacts` or `ClientResources.take` (the reload has run by then).
3. Read it in Rust once, in `boot()` or `java_content.rs` (`generate`, `load_keys`), and keep it in the `Host` or a
   `OnceLock`. Never ask Java for it again at runtime.
4. If it changes content, bump `GENERATION` in `G/java_content.rs`; if it is a new input, add it to `fingerprint_of`.
5. Check `cargo test -p horizon-client --features java-mods --bins java_content` (the fingerprint and cache tests).

### Add a new client recording (something the Java client draws that Rust paints)

1. Java: a recorder entry kind in `B/Recorder.java` (a new constant beside `QUAD`..`BLUR`, a writer that does `count++`),
   or a new op beside `client_models_take`; add a `case "client_..."` in `ClientBridge.run`.
2. If it needs a trigger, count it in `ClientBridge.status` (counts lead the reply, so Rust reads them without
   parsing the rest) and add the take to `Job::run` in `java_screens.rs`.
3. Rust: add the entry to `paint_frame`'s match (unknown bytes end the frame) or a `parse_<thing>` and a
   `java_<thing>_apply` in `G/java_models.rs`; store it in an `R/` registry (`dynamic_models`, `dynamic_renders`).
4. Draw from the registry every frame; never call Java on the render path.
5. Test: a Rust test that feeds bytes to the parser (model: `a_recorded_hud_paints_in_call_order_and_stops_at_a_broken_entry`
   in `java_screens.rs`, `the_boot_export_reads_back_and_names_the_sprites_the_window_lacks` in `java_models.rs`);
   a Java unit check if the writer has logic (below).

### Add a Java unit check

1. File: `J/unit/dev/horizonlab/javamods/<package>/<Name>Check.java`, a class with `main` that throws `AssertionError`
   and prints `<Name> ok`. Models: `RecorderPrimitivesCheck`, `HudAnimationCheck`, `ItemCodeCheck`.
2. Use the package of the class under test if it is package-private (`unit/.../boot/` for `Recorder`).
3. Make the logic under test a static, GPU-free method (`Recorder.primitives`, `HudAnimation.ends`): `unit` never boots.
4. Run `python3 tools/java_mods.py build && python3 tools/java_mods.py unit`.

### Add a test-mod check (a bridge or event seen from a real mod)

1. Pick the family class in `J/testmod/.../` (`ChecksRules`, `ChecksLevelApi`, `ChecksEffects`, `ChecksPlayer`,
   `ChecksValues`, `ChecksEntities`, `ChecksBlockEntities`, `ChecksServer`, `ChecksChunks`); a new family goes in
   `Checks.runAll`'s `families` array (each gets its own area, `base + 8 + i*4`).
2. Add `Checks.check("name", () -> ...)` in its `run(context, area)`; it prints `CHECK name ok|FAILED|THREW`.
3. For new blocks or items add their json under `J/testmod/content/` (`assets/` and `data/horizon_java_test/horizon/`)
   and register them in `TestMod.java` (`ITEMS.registerItem("wand", Wand::new)` style).
4. Client-side checks (keys) live in `ClientKeys.java`; `java_mods.py bridges` requires the names listed in its
   `wrong.extend(...)` line (`key_quiet_under_a_native_screen`, `key_fires_without_a_screen`, `mod_key_follows_the_rebind`).
5. Run `java_mods.py build` then `java_mods.py bridges`; `bridges` reads `CHECK` lines from `ctl.log`.

### Add a bridged native (serve a vanilla method from Rust)

1. Pick the family file in `G/java_bridges/` (or add one: a `mod` line and a `table()` in `mod.rs` `bridges()`).
2. Add the entry inside its `java_bridges! { table for Game<'static> { ... } }` (model: `player_teleport_to` in
   `player.rs`): class, method, JVM descriptor, optional `if "Receiver"`, then `fn name(game, args) -> ret`. The native's
   name is unique across all families.
3. If an argument or result type has no `Arg` or `Ret`, add one in `H/bridge.rs` and a matching `Flat.raw(T)` or
   `Flat.to<T>` in `B/Flat.java`.
4. If the body changes what a getter reads, refill the view (`game.java_refresh_player()`); if it changes the
   level, return through `game.after_java_change(out)`.
5. Add a check in the test mod (above); run `java_mods.py check` (static, no game) then `java_mods.py bridges`.
   A method deliberately not served goes in `T/java_mods_skips/*.tsv` with a reason (`java-object`, `java-body`, `slower`).

## 7. Tests and tools

All from the repo root, inside `nix-shell -p jdk25 pkg-config alsa-lib udev` (see java-mods.md "Run it").

| Command | What it does |
|---|---|
| `python3 tools/java_mods.py build [--java-only]` | javac `host`, `plugin`, `boot` (plus `facts-shared*`), `testmod` into `.local/java-mods/` jars; then cargo builds `horizon-java-host`. Takes `build.lock` |
| `python3 tools/java_mods.py crossing [--jars DIR] [--rounds 5] [--n 200000] [--soak 3]` | `H/examples/crossing_bench.rs` through the real `Host::request`, dedicated dist, no mods: ns per `bench_empty`, `bench_echo`, `bench_nested`, a 1 MB reply, the reentrancy and overflow checks and a four-thread soak. Fails above 1,000 ns for an empty request |
| `python3 tools/java_mods.py unit` | Compiles `J/unit/**` and runs each `*Check` class on the built boot classes; no boot, no game |
| `python3 tools/java_mods.py check [--analysis INSTALL/analysis/entity-facts.json]` | Static: every bridge entry against the real jars (method exists, `Arg`/`Ret` line up, `Flat` has the overload); with `--analysis`, every entity writer mods call must be bridged or `java-owned` in `T/java_mods_skips` |
| `python3 tools/java_mods.py bridges [--extra-jars DIR] [--scratch DIR]` | Live: the test mod's wand on a real click, the machine, a chunk unload; prints `CHECK` results; two sessions |
| `python3 tools/java_mods.py install --pack MODS --out INSTALL` | A player install (JDK, libraries, jars, asset index); idempotent |
| `python3 tools/java_mods.py launch --install INSTALL [--dedicated] ...` | Starts the game (or the server) from an install; scratch homes |
| `python3 tools/java_mods.py mixins --install INSTALL --out OUT` | The static list of every mixin handler of the install's mods (`mod_parity.scan_install`) |
| `python3 tools/java_mods.py run \| play \| libraries \| surface \| fetch \| bench` | Boot only; scripted `horizon-ctl` session; library mods; what jars call; Modrinth; benchmark |
| `python3 tools/mod_parity.py mixins --install DIR --out OUT` | Same static mixin list |
| `python3 tools/mod_parity.py run --install DIR --scenario basic --out OUT` | Real client, then Horizon, same scripted steps (`T/parity_scenarios/basic.json`), then the comparison; builds `J/parity/{lib,mod}`; uses the agents' gate |
| `python3 tools/mod_parity.py compare --forge F --horizon H --mixins M --out OUT` | Compares two runs' JFR event counts and mixin handler counts per step |
| `python3 tools/mod_replaces.py --install DIR --out OUT [--mixins M]` | `replaces.json` and `entity-facts.json` (guards per entity event, state groups and writers per entity class, edge and body routes, overrides, capabilities, opaque jars); exits non-zero listing entity methods `GROUPS` in `T/mod_entity_facts.py` doesn't name |
| `python3 -m unittest tests.test_mod_entity_facts tests.test_mod_replaces tests.test_mod_parity tests.test_java_mods_install tests.test_java_mods_assets` | Python unit tests of the tools (`test_mod_parity` also compiles `MixinScannerCheck`) |
| `python3 tools/profile_report.py DIR [--top N] [--no-jfr]` | Ranks a `HORIZON_PROFILE` folder: `server-ticks.log`, `java-ticks.log`, JFR |
| `cargo test -p horizon-java-host` / `cargo test -p horizon-client --features java-mods --bins java_` | Rust tests of the host and the `java_*` modules (they live in the game binary and only compile with the feature) |

Environment:

- `HORIZON_JAVA_RUN`: the run folder with `horizon-java.args`; unset means no JVM. `HORIZON_JAVA_HOST_JAR`, `HORIZON_JAVA_OPTS`
  (appended to the JVM options, so `-Xmx` or `-Dhorizon.java.shapes=off|rust|shadow` go here), `HORIZON_JAVA_DIST=server`
  (`java_mods.py`, dedicated side), `HORIZON_JAVA_MODS_DIR` (build and read jars from another folder).
- `HORIZON_JAVA_DEBUG`: extra logs. Rust: HUD frame statistics (`java_hud_frame_apply`), every click's wait
  (`java_click_canceled`). Java: `ClientBridge` HUD debug, `Wire` per-packet lines, block entity loads, `use` results.
- `HORIZON_PROFILE=DIR`: `H/profile.rs`. Starts JFR (`exit.jfr` on JVM exit), `G/profile.rs` `profile_tick` writes
  `server-ticks.log` and `java-ticks.log` each second (`Host::request` times per op, `Profile.take`), console
  `profile dump` writes `dump-<ms>.jfr`. Since the arena crossing a request costs about 1 µs less than before, so
  per-op means drop by that much.

Which check covers what:

| Change | Check |
|---|---|
| A bridged native | `java_mods.py check`, then a test-mod check and `bridges` |
| An event or hook on the server | `bridges` (wand, machine, chunk unload and reload) |
| Recorder, HUD timing, `ItemCode`, the world recorder | Java unit checks (`java_mods.py unit`: `WorldFrameOrderCheck`, `WorldFrameSubmitCheck`, `WorldFrameListenersCheck`, `RecordingDeviceCheck`) |
| A parser or wire layout | Rust unit tests in the same `java_*.rs` |
| Boot facts, cache | `java_content.rs` tests (`the_fingerprint_follows_jars_configs_and_arguments`, `the_cache_holds_only_finished_content`) |
| Entity facts | `tests/test_mod_entity_facts.py` (guards, groups, routes, caps on javac-built classes), `EntityFactsCheck` (`java_mods.py unit`), `cargo test -p horizon-java-host entity_facts` |
| Mixins and events against stock | `mod_parity.py run` |
| Anything visual or with real input | A person, in a window |

## 8. Gotchas

- **One owner per effect.** Java runs only what a mod overrides; Rust keeps the rest. Examples:
  `ClientBridge.sentByWindow` drops the Java client's own movement, use and attack packets because Horizon's client
  already tells the server; `Host::item_code_runs` and `ItemCode` pick Java per click; `java_destroy_block` passes
  `own_mine` so the tool isn't worn twice; `StateMap.owned` (`java_block`) gates block behaviour to mods' blocks;
  `HudLayers.discarding` lets vanilla's layers run but keeps no pixels (native draws them). Doing an effect on both
  sides doubles it.
- **No polling, no mirroring.** The Java client learns only from the server's packets, in vanilla's order
  (`Clients`, `Tracking`, `ChunkMapMixin`; commit `d74638b0f`). Never copy server objects into client ones or
  re-check on a timer. `hud_frame_wanted` is vanilla's per-frame HUD recording, not a poll.
- **Boot data goes into native.** What the boot loads is exported once and read in Rust (section 2); never fetch it
  from Java at runtime (`ClientResources`: "Java isn't asked for it again"; `java_tabs.rs`: "the screen itself never
  calls Java").
- **Never call Java on a render path.** Frames draw from recordings (`dynamic_renders::draw`, `paint_frame`). Ticks run
  on the `Java client` worker so the window doesn't wait (`java_client_running`, `java_client_busy`). Names and tooltips
  are asked once with the next tick's batch and cached (`java_item_text.rs`). Only clicks and an open screen wait on
  Java, and `java_click_canceled` logs waits over 5 ms.
- **Clean room.** Never read mod or NeoForge source; `javap`, traces and JFR are fine; vanilla's decompiled source is
  fine. No code keyed on a mod id or class: fix the shared layer. Ownership is by registry entry, never a namespace
  (`BootFacts`; java-mods.md "What a mod owns").
- **An empty reply means failure.** `Host::request` returns `None` when Java threw or the reply is empty
  (`Host::cross`: a return of 0, -1 or a pending exception). An op with nothing to say must return a byte (`GameBridge.refresh` returns `{1}`).
- **Calls don't nest from Rust.** A level hook runs inside `with_level`, so Java can't call back into the world
  there; queue and drain after (`BLOCK_EVENTS`, `push_event`). Inside Java, nested calls track `depth`: effects are
  written only at depth 1, and a throw at depth 1 abandons the call (`abandonCall`).
- **A view's getters read Java copies.** Refill after a bridge changes them (`java_refresh_player`, `put_player` in
  `player.rs`). A mod's redirect inside a bridged method never runs: its Java body doesn't.
- **Views skip constructors.** Mods' constructor injections and field initializers run later via `ViewInits` and
  `Inits`; an injection capturing a local gets a default.
- **Mixins.** `defaultRequire: 1` in both configs: an injector that doesn't match fails the boot. List a new mixin in
  `J/boot/res/horizon_java.mixins.json` (common and server) or `horizon_java.client.mixins.json` (`client` array).
  `MapperBootstrapMixin` uses `require = 0` on purpose.
- **Thread-locals.** `JAVA_SERVER`, `BLOCK_EVENTS`, `WIRE_OUT` (`java_mods.rs`) belong to the thread that runs the
  server; `CLIENT` and `WORKER` (`java_screens.rs`) to the window's. A value set on one is invisible on the other.
- **`unit` and `HORIZON_JAVA_MODS_DIR`.** `java_mods.py unit` reads `LOCAL/classes/boot`, while `build` writes to the
  `HORIZON_JAVA_MODS_DIR` folder when it is set; set the variable the same way for both, or the check runs old classes.
- **Every cfg site has two arms.** A new `java_hook_*` needs its `cfg(not(feature = "java-mods"))` arm, or the default
  build breaks; `java_screens.rs` ends with the stubs for the same reason.
- **Commits.** `Java mods:` prefix, body of short bullets.

Update this map in the same commit as any change that moves or renames what it lists.
