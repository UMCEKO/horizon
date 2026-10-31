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
| Block support shapes served from Rust | `ShapeBridge`, `shapes.rs` | `-Dhorizon.java.shapes=off\|rust\|shadow`; steps aside for a mixin on the same code |
| `ServerLevel`, `ServerPlayer` as views | `GameBridge`, `mixin/` | Built without a server; position, rotation, sneaking, game mode, experience and inventory arrive with each call |
| `Item.use`, `Item.useOn` | `game/java_mods.rs`, beside the native hooks | The real `ServerPlayerGameMode.useItem` and `ItemStack.useOn`, so NeoForge's events fire |
| World access from Java | `game/java_bridges.rs` | Blocks, signals, scheduled ticks, game events, heightmaps, light, stats, messages |
| What a call changed | `game/java_mods.rs` | Experience, inventory, sounds, and entities through Horizon's own NBT loader |
| The item's data in Horizon | [`horizon-port-pack carry`](modpacks.md), `--dev-mod` | By hand for now |

## Not yet

- Block entities and `getEntities`, blocks' own behaviour, ticking, modded entities, tooltips.
- `level.getServer()` and `player.connection` are null, so a mod reaching them throws; the use fails, is logged and keeps what it already did to the world.
- An action bar message arrives as a chat line. `setBlock`'s update limit is ignored, and bridged calls act on the acting player's dimension.
- Vanilla items don't reach Java, so a mod's `RightClickItem` listener doesn't hear them.
- The player's name is the host's on a server with several players; health and food don't arrive yet.
- The dedicated server starts the JVM but doesn't wait for it when a world opens.
- The game finds NeoForge through dev variables, not a `java-mods` folder.

## Run it

Inside `nix-shell -p jdk25 pkg-config alsa-lib udev` with the graphics libraries of [the parity workflow](parity-workflow.md):

```sh
python3 tools/java_mods.py build
cargo build --profile fast -p horizon-client --features java-mods --bin horizon-client --bin horizon-ctl
python3 tools/java_mods.py run --jars DIR --opts=--enable-native-access=ALL-UNNAMED mods use_probe=NAMESPACE:ITEM
python3 tools/java_mods.py play --jars DIR --dev-mod CARRY "give NAMESPACE:ITEM" "wait 40" "use-item" state
python3 tools/java_mods.py bridges               # the test mod's wand on a real click
python3 tools/java_mods.py surface --jars DIR    # what these jars call, by what the port ledger knows of it
```

`play` runs in scratch folders; `--scratch DIR` reuses one, to test a save across two sessions. Without `--jars` it starts
no JVM, which is the baseline to measure against.
