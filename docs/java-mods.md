# Java mods in an embedded JVM (experimental)

An optional path beside [native mods](native-mods.md) and [ported content](modpacks.md): real NeoForge 26.3 mod jars run
unchanged on real NeoForge and Minecraft classes, in a JVM inside the game's process. It is off unless the game is built
with `--features java-mods` and started with `HORIZON_JAVA_RUN` set.

Horizon stays the game. World, ticks, entities, rendering and saves are Rust; Java runs mod code and the objects mod code
touches. Anything Horizon has already ported is served from Rust unless a mod's mixin changes it, and shadow mode checks a
bridge against the real Java before it is trusted.

## What exists

| Piece | Where | State |
|---|---|---|
| FML, mixins, registries, mods' own code | `crates/horizon-java-host/src/lib.rs`, `mods/horizon/src/java-mods-host/` | Real classes, started on a background thread; a title screen doesn't wait, a world does |
| Data packs: tags, recipes, data maps, dynamic registries | `GameBridge.prepare` | Real `WorldLoader`, mods' packs included |
| Block support shapes served from Rust | `ShapeBridge`, `shapes.rs` | `-Dhorizon.java.shapes=off\|rust\|shadow`; steps aside for a mixin on the same code |
| `ServerLevel`, `ServerPlayer` as views | `GameBridge`, `mixin/` | Built without a server; position, rotation, sneaking, game mode, experience and inventory arrive with each call |
| `Item.use` | `game/java_mods.rs`, beside `native_use_item` | The real `ServerPlayerGameMode.useItem`, so NeoForge's events fire |
| What a call changed | `game/java_mods.rs` | Experience, inventory, sounds, and entities through Horizon's own NBT loader |
| The item's data in Horizon | [`horizon-port-pack carry`](modpacks.md), `--dev-mod` | By hand for now |

## Not yet

- Reads of the world from Java (`getBlockState`), `useOn`, blocks and block entities, ticking, modded entities, tooltips.
- `level.getServer()` and `player.connection` are null, so a mod reaching them throws; the use fails, is logged and changes nothing.
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
python3 tools/java_mods.py surface --jars DIR    # what these jars call, by what the port ledger knows of it
```

`play` runs in scratch folders; `--scratch DIR` reuses one, to test a save across two sessions. Without `--jars` it starts
no JVM, which is the baseline to measure against.
