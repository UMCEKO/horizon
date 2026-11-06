# Java mods in an embedded JVM (experimental)

An optional path beside [native mods](native-mods.md) and [ported content](modpacks.md): real NeoForge 26.3 mod jars run
unchanged on real NeoForge and Minecraft classes, in a JVM inside the game's process. It is off unless the game is built
with `--features java-mods` and started with `HORIZON_JAVA_RUN` set.

Horizon stays the game. World, ticks, entities, rendering and saves are Rust; Java runs mod code and the objects mod code
touches. Anything Horizon has already ported is served from Rust, except getters and tiny methods, which are faster left in
Java.

## Serving a method from Rust

One entry in a family's `table()` under `crates/horizon-client/src/game/java_bridges/`, and nothing in Java:

```rust
horizon_java_host::java_bridges! { table for Game<'static> {
    "net/minecraft/world/level/Level".setBlock
    "(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/state/BlockState;II)Z" if "net/minecraft/server/level/ServerLevel" =>
    fn level_set_block(game, pos: Pos, state: State, flags: i32, _limit: i32) -> bool {
        // any Rust on &mut Game
    }
}}
```

The macro makes the native and a row of the table the game publishes to the JVM. `BridgeProcessor`, an FML class processor,
reads the table and gives each method a first step that jumps to its native. `if` names the receiver it applies to; a
mod's own `Level` subclass runs the Java body. It runs before Mixin, so a mod's HEAD and RETURN injections still apply. A
mod's redirect inside the Java body doesn't run, since the body doesn't.

A Java object crosses as its `Flat.raw` overload, which an `Arg` type in `bridge.rs` reads back: `BlockPos` as a long,
`BlockState` as an id, `Holder` and `Stat` as strings, `Vec3` as doubles, `Component` as NBT. `Skip` drops an argument the
port doesn't need. A body's result rebuilds through `Flat.to<Type>`. Adding a type is one method on each side.

The port ledger finds candidates, not bindings: some of its matches are narrow (`StateHolder.setValue` points at
`set_honey_level`), so someone reads the Rust before writing the entry. `java_mods.py surface` lists what's left by how many
mods call it, and `java_mods.py bridges` checks the bindings with a real mod's real click.

Getters read the view's Java copy, filled when the game calls in. So a bridge that changes something a getter reads must
refill the view, or Java sees the old value until the next call.

## What exists

