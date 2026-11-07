# SPEC-3: route values and receivers (what the batches pass and return)

Design: `DESIGN.md` §2.5. Needs SPEC-2 (the player fill format; a `Player` argument is that fill). Size: ~400 changed lines.

Paths as SPEC-2; `H/` = `crates/horizon-java-host/src/`. Read first: `G/java_routing.rs` (all of it), `B/Routing.java` `run`, `B/Flat.java`, `H/bridge.rs` (`Arg`, `Ret`, `Source`, `Ent`, `Point`, `Text`, `Bytes`), `H/routes.rs`, W9 `SPEC-3-1.md` (which adds `Recv::Entity`; don't add it here).

Comments in code: one line at most, only for a non-obvious why.

## 1. Java reads arguments by type, through `Flat`
Replace the `if/else` chain in `Routing.run` with a table built once in `Routing.confirm`: for each route, one `ArgReader` per parameter, chosen by the parameter's class:

| Parameter type | Wire (Rust writes) | Java builds |
|---|---|---|
| `int long float double boolean` | as today | as today |
| `BlockPos` | `long` | `BlockPos.of` |
| `String` | `text` | as today |
| `ItemStack`, `ItemInstance` | `bytes` (saved tag) | `GameBridge.decode` |
| `Component` | `bytes` (`Flat.raw(Component)`'s form) | `Flat.toComponent(byte[])` (add if missing; the working tree's `ComponentArg` writes the wrapper `Flat.raw` reads) |
| `Player`, `ServerPlayer` | the SPEC-2 fill | `GameBridge.fill(in, false).player()` |
| `Vec3` | 3 `double` | `new Vec3` |
| `Entity` and subclasses | `int` id, 0 = null | `Flat.toEntity(int)` (W9 views); a type mismatch throws `ClassCastException` naming the route |
| `DamageSource` | W9 1c2a's `Source` bytes | `Flat.toDamageSource(byte[], ServerLevel)` (add: the inverse of `Flat.raw(DamageSource)`, `Flat.java:179`; entity refs through `toEntity`) |
| `MobEffectInstance` | `text id, int duration, int amplifier, bool ambient, bool visible, bool icon` | `new MobEffectInstance(holder, …)` |
| `Holder<T>` | `text` registry id | `Flat.toHolder(String)` (exists) |
| `ResourceKey<T>` | `text registry`, `text id` | `ResourceKey.create` |
| `Level`, `ServerLevel`, `LevelReader`, `LevelAccessor`, `ServerLevelAccessor`, `BlockGetter` | `text dim` | `GameBridge.level(dim)` |
| `TeleportTransition` | `Flat.raw(TeleportTransition)`'s bytes | `Flat.toTeleportTransition` (add, inverse of `Flat.java:69`) |
| `ValueInput` | `bytes` (a compound tag) | `TagValueInput.create(ProblemReporter.DISCARDING, registryAccess, tag)` |
| `ValueOutput` | nothing | a fresh `TagValueOutput.createWithContext(...)`; after the call the reply carries its `buildResult()` (§3) |

Any other type: the boot fails at `confirm` for an `on` route that has a gate (`route <key>: no argument reader for <type>`), and logs once for one without a gate (today's behaviour). A gated route that can't run is a build error, not a runtime surprise.

## 2. Rust writes them (`G/java_routing.rs`)
`RouteArg` impls for: `[f64; 3]` (Vec3), `&Source` (W9 1c2a, `H/bridge.rs:395`'s fields in the order `Flat.toDamageSource` reads), `&EffectArg` (new struct: `id`, `duration`, `amplifier`, `ambient`, `visible`, `icon`), `EntityArg(Option<u32>)`, `HolderArg<'a>(&'a str)`, `KeyArg<'a>(&'a str, &'a str)`, `LevelArg<'a>(&'a str)` (dim id), `TransitionArg(Vec<u8>)`, `InputArg<'a>(&'a Tag)`, `OutputArg` (writes nothing). Keep `ComponentArg` and `PlayerArg` as the working tree has them.

## 3. Returns and written outputs
- `RouteRet` impls for `[f64; 3]` (Java writes 3 doubles), `Option<ItemStack>` (bytes; empty = `None`), `String` (`text`), `Option<[f64; 3]>` (`bool` then 3 doubles, for `Optional<Vec3>`).
- Java `run`: return types `V Z I J F D` as today, plus `Vec3`, `ItemStack`, `String`, `Optional<Vec3>`, `ResourceKey` (as `text id`), `InteractionResult` (as `int`: the ordinal of `InteractionResult`'s sealed subtypes in this order: `Success`, `Fail`, `Pass`, `TryEmptyHandInteraction`, then `Success`'s `SwingSource`; write it as a pair `int kind, int swing`), `Optional<T>` of those.
- **Written outputs:** after the value, for each `ValueOutput` parameter in order, `bytes` of `output.buildResult()` (an empty compound if nothing was written). Rust reads them with `route_call_out::<R>(…) -> Option<(R, Vec<Tag>)>`; `route_call` stays for routes without outputs.
- `InteractionResult` on the Rust side: a `RouteRet` for the existing native result type the use paths return (find it in `G/hand.rs` / `game.rs` `use_item`; if there is none shared, add `pub(in crate::game) enum UseResult { Success(bool swing), Fail, Pass, TryEmptyHand }`).

## 4. Receivers
`Recv` (in `G/java_routing.rs`) gains:
- `Server` (byte 3): Java receiver `GameBridge.server` (the `MinecraftServer` view).
- `Scoreboard` (byte 4): `GameBridge.server.getScoreboard()`. Its contents come from SPEC-6; until then a route on it is `Admitted::Pending` (never crosses).
- `BlockEntity(BlockPos)` (byte 5, then `text dim`, `long game_time`, `long pos`): `level.getBlockEntity(pos)`; a null or wrong-class receiver throws naming the route. The views are SPEC-16's; until it lands the family stays `Pending`.

`Recv::Player`'s owner projection in `Routing.run` grows from `Inventory`/`AbstractContainerMenu` to a table by the route owner's class:
- `Inventory` → `player.getInventory()`;
- `AbstractContainerMenu` and subclasses → `player.containerMenu`, and the route throws if `containerMenu` isn't an instance of the owner;
- `ServerGamePacketListenerImpl`, `ServerCommonPacketListenerImpl` → `player.connection`;
- `FoodData` → `player.getFoodData()`;
- otherwise the player.

`Gate::admitted` treats `Server` like `Static`. `BlockEntity` and `Scoreboard` are admitted only when the route's `Admitted` isn't `Pending`.

## 5. The patched-method fact for W10 (`H/routes.rs`, `B/Routing.java`)
- Rust: `pub fn patched(key: &str) -> bool` = the route exists and is `on`. W10's seams call it.
- Java: `public static boolean patched(String key)` = `route(key) >= 0`.
- Both also answer for keys whose owner is W10 in `owners.tsv`: they are in `routes.bin` like any route.

## 6. Tests
- Rust (`G/java_routing.rs` tests): `vec3_and_effect_args_write_as_java_reads_them` (bytes written out in the test, as `route_args_write_as_java_reads_them`); `optional_vec3_reads_both_cases`; `item_stack_return_round_trips` (stone ×3 with a custom name tag, through `item_from_nbt`); `written_outputs_follow_the_value` (a hand-built reply: status 0, `int 7`, `bytes` of `{a:1b}` → `(7, [tag])`, then effects).
- Java (`U/RoutingArgsCheck.java`, extend): each new reader on the bytes the Rust test prints; `ArgReader` for an unknown type on a gated route makes `confirm` throw (call the table builder directly with a fake route).
- Java: `RoutingRecvCheck` (new): the owner projection picks `connection` for `ServerGamePacketListenerImpl#tryPickItem` and throws for a `ChestMenu` route when `containerMenu` is the inventory menu.
- `cargo clippy -p horizon-client --features java-mods -- -D warnings` (and without); `cargo test -p horizon-java-host routes`; `java_mods.py build`, `check`, `unit`.

## Done bar
Tests pass; `java_mods.py check` adds no problem; `java_routing.py lint` passes; a boot of the showcase install prints no `no argument reader` for any route that SPEC-6 … SPEC-20 will gate (list the remaining ones in the report: they tell the batches what's missing).

## For the lead
No behaviour change by itself. Run the `bridges` session once; the three `route_*` checks still pass. No user steps.
