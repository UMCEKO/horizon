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
| `Item.use`, `Item.useOn`, clicks on mods' blocks | `game/java_mods.rs`, beside the native hooks | The real `ServerPlayerGameMode` and `ItemStack.useOn`, so NeoForge's events fire |
| Mods' blocks | `BlockEntities.blockChanged`, `GameBridge.blockEvents` | Real `onPlace`, removal, neighbour changes, scheduled and random ticks |
| Mods' block entities | `BlockEntities`, `game/java_bridges/block_entities.rs` | Real objects in Java; their saved form lives in Horizon's chunk, and Horizon's list gives them their turn to tick |
| Vanilla block entities and all entities | `BlockEntities`, `EntityViews` | Real Java objects read from Horizon's saved form for one call, with what it changed written back |
| Forge Energy and other capabilities | NeoForge's own code | Run as they are on the above |
| Server start and stop, server, level and player ticks, logins, right clicks, breaks, attacks | `GameBridge`, the native hooks | NeoForge's real `ServerLifecycleHooks`, `EventHooks` and `CommonHooks`; `level_tick` with its level in place |
| Vanilla methods served from Rust | `game/java_bridges/` | Per family, each with its checks in the test mod |
| Block support shapes | `ShapeBridge`, `shapes.rs` | `-Dhorizon.java.shapes=off\|rust\|shadow`; steps aside for a mixin on the same code |
| The mod's data in Horizon | [`horizon-port-pack carry`](modpacks.md), `--dev-mod` | By hand for now |

What isn't bound and why is in `tools/java_mods_skips/`. `java-object` is Java data, `java-body` is vanilla's Java working on bridged calls, `slower` has its timing in the note, and `needs-*` was written before views existed, so those need a second look.

## Not yet

- Mods' mixins into vanilla code Horizon runs in Rust never fire, since that Java never runs: `MinecraftServer`'s loop, `ServerLevel.tick`, entity AI and so on. 60 library mods aim mixins at 306 vanilla classes; nothing yet says how many of those patches matter.
- The JVM boots the dedicated-server side, so a mod that needs `net.minecraft.client` classes to load fails as it does on a NeoForge server, and stops every Java mod from starting (the game carries on without them). It would load on a NeoForge client.
- A Horizon block's properties come from a vanilla template of the same class, so `carry` with oracle facts leaves out a machine with `facing` and `lit` on a plain block. Without facts it keeps the block with no properties.
- Mods' menus and screens: `openMenu` makes the real Java menu, but no client shows it. Mods' client code doesn't run at all.
- Mods' own entities tick nowhere; their saved form is kept. Entity views last one call and aren't tracked between calls.
- A mod's packets are logged and dropped (`ConnectionMixin`). Advancement triggers do nothing for a view (`CriterionMixin`).
- Mods' item slots don't reach Horizon's hoppers, comparators or `/data` until a save.
- Worldgen features and ore spawns from mods' Java code. Their data-driven ones come through `carry`.
- A block whose properties didn't reach Horizon, as without oracle facts, reads as its one state there, so a `facing` set in Java doesn't survive a read. The NeoForge oracle stalled at export on Voluminous Energy, so this needs fixing.
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