| Piece | Where | State |
|---|---|---|
| FML, mixins, registries, mods' own code | `crates/horizon-java-host/src/lib.rs`, `mods/horizon/src/java-mods-host/` | Real classes, started on a background thread; a title screen doesn't wait, a world does |
| Data packs: tags, recipes, data maps, dynamic registries | `GameBridge.prepare` | Real `WorldLoader`, mods' packs included |
| `ServerLevel`, `ServerPlayer`, `MinecraftServer` | `GameBridge`, `ServerView`, `mixin/` | Real classes built without a world, filled from Horizon for each call |
| The client side | `ClientLoader`, `Host.run` | NeoForge's real client mod loading on a `Minecraft` built without a window, so mods with client code load as on a real client. `HORIZON_JAVA_DIST=server` boots the dedicated side instead |
| The client's resource reload | `ClientLoader`, `ClientResources`, `mixin/AtlasMixin`, `game/java_models.rs` | At boot, the constructor's steps in its order: the real pack repository (vanilla, mods' packs, the resource pack folder), every reload listener made and registered as `Minecraft` does, then `createReload` with `Minecraft` as its main executor, so NeoForge's events fire from where a real client fires them (`RegisterTextureAtlasesEvent`, `RegisterParticleProvidersEvent`, `RegisterBlockModelsEvent`, `TextureAtlasStitchedEvent` for each of the atlases, `AddLayers`, `ClientStartedEvent`, `ClientResourceLoadFinishedEvent`). Only GPU work is left out: textures, samplers and animation uploads, shader compilation, the font's glyph textures, the sound device and the GPU warnlist. Its results (code-built block states, items only their mods' code draws, every atlas's sprites) reach Rust once as the boot ends; mip levels are freed after |
| What mods add to constructors | `ViewInits`, `Inits` | A view skips its constructor, so a class processor after Mixin keeps mods' injections and field initializers from it, and they run once the view is filled. An injection capturing a local gets a default for it |
| `Item.use`, `Item.useOn`, clicks on mods' blocks | `game/java_mods.rs`, beside the native hooks, `ItemCode` | The real `ServerPlayerGameMode` and `ItemStack.useOn`, so NeoForge's events fire, for the clicks a mod item's class has code for (`ItemCode`: any of `use`, `finishUsingItem`, `onUseTick`, `releaseUsing`, `getUseDuration`, `getUseAnimation`, `useOnRelease`, `onStopUsing`, `canContinueUsing`, `canEquip`, `getEquipmentSlot` for a click in the air; `useOn`, `onItemUseFirst`, `doesSneakBypassUse` on a block; `interactLivingEntity` on a mob) beyond `Item` and `BlockItem`, which Horizon runs from the components; a block item placing a block a mod owns runs in Java too, as `BlockItem.place` runs that block's code (`getStateForPlacement`, `setPlacedBy`, its block entity's `applyComponentsFromItemStack`). A use Java starts is held by Horizon's player, and each of its ticks, its release and its stop go back to Java with the use put back on the view; the player's own client hears of it from the living flags the server sends it (`LocalPlayer.onSyncedDataUpdated`), so letting go releases it there. Experience and the stacks come back after each call |
| The acting player's food, health, absorption and effects | `PlayerSync`, `mixin/FoodSyncMixin`, `mixin/PlayerSyncMixin`, `game/java_bridges/player.rs` | Filled into the view as Horizon holds them when a call starts; vanilla's own setters and effect callbacks (`FoodData`'s writes, `setHealth`, `setAbsorptionAmount`, `onEffectAdded`, `onEffectUpdated`, `onEffectsRemoved`) write each change through to Horizon's player as mod code makes it, so nothing is copied back. A change Horizon makes during a call (a bridged hurt) fills the view again |
| Mining speed | `GameBridge.destroyProgress`, `ClientBridge.destroyProgress`, `PlayerSync.settle`, `game/hand.rs` | With Java mods, `Game::destroy_progress` is the real `BlockState.getDestroyProgress` on the server's player view, and for the window on the Java client's `LocalPlayer`: the held item's `getDestroySpeed`, NeoForge's `BreakSpeed` and harvest events run. A survival START breaks at once when that progress is complete, `tick_mining` uses the same function, and the break's harvest decision is Java's `hasCorrectToolForDrops` (`correct_tool`). The view settles equipment attribute modifiers and eye fluids first, and sends the client its attributes and effect changes as vanilla's packets |
| `Item.mineBlock` | `ItemCode.MINE`, `GameBridge` `mine_block`, `game/java_mods.rs` | After a native player break removed the block, a held item whose class has its own `mineBlock` runs it in Java (a tool spending energy, breaking more blocks) instead of Horizon's tool wear |
| A player's break of a mod's block | `GameBridge` `destroy_block`, `game/java_mods.rs` | After the break event, NeoForge's `destroyBlock` order from a JFR trace of a mod-free server: `playerWillDestroy`, the harvest check, `mineBlock` (before removal), `onDestroyedByPlayer`, `destroy`, then `playerDestroy` with the block entity as it was. Java owns drops, XP, particles and removal for those blocks; Horizon's tool wear runs only when the item has no `mineBlock` of its own |
| Mods' blocks | `BlockEntities.blockChanged`, `GameBridge.blockEvents` | `LevelChunk.setBlockState`'s steps with the real flags (`UPDATE_NEIGHBORS`, piston, skipped side effects, skipped `onPlace`), the real neighbour in `neighborChanged`, scheduled and random ticks. They run after Horizon's cascade: see Not yet |
| Mods' block entities | `BlockEntities`, `game/java_bridges/block_entities.rs` | Real objects in Java; their saved form lives in Horizon's chunk, and Horizon's list gives them their turn to tick |
| Joining the level | `BlockEntities.join`, `loadFresh` | `setLevel` and `clearRemoved` as `LevelChunk.setBlockEntity` does, then `onLoad` before the next ticker, as `Level.tickBlockEntities` does |
| Chunk load and unload | `game/java_mods.rs` `java_chunks_changed`, `GameBridge.chunksChanged` | A column joining the level has its mods' block entities taken as `LevelChunk.postProcessGeneration` promotes them; one leaving hands back what it hadn't saved, then `onChunkUnloaded` and `clearAllBlockEntities`. Chunks loaded before the server's start catch up after it, as servers' configs load there |
| Chunk events and mods' chunk data | `game/java_chunks.rs`, `ChunkEvents` | As `ChunkMap` fires them, on the chunk's full vanilla tag, the one Horizon reads from and writes to its region files (sections, heightmaps, block entities, attachments): a column read from the save gets `ChunkDataEvent.Load` with its attachments restored, then `ChunkEvent.Load` once its block entities are in (new for a generated one); each chunk Horizon writes gets `ChunkDataEvent.Save`, and what Java's `write` returns is the record saved. A chunk a mod marked unsaved is saved with the changed ones. A leaving column gets `ChunkEvent.Unload` after its save, as `scheduleUnload` does. `LevelEvent.Save` follows a level's chunks, and `ModifyCustomSpawnersEvent` comes before `LevelEvent.Load` with vanilla's spawners |
| Chunks | `ChunkViews`, `LevelChunkMixin`, `ChunkCacheMixin` | `getChunk`, `getChunkNow` and `hasChunk` answer for chunks Horizon has loaded with a `LevelChunk` holding nothing: its blocks and block entities are the level's. `getChunk` with `load` blocks the server thread on Horizon's streaming, with a ticket, as `managedBlock` does. Ticking comes from Horizon's ticket levels |
| Who sees what | `lan/hub.rs` `Tracking`, `game/java_mods.rs` `java_tracking`, `Tracking`, `ChunkMapMixin` | The hub tells Java each change to a player's view, sent chunks and entity pairings. `isChunkTracked`, `getPlayers` and `sendToTracking*` answer from that, and block updates and mods' chunk packets go only to the players tracking the chunk. `ChunkWatchEvent`s fire as chunks reach a player |
| `/data` on a mod's block entity | `data.rs`, `java_hooks.rs` | Reads Java's live form; `merge` and `modify` go to the real `loadWithComponents` |
| Mods' menus and screens | `Menus`, `ClientBridge`, `Recorder`, `Sprites`, `game/java_screens.rs` | The real server menu and the real client screen: packets between them are two queues in this JVM. The screen draws through the real `GuiGraphicsExtractor`; quads (with their texture's file), text and items reach Rust in call order, and Rust paints them with its GUI atlas, items and font. Screens open and close through the real `Gui.setScreen`, so `ScreenEvent.Opening` and `Closing` fire |
| Multiplayer | `Wire`, `RemoteClient`, `Clients`, `Menus`, `game/java_mods.rs`, `lan/conn.rs`, `net.rs` | A server with Java mods (the dedicated one, or a world open to the LAN) probes each Horizon client in configuration with an empty `horizon:java` payload; a client with a JVM answers 1. That player's Java client then lives in the client's process: the server's Java side writes its configuration (features, registries and tags in full, the configs NeoForge syncs) and its play packets with Minecraft's own protocols, one `horizon:java` frame each, and the client's Java client writes its packets back the same way. A block entity's update waits for the window's chunk, as it rides with the chunk's data on a real client |
| The Java client | `ClientBridge`, `Clients`, `Menus`, `mixin/` | It joins as the window's login to its own server finishes, before any play packet: vanilla's configuration (enabled features, known packs, `SynchronizeRegistriesTask`'s registries and tags, then `filterRegistries` as a memory connection does) gives it its registries, which also decode mods' item components off the network thread. `Menus.login` sends `PlayerList.placeNewPlayer`'s packets in its order, and a change of level sends the respawn's; the real `ClientPacketListener.handleLogin` and `handleRespawn` build the `ClientLevel` and `LocalPlayer`. Every window tick runs the real `Minecraft.tick` on its "Render thread", with `ClientTickEvent`, the level's tick, entities and the block entity tickers vanilla's list holds; what draws (game renderer, level extractor, particles, music) is cancelled, as rendering is native. It learns blocks and block entities only from packets: the server view gathers changed blocks as `ChunkHolder.blockChanged` does and sends their block updates and block entity packets at the level's next tick (`broadcastChanges`); a joining block entity sends its update tag as a chunk's arrival does; mods' payloads reach the players the chunk map tracks. Text measures with Horizon's font, whose whole advance table it gets as it joins Leaving a world runs when the window disconnects, after its server has stopped (`java_client_left`): vanilla's `Minecraft.disconnect` against the already shut down server view, no screen after it, and NeoForge's `revertToFrozen` is cut out because the server in this JVM shares the registries. Mekanism's item mode key (N) opens its radial menu while held; a tap shorter than a tick (`horizon-ctl key N tap`) does nothing, as it polls the key each tick. |
| The window's input | `ClientBridge.events`, `scroll`, `game/java_screens.rs` | Each key, button, character, cursor move and wheel turn goes through the callback a real client's window library calls: `KeyboardHandler.keyPress` and `charTyped`, `MouseHandler.onButton`, `onMove` and `onScroll`, so `InputEvent.Key` and the screens' events fire and mods' `KeyMapping`s get their clicks. The world's input reaches them at the next tick in its order; a Java screen's at once. A key one of the window's own screens takes only changes the keyboard's state, and that screen opening or closing is told as `Gui.setScreen` would act on it. The window's own keys (debug keys, screenshot, fullscreen, the pause menu), the cursor's grab and text input stay native. The wheel goes through the real `MouseHandler.onScroll`, and a mod's canceled `MouseScrollingEvent` keeps the hotbar still. Inventory, permissions and game mode arrive as the login's packets and `broadcastChanges` send them. The window sends its size, focus, hotbar selection and the native movement's result only when they change; the player takes the movement in `LocalPlayer.move` |
| Clicks and `handleKeybinds`' keys | `ClientBridge.click`, `mixin/MinecraftActions`, `game/remote.rs`, `game/java_screens.rs` | Before the window attacks, uses or picks a block, the Java client runs the real `startAttack`, `startUseItem` or `pickBlockOrEntity` with the window's hit as `hitResult`, so `InteractionKeyMappingTriggered` and the client `PlayerInteractEvent`s fire first; a mod's cancel stops the window's action, with its hand swing. Drop, hand swap and hotbar keys reach it as the window performs them. Its packets for these are dropped, as the window sends its own; `handleKeybinds` itself stays cancelled |
| The Java client's chunks | `ClientWorld`, `java_bridges/client_world.rs`, `mixin/ChunkHeightMixin` | The client level's blocks, heights, light and biomes are the window's own chunks, the data the server sent this client: the window shares its columns with the Java client at each tick and click (nothing is copied) and bridged natives read them on its thread. A column arriving or leaving reaches it as `ClientChunkCache` would hear of it: `onChunkLoaded` and `ChunkEvent.Load`, or `ChunkEvent.Unload` and `ClientLevel.unload` with the column's block entity copies The window keeps the Java-visible columns up to date from `ChunkMap`'s own log of written keys (`take_touched`), so a publish with nothing written does no work, and a chunk arrival, replacement or drop is told as a `CHUNK` event. `ClientChunkCache` holds only the columns the Java client was told of (`ClientBridge.hasChunk`), applied after the tick's packets so a level exists for them; a block entity is made when its chunk arrives. |
| The Java client's sounds | `ClientSounds`, `mixin/SoundEngineMixin`, `game/java_screens.rs`, `audio.rs` | `SoundEngine`'s channels are Horizon's: each play (with its source, place, attenuation, loop and delay), each tickable sound's new volume, pitch and place after its tick, and each stop reach Horizon's sound engine as events in order, which resolves and plays them. Its finished channels come back at the next tick, so `isActive` answers as vanilla's does |
| Block models built from block entity data | `ClientModels`, `game/java_models.rs`, `dynamic_models.rs` | A code-built model whose geometry follows its block entity (cable arms, drive cells) is baked by the Java client when that data changes, off the window's thread; equal geometry is sent once and Rust meshes and draws it as any model. Its tint indexes are resolved there by the block's tint sources, so a color from block entity data is part of the geometry. Meshing costs what a native model's does; 1024 cables placed at once take 54 ms on the Java client's thread and 0.1 ms on the window's |
| Block entity renderers | `BlockRenders`, `mixin/PoseMixin`, `game/java_models.rs`, `dynamic_renders.rs` | A mod's renderer is run in the Java client when its block entity changes, through a collector that keeps what it submits (models, custom geometry, block models, items) with the `PoseStack` operations under each piece. Eight ticks are sampled with the client's ticker run between them, and each operation that changes is fitted as constant, linear or a sine of time; Rust draws the pieces every frame from that, and items with its own item models. The client copy changes only by what the server sends, as on a real client |
| Creative tabs | `CreativeTabs`, `game/java_tabs.rs`, `creative.rs` | At the Java client's login (`ClientPlayerNetworkEvent.LoggingIn`) the real `tryRebuildTabContents` runs once with and once without permissions, so mods' display generators and their additions to vanilla's tabs run. The sorted tabs (`CreativeModeTabRegistry`), pages of ten as the real screen lays them out, titles, icons, backgrounds, search bars, contents and search contents with their tooltip lines and tags reach Rust once; for that session they replace `creative-inventory.json`'s tabs. Opening the screen, searching, turning pages and permission changes run no Java. Stacks keep their components, so a full and an empty energy cube are two entries |
| Mods' item models | `ClientModels`, `BlockRenders.items`, `mixin/ModelManagerMixin`, `game/java_models.rs` | An item whose definition names a type only its mod's code knows (a special renderer, a mod's own model type) has its model baked by the client's resource reload; Rust asks it for each stack and display context once, gets the quads the model submits in item space, and draws them in its item layers. Until the answer comes, Horizon's own reading of the definition draws |
| Mods' item data components on the wire | `horizon_protocol::item::foreign`, `game/java_components.rs`, `StackComponents` | The value goes behind its type's id in Horizon's registry as the type's own `streamCodec` writes it; the other end reads it with that codec and holds the saved form. A type without a persistent codec keeps only its bytes, in the stack's `wire` patch. The id is by Horizon's merged registry (mods' sorted after vanilla's), which both ends read; only names cross into Java |
| Names and tooltips of mods' items | `ItemText`, `ClientBridge`, `game/java_item_text.rs`, `game/stations.rs`, `game/titles.rs` | For a stack of an item a mod registered (the boot's owned set), the window asks the Java client once, with the next tick's batch, for `getHoverName` and `getTooltipLines` (the item's `getName` and `appendHoverText`, NeoForge's `ItemTooltipEvent`), cached by the stack's saved form without its count and the advanced and creative flags (2048 entries, least recently used out). Slot, container and creative tooltips and the hotbar's name popup draw the answer; until it arrives they draw the components' text |
| Block outlines from block entities | `game/java_shapes.rs`, `ClientBridge`, `Clients.blockChanged` | The window asks the Java client for the outline of a mod block with a block entity and caches it per position until a block entity copy or its update packet changes |
| Block collisions from block entities | `game/java_shapes.rs`, `Clients.sendShapes`, `ClientBridge`, `World.block_entity_shapes` | A mod block with a block entity gets its `getCollisionShape(level, pos)` from Java when that block or entity changes: the server's view answers at the level's next tick for the server's collisions, and the Java client's copy for the window's. Both ask with the empty context, so a shape that differs by entity isn't told apart |
| Hoppers into and out of mods' blocks | `machines.rs`, `GameBridge.hopperTransfer` | Past vanilla containers, NeoForge's `VanillaInventoryCodeHooks` with the block's item handler, on a copy of the hopper |
| Vanilla block entities and all entities | `BlockEntities`, `EntityViews` | Real Java objects read from Horizon's saved form for one call, with what it changed written back |
| Forge Energy and other capabilities | NeoForge's own code | Run as they are on the above |
| Server start and stop, server, level and player ticks, logins, right clicks, breaks, attacks | `GameBridge`, the native hooks | NeoForge's real `ServerLifecycleHooks`, `EventHooks` and `CommonHooks`; `level_tick` with its level in place |
| Vanilla methods served from Rust | `game/java_bridges/` | Per family, each with its checks in the test mod |
| Block support shapes | `ShapeBridge`, `shapes.rs` | `-Dhorizon.java.shapes=off\|rust\|shadow`; steps aside for a mixin on the same code |
| The mod's blocks, items and data in Horizon | `BootFacts`, `game/java_content.rs`, [`horizon-port-pack carry`](modpacks.md) | Exported by the boot itself, carried in the game's process and cached: see below |
| What a mod owns | `BootFacts` | The registry entries added (or replaced) after vanilla's bootstrap, and the client's id mappers' entries vanilla's client bootstrap didn't make. Never a namespace |

What isn't bound and why is in `tools/java_mods_skips/`. `java-object` is Java data, `java-body` is vanilla's Java working on bridged calls, `slower` has its timing in the note, and `needs-*` was written before views existed, so those need a second look.

## Blocks and items from the boot

Horizon's block and item tables are built before a world can open, so a Java mod's registrations reach them as data the
JVM exports while it boots, never by asking Java later:

- `Boot.main` snapshots vanilla's registries right after vanilla's bootstrap, and `MapperBootstrapMixin` the client's
  item model mappers where their own bootstrap hands the mods its registration event. Once the mods have loaded, every
  entry added or replaced since is theirs. A snapshot that fails fails the facts export, never the boot. That set answers "is this a mod's"
  in Java (`BlockEntities.ours`, `ClientModels`, `BlockRenders`, player ticks) and in Rust, through the `states` and
  `items` lists.
- `ClientResources.take`, at the end of the client's boot reload, exports every state of each block whose model a mod's code builds, as the reload baked it
  with no block entity data (`mods/horizon/src/facts-shared-client`, the client oracle's writer too), and content
  generation carries them as `horizon:baked` models, so those blocks draw from the first frame; block entity data
  changes them later (`ClientModels`).
- After the data packs load, `BootFacts` writes the facts (`mods/horizon/src/facts-shared`, the same writer as the
  oracle's): every block's states, shapes, light and sounds, the mods' items with every default component by its codec,
  their loot tables, data component types and tags.
- The game carries them in-process (`horizon_port_pack::pipeline::carry_boot`) into content mods under
  `.horizon-cache/java-mods/<fingerprint>/`, which go in the resource root before anything else, so a by-hand carry of
  the same mod steps aside. The fingerprint hashes every jar in the run's `mods/`, the JVM's arguments (loader and
  libraries), the mods' `config/`, the game's tables and the generation number in `java_content.rs`.
- The JVM gets `--assetsDir` and `--assetIndex` as a launcher passes them: `java_mods.py` links the prepared assets into
  vanilla's layout under `assets-index/` beside the jars (symlinks, cached by the index's content), so sounds resolve
  through the real index. The `mod` lines of `horizon-java.args` name the mod jars FML loads from outside `mods/`
  (NeoForge's own), carried like the others, so the window's atlases follow the packs of the Java client's reload.
- A cached launch builds its tables at once while the JVM boots behind; when the boot is done its facts are compared
  with the ones the cache came from, and a difference is a loud warning and rebuilt content for the next launch. A first
  launch waits for the boot on the loading screen ("Loading Java mods").
- A boot that fails (FML or its loading errors, the data packs, the export) is a failure: no world opens, and the title screen shows the
  Java error. Worlds saved with the mods' blocks are never opened without them.

## Not yet

- Creative tabs are built once per login: a mod whose tab contents depend on something that changes later in the session (a config, data a server sends) keeps what it listed at login. A client without a JVM keeps vanilla's tabs.
- Mods' mixins into vanilla code Horizon runs in Rust never fire, since that Java never runs: `MinecraftServer`'s loop, `ServerLevel.tick`, entity AI and so on. 60 library mods aim mixins at 306 vanilla classes; nothing yet says how many of those patches matter.
- Screens work for a player whose window has a JVM with the same mods (its own world, or a server it joined: see Multiplayer); a client without one gets none. Pictures in a screen (an entity, a 3D block) and mods' own render pipelines and shaders draw nothing, and the client level sees only block entities, not the world's blocks or light.
- A renderer's motion is recorded from 8 ticks: one that follows the camera, real time or a curve other than a line or a sine is drawn as it was at the recording. Text a renderer draws is left out.
- The Java client's configuration runs vanilla's tasks only: NeoForge's own (registry and config sync, data map negotiation) need a real `Connection` in the configuration protocol, so its configs are this JVM's own and `ServerView` still marks none owed.
- The Java client's level holds no entities (its chunks are the window's, but not their entities), so a click on an entity doesn't reach its `startAttack` or `startUseItem`, and its own `pick` sees only block entities: a `missTime` it sets after a miss can keep its next clicks from firing `InteractionKeyMappingTriggered`. A hotbar change the server makes (pick block) doesn't reach it.
- Mods' blocks hear of changes Horizon makes after its whole update cascade, not inside `setBlock`, and a change made during a cascade reads the world as the cascade left it. Running them inside needs the level hooks to hand Java the running `Sim` instead of `&mut Game`, since `GameLevel` holds the world for as long as the cascade runs.
- A kept block entity (`shouldChangedStateKeepBlockEntity`) loses Horizon's record for a moment and Java restores it, as Horizon can't ask Java from inside the cascade.
- The chunk data a chunk event carries has the chunk's own keys but no sections, heightmaps, block entities or entities, which stay in Rust. A mod's custom spawners are kept but don't tick, and `RegisterConfigurationTasksEvent` waits for the configuration phase. A chunk's attachment sync sent before the window's Java client joined is dropped.
- Mods' own entities tick nowhere; their saved form is kept. Entity views last one call and aren't tracked between calls.
- A mod's packets reach the Java client of the player they are for, in the window's process or over `horizon:java`; with none to take them they are logged and dropped (`ConnectionMixin`). Advancement triggers do nothing for a view (`CriterionMixin`).
- A client without a JVM can't read a stack holding a mod's component, so a LAN guest with the mods' content but no JVM loses that packet. Hashed stacks leave mods' components out, so a server that checks them resyncs the slot.
- Mods' item slots don't reach Horizon's comparators until a save, and hopper minecarts don't reach mods' item handlers.
- Worldgen features and ore spawns from mods' Java code. Their data-driven ones come through `carry`.
- An effect a mod registers has no Horizon effect, so it stays in Java's view (logged). Experience and stacks still come back as copies after a call, since mod code changes stacks in place rather than through setters. Only the acting player's view writes through; a mod changing another player's view outside that player's call is lost.
- A mod's entries in a namespace no jar's mod id names (or in `minecraft`) aren't carried, as content mods declare blocks and items under their own id only, and a jar with several mods carries under the first one's id.
- Game rules, the world border and the scoreboard are vanilla's defaults in Java (an empty scoreboard: Horizon's teams and objectives don't reach it). Other players have no views, only the acting one. Bridged calls act on the acting player's dimension.
- A mod's redirect inside a bridged method doesn't run, since the method's Java body doesn't. An action bar message arrives as a chat line. `setBlock`'s update limit is ignored.
- The game finds NeoForge through dev variables, not a `java-mods` folder.

## Playing with Java mods

For a player who has a Horizon build with `--features java-mods`, a prepared Horizon (`.local/horizon-client/26.3`, from
their own Minecraft 26.3 install) and a folder of NeoForge 26.3 mod jars. Nothing here is shipped: the install is built on
the player's machine from their jars and downloads (Eclipse Temurin JDK 25, checked against Adoptium's sha256; NeoForge
26.3.0.101 from its own maven, whose installer also fetches Mojang's files, as the launcher does), like the prepare flow.

```sh
python3 tools/java_mods.py install --pack MODS --out INSTALL      # JDK, NeoForge's libraries, the host, the mods, the asset index
python3 tools/java_mods.py launch --install INSTALL --world W     # the game, with the mods; everything after the options goes to it
python3 tools/java_mods.py launch --install INSTALL --dedicated --nogui --port 25565    # the server
```

- `install` is idempotent: run it again after changing the jars. Downloads are kept in `~/.cache/horizon/java-mods`
  (`--cache`), so a second install fetches nothing. `--java DIR` uses a JDK 25 you have instead of downloading one. It needs
  the built `horizon-java-{host,boot,plugin}.jar` (`java_mods.py build`; `--host-jars` names another folder). Paths in the
  install are absolute, so run `install` again after moving it. The server's `eula.txt` is yours to accept, or pass
  `--accept-eula` once you have read <https://aka.ms/MinecraftEULA>.
- `launch` sets the environment the game needs (`JAVA_HOME`, the library path, `HORIZON_JAVA_HOST_JAR`, `HORIZON_JAVA_RUN`) and
  `--resources` from the install. `HORIZON_HOME`, `HORIZON_LAUNCHER_HOME`, `XDG_DATA_HOME` and `XDG_CONFIG_HOME` keep what
  you set and otherwise live in `INSTALL/home`, never in your own game folders. `--game PATH` names the game binary
  (default `target/fast/horizon-client`). Audio stays on; set `HORIZON_NO_AUDIO=1` to turn it off.
- The mods' blocks and items are generated by the game at its first launch from what their boot registers (a first launch waits on
  the loading screen, later ones use the cache in `HORIZON_HOME/.horizon-cache/java-mods`). No `--dev-mod` is needed.
- The server runs in `INSTALL/server` (its `server.properties`, world and logs). A client joining it needs the same install.
- Today the tool needs a Linux or macOS machine; the paths `java_mods.py` writes use `:`.
- The launcher does the same from its window or `horizon-launcher java-mods` (launcher/README.md): the jars come from the
  game version's `java-mods/` folder (a local build names its folder in a `.local-build` file), the asset index id from
  `asset-index-id.txt` in the prepared resources, and the arguments are rewritten at each launch. It writes `;` classpaths
  and `PATH`/`DYLD_LIBRARY_PATH` for Windows and macOS (untested there).

## Run it

Inside `nix-shell -p jdk25 pkg-config alsa-lib udev` with the graphics libraries of [the parity workflow](parity-workflow.md):

```sh
python3 tools/java_mods.py build
cargo build --profile fast -p horizon-client --features java-mods --bin horizon-client --bin horizon-ctl
python3 tools/java_mods.py run --jars DIR --opts=--enable-native-access=ALL-UNNAMED mods use_probe=NAMESPACE:ITEM
python3 tools/java_mods.py play --jars DIR --dev-mod CARRY "give NAMESPACE:ITEM" "wait 40" "use-item" state
python3 tools/java_mods.py bridges               # the test mod's wand on a real click; --extra-jars DIR loads more alongside
python3 tools/java_mods.py libraries --top 150   # each library mod alone, then all that booted together: real server start and one tick
python3 tools/java_mods.py fetch SLUG --out DIR  # a Modrinth project and its required dependencies
python3 tools/java_mods.py surface --jars DIR    # what these jars call, by what the port ledger knows of it
```

`play` runs in scratch folders; `--scratch DIR` reuses one, to test a save across two sessions. Without `--jars` it starts
no JVM, which is the baseline to measure against. `HORIZON_JAVA_MODS_DIR=DIR` builds, reads and runs the jars (and their
runs) from `DIR` instead of `.local/java-mods`, to try a Java change without touching the jars other runs use.

The JVM's memory is set in `create_vm` (`MEMORY` in `crates/horizon-java-host/src/lib.rs`), as a pack's launcher sets it: a
640 MB heap with compact object headers (`-XX:+UseCompactObjectHeaders`, JEP 519: about 8% less live heap), before the run's own arguments, so a `jvm -Xmx...` line in
`horizon-java.args` or `HORIZON_JAVA_OPTS` overrides it. Measured with Mekanism and AE2 on a fresh world: about 270 MB of
live heap, 140 MB of metaspace and 1.0 GB for the whole JVM. Without a cap G1 grew the heap to 2.4 GB while the server's
data packs loaded: mods' jars are zip packs, and vanilla's `FilePackResources.listResources` walks every entry of the jar on
each call, about 27 GB of short-lived garbage in 5 s.
