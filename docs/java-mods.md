# Java mods in an embedded JVM (experimental)

An optional path beside [native mods](native-mods.md) and [ported content](modpacks.md): real NeoForge 26.3 mod jars run
unchanged on real NeoForge and Minecraft classes, in a JVM inside the game's process. It is off unless the game is built
with `--features java-mods` and started with `HORIZON_JAVA_RUN` set.

Horizon stays the game. World, ticks, entities, rendering and saves are Rust; Java runs mod code and the objects mod code
touches. Anything Horizon has already ported is served from Rust, except getters and tiny methods, which are faster left in
Java.

## Serving a method from Rust

One entry in `crates/horizon-client/src/game/java_bridges.rs`, and nothing in Java:

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
| What mods add to constructors | `ViewInits`, `Inits` | A view skips its constructor, so a class processor after Mixin keeps mods' injections and field initializers from it, and they run once the view is filled. An injection capturing a local gets a default for it |
| `Item.use`, `Item.useOn`, clicks on mods' blocks | `game/java_mods.rs`, beside the native hooks | The real `ServerPlayerGameMode` and `ItemStack.useOn`, so NeoForge's events fire |
| Mods' blocks | `BlockEntities.blockChanged`, `GameBridge.blockEvents` | `LevelChunk.setBlockState`'s steps with the real flags (`UPDATE_NEIGHBORS`, piston, skipped side effects, skipped `onPlace`), the real neighbour in `neighborChanged`, scheduled and random ticks. They run after Horizon's cascade: see Not yet |
| Mods' block entities | `BlockEntities`, `game/java_bridges/block_entities.rs` | Real objects in Java; their saved form lives in Horizon's chunk, and Horizon's list gives them their turn to tick |
| Joining the level | `BlockEntities.join`, `loadFresh` | `setLevel` and `clearRemoved` as `LevelChunk.setBlockEntity` does, then `onLoad` before the next ticker, as `Level.tickBlockEntities` does |
| Chunk load and unload | `game/java_mods.rs` `java_chunks_changed`, `GameBridge.chunksChanged` | A column joining the level has its mods' block entities taken as `LevelChunk.postProcessGeneration` promotes them; one leaving hands back what it hadn't saved, then `onChunkUnloaded` and `clearAllBlockEntities`. Chunks loaded before the server's start catch up after it, as servers' configs load there |
| Chunks | `ChunkViews`, `LevelChunkMixin`, `ChunkCacheMixin` | `getChunk`, `getChunkNow` and `hasChunk` answer for chunks Horizon has loaded with a `LevelChunk` holding nothing: its blocks and block entities are the level's. `getChunk` with `load` blocks the server thread on Horizon's streaming, with a ticket, as `managedBlock` does. Ticking comes from Horizon's ticket levels |
| Who sees what | `lan/hub.rs` `Tracking`, `game/java_mods.rs` `java_tracking`, `Tracking`, `ChunkMapMixin` | The hub tells Java each change to a player's view, sent chunks and entity pairings. `isChunkTracked`, `getPlayers` and `sendToTracking*` answer from that, and block updates and mods' chunk packets go only to the players tracking the chunk. `ChunkWatchEvent`s fire as chunks reach a player |
| `/data` on a mod's block entity | `data.rs`, `java_hooks.rs` | Reads Java's live form; `merge` and `modify` go to the real `loadWithComponents` |
| Mods' menus and screens | `Menus`, `ClientBridge`, `Recorder`, `Sprites`, `game/java_screens.rs` | The real server menu and the real client screen: packets between them are two queues in this JVM, and a joining client gets a login's recipe sync. The screen draws through the real `GuiGraphicsExtractor`; quads (with their texture's file), text and items reach Rust in call order, and Rust paints them with its GUI atlas, items and font. Mouse and keys go back as `MouseHandler` and `KeyboardHandler` hand them over |
| The Java client | `ClientBridge`, `Clients`, `mixin/Client*` | Its own `ClientLevel`, `LocalPlayer` and connection as views. It learns blocks and block entities only from packets: the server view gathers changed blocks as `ChunkHolder.blockChanged` does and sends their block updates and block entity packets at the level's next tick (`broadcastChanges`); a joining block entity sends its update tag as a chunk's arrival does; mods' payloads reach the players the chunk map tracks. Mods' client reload listeners run once over the real packs; text measures with Horizon's font |
| The window's input in the world | `ClientBridge.keys`, `scroll`, `game/java_screens.rs` | Held keys reach the client's `KeyMapping`s at its next tick and its `KeyboardInput` reads them, so `isShiftKeyDown` is true on the client. The wheel goes through the real `MouseHandler.onScroll`, and a mod's canceled `MouseScrollingEvent` keeps the hotbar still. Inventory, permissions and game mode arrive as the login's packets and `broadcastChanges` send them; the hotbar selection follows the window's |
| Block models built from block entity data | `ClientModels`, `game/java_models.rs`, `dynamic_models.rs` | A code-built model whose geometry follows its block entity (cable arms, drive cells) is baked by the Java client when that data changes, off the window's thread; equal geometry is sent once and Rust meshes and draws it as any model. Its tint indexes are resolved there by the block's tint sources, so a color from block entity data is part of the geometry. Meshing costs what a native model's does; 1024 cables placed at once take 54 ms on the Java client's thread and 0.1 ms on the window's |
| Block entity renderers | `BlockRenders`, `mixin/PoseMixin`, `game/java_models.rs`, `dynamic_renders.rs` | A mod's renderer is run in the Java client when its block entity changes, through a collector that keeps what it submits (models, custom geometry, block models, items) with the `PoseStack` operations under each piece. Eight ticks are sampled with the client's ticker run between them, and each operation that changes is fitted as constant, linear or a sine of time; Rust draws the pieces every frame from that, and items with its own item models. The client copy changes only by what the server sends, as on a real client |
| Mods' item models | `ClientModels`, `BlockRenders.items`, `mixin/ModelManagerMixin`, `game/java_models.rs` | An item whose definition names a type only its mod's code knows (a special renderer, a mod's own model type) has its model baked in the Java client; Rust asks it for each stack and display context once, gets the quads the model submits in item space, and draws them in its item layers. Until the answer comes, Horizon's own reading of the definition draws |
| Mods' item data components on the wire | `horizon_protocol::item::foreign`, `game/java_components.rs`, `StackComponents` | The value goes behind its type's id in Horizon's registry as the type's own `streamCodec` writes it; the other end reads it with that codec and holds the saved form. A type without a persistent codec keeps only its bytes, in the stack's `wire` patch. The id is by Horizon's merged registry (mods' sorted after vanilla's), which both ends read; only names cross into Java |
| Block outlines from block entities | `game/java_shapes.rs`, `ClientBridge`, `Clients.blockChanged` | The window asks the Java client for the outline of a mod block with a block entity and caches it per position until a block entity copy or its update packet changes |
| Block collisions from block entities | `game/java_shapes.rs`, `Clients.sendShapes`, `ClientBridge`, `World.block_entity_shapes` | A mod block with a block entity gets its `getCollisionShape(level, pos)` from Java when that block or entity changes: the server's view answers at the level's next tick for the server's collisions, and the Java client's copy for the window's. Both ask with the empty context, so a shape that differs by entity isn't told apart |
| Hoppers into and out of mods' blocks | `machines.rs`, `GameBridge.hopperTransfer` | Past vanilla containers, NeoForge's `VanillaInventoryCodeHooks` with the block's item handler, on a copy of the hopper |
| Vanilla block entities and all entities | `BlockEntities`, `EntityViews` | Real Java objects read from Horizon's saved form for one call, with what it changed written back |
| Forge Energy and other capabilities | NeoForge's own code | Run as they are on the above |
| Server start and stop, server, level and player ticks, logins, right clicks, breaks, attacks | `GameBridge`, the native hooks | NeoForge's real `ServerLifecycleHooks`, `EventHooks` and `CommonHooks`; `level_tick` with its level in place |
| Vanilla methods served from Rust | `game/java_bridges/` | Per family, each with its checks in the test mod |
| Block support shapes | `ShapeBridge`, `shapes.rs` | `-Dhorizon.java.shapes=off\|rust\|shadow`; steps aside for a mixin on the same code |
| The mod's data in Horizon | [`horizon-port-pack carry`](modpacks.md), `--dev-mod` | By hand for now |

What isn't bound and why is in `tools/java_mods_skips/`. `java-object` is Java data, `java-body` is vanilla's Java working on bridged calls, `slower` has its timing in the note, and `needs-*` was written before views existed, so those need a second look.

## Not yet

- Mods' mixins into vanilla code Horizon runs in Rust never fire, since that Java never runs: `MinecraftServer`'s loop, `ServerLevel.tick`, entity AI and so on. 60 library mods aim mixins at 306 vanilla classes; nothing yet says how many of those patches matter.
- Screens work for the player of the window that hosts the world; a LAN guest or a dedicated server's player gets none, as their client has no JVM of its own. Pictures in a screen (an entity, a 3D block) and mods' own render pipelines and shaders draw nothing, and the client level sees only block entities, not the world's blocks or light.
- A renderer's motion is recorded from 8 ticks: one that follows the camera, real time or a curve other than a line or a sine is drawn as it was at the recording. Text a renderer draws is left out.
- `KeyboardHandler.keyPress` itself doesn't run on the Java client (its global and debug keys would act on a window it doesn't have), so `InputEvent.Key` isn't fired and nothing consumes a `KeyMapping`'s clicks: a mod's own key bindings do nothing in the world yet.
- Mods' blocks hear of changes Horizon makes after its whole update cascade, not inside `setBlock`, and a change made during a cascade reads the world as the cascade left it. Running them inside needs the level hooks to hand Java the running `Sim` instead of `&mut Game`, since `GameLevel` holds the world for as long as the cascade runs.
- A kept block entity (`shouldChangedStateKeepBlockEntity`) loses Horizon's record for a moment and Java restores it, as Horizon can't ask Java from inside the cascade.
- `ChunkEvent.Load` and `Unload` don't fire, chunks' attachments aren't saved, and a chunk's attachment sync sent before the window's Java client joined is dropped. The Java client isn't told a chunk was forgotten, so it keeps those block entity copies.
- Mods' own entities tick nowhere; their saved form is kept. Entity views last one call and aren't tracked between calls.
- A mod's packets reach only the window's own Java client; with none to take them they are logged and dropped (`ConnectionMixin`). Advancement triggers do nothing for a view (`CriterionMixin`).
- A client without a JVM can't read a stack holding a mod's component, so a LAN guest with the mods' content but no JVM loses that packet. Hashed stacks leave mods' components out, so a server that checks them resyncs the slot.
- Mods' item slots don't reach Horizon's comparators until a save, and hopper minecarts don't reach mods' item handlers.
- Worldgen features and ore spawns from mods' Java code. Their data-driven ones come through `carry`.
- A block without oracle facts keeps only its template's states, so a `facing` set in Java doesn't survive a read. `carry` with facts declares them.
- Game rules and the world border are vanilla's defaults in Java. Other players have no views, only the acting one. Bridged calls act on the acting player's dimension.
- A mod's redirect inside a bridged method doesn't run, since the method's Java body doesn't. An action bar message arrives as a chat line. `setBlock`'s update limit is ignored.
- The dedicated server starts the JVM but doesn't wait for it when a world opens. The game finds NeoForge through dev variables, not a `java-mods` folder.

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
no JVM, which is the baseline to measure against.
