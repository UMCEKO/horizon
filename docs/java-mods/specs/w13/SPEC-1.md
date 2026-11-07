# SPEC-1 (W13): the server event census, the gate, and the two event ops

Design: `DESIGN.md` §4.1, §4.5, D2. Read first: `CLAUDE.md`, `RULES.md`, `docs/java-mods-map.md` §3 ("Firing events")
and §6 ("Add a new event firing"), `B/Shells.java` (`ownerOf`, `wanted`: how the bus's listener lists are read),
`B/GameBridge.java` (`register`, `use`, `fill`, `levelOf`, `safely`, `writeEffects`), `G/java_mods.rs`
(`JAVA_SERVER`, `java_world_phase`, `java_level_call`, `java_call_use`, `java_apply_use`), `G/java_hooks.rs`.
Paths: `B/` = `mods/horizon/src/java-mods-host/boot/dev/horizonlab/javamods/boot/`, `G/` =
`crates/horizon-client/src/game/`, `T/` = `mods/horizon/src/java-mods-host/testmod/`, `U/` = `.../unit/`.

Depends on nothing. Every later W13 spec builds on it. About 350 changed lines.
Code comments: one line at most, only for a non-obvious why. Never restate the code.

## Goal
1. At each server start, Java reads which W13 event classes have listeners and Rust keeps the answer as a bitmask.
2. Every W13 site asks `self.java_listens(JavaEvent::X)` (one bit test) before it builds anything.
3. Two ops carry every W13 event: `level_event` (no player) and `player_event` (the acting player's view filled).
4. A census in the log: what is listened, and loudly what is listened but has no site in this build.

## The ids (wire values; Java and Rust must agree; a unit check compares them)
`0 ExplosionStart` `level.ExplosionEvent$Start`, `1 ExplosionDetonate` `level.ExplosionEvent$Detonate`,
`2 ExplosionKnockback` `level.ExplosionKnockbackEvent`, `3 PistonPre` `level.PistonEvent$Pre`, `4 PistonPost`
`level.PistonEvent$Post`, `5 EntityPlace` `level.BlockEvent$EntityPlaceEvent`, `6 EntityMultiPlace`
`level.BlockEvent$EntityMultiPlaceEvent`, `7 FarmlandTrample` `level.BlockEvent$FarmlandTrampleEvent`, `8 Bonemeal`
`entity.player.BonemealEvent`, `9 Clone` `entity.player.PlayerEvent$Clone`, `10 Respawn`
`entity.player.PlayerEvent$PlayerRespawnEvent`, `11 TravelToDimension` `entity.EntityTravelToDimensionEvent`,
`12 ChangedDimension` `entity.player.PlayerEvent$PlayerChangedDimensionEvent`, `13 SetSpawn`
`entity.player.PlayerSetSpawnEvent`, `14 WakeUp` `entity.player.PlayerWakeUpEvent`, `15 AdvancementEarn`
`entity.player.AdvancementEvent$AdvancementEarnEvent`, `16 AdvancementProgress`
`entity.player.AdvancementEvent$AdvancementProgressEvent`, `17 GameRuleChanged` `level.GameRuleChangedEvent`,
`18 PickupXp` `entity.player.PlayerXpEvent$PickupXp`, `19 XpChange` `entity.player.PlayerXpEvent$XpChange`,
`20 LevelChange` `entity.player.PlayerXpEvent$LevelChange`, `21 ItemToss` `entity.item.ItemTossEvent`, `22 ArrowLoose`
`entity.player.ArrowLooseEvent`, `23 UseItemOnBlock` `entity.player.UseItemOnBlockEvent`, `24 ChunkTicket`
`level.ChunkTicketLevelUpdatedEvent`, `25 CropGrowPre` `level.block.CropGrowEvent$Pre`, `26 CropGrowPost`
`level.block.CropGrowEvent$Post`, `27 NeighborNotify` `level.BlockEvent$NeighborNotifyEvent`.
Census only (no site in W13, DESIGN §3): `28` `entity.player.PlayerContainerEvent$Open`, `29` `...$Close`, `30`
`PlayLevelSoundEvent$AtPosition`, `31` `PlayLevelSoundEvent$AtEntity`, `32` `entity.player.PlayerRespawnPositionEvent`,
`33` `entity.player.PlayerEvent$LoadFromFile`, `34` `entity.player.PlayerEvent$SaveToFile`, `35`
`entity.player.PlayerEvent$NameFormat`, `36` `entity.player.PlayerEvent$PlayerChangeGameModeEvent`, `37`
`StatAwardEvent`, `38` `VanillaGameEvent`, `39` `level.block.CreateFluidSourceEvent`, `40`
`level.BlockEvent$FluidPlaceBlockEvent`, `41` `level.BlockDropsEvent`, `42` `level.BlockGrowFeatureEvent`, `43`
`level.NoteBlockEvent$Play`, `44` `level.SleepFinishedTimeEvent`, `45` `entity.player.CanPlayerSleepEvent`, `46`
`CommandEvent`, `47` `ServerChatEvent`, `48` `entity.player.PlayerSwitchHotbarSlotEvent$Pre`, `49`
`level.BlockEvent$BlockToolModificationEvent`.
All names are under `net.neoforged.neoforge.event.`. Ids never change meaning; new ones append.

## Java

### `B/ServerEvents.java` (new)
- `static final String[] NAMES` in id order (above). `static volatile long listened`.
- `static long census(long sited)`: for each id, `Class.forName(name, false, Boot.class.getClassLoader())`, then the bus's
  `getListenerList(type).getListeners()` (reflection exactly as `Shells.wanted`). Count a listener unless `ownerOf(listener)`'s
  package starts with `dev.horizonlab.javamods.` and is not the test mod's (`dev.horizonlab.javamods.testmod`). Move
  `Shells.ownerOf` to a package-private static in `ServerEvents` (or make it package-private where it is) and use it from both.
  The bus builds a class's list with its supertypes' listeners, so a listener on `BlockEvent` makes every `BlockEvent`
  subclass listened; the test mod checks this (below), do not add any supertype logic.
- For each listened id: `System.err.println("Horizon: server event " + type.getName() + " listened by " + owners)` (the binary name, e.g. `net.neoforged.neoforge.event.level.ExplosionEvent$Start`, as JFR and `mod_parity` name classes) where
  `owners` is the listeners' owner class names, sorted and distinct, joined by `, `.
- For an unlistened id: `System.err.println("Horizon: server event " + type.getName() + " not listened")`.
- For a listened id whose bit is not in `sited`: `System.err.println("Horizon: WARNING: server event " + type.getName() +
  " is listened (" + owners + ") but Horizon has no site for it: mods will not hear it")`.
- Sets `listened` and returns it.
- `static void post(String what, Runnable step)`: `GameBridge.safely(what, step)` plus, when `HORIZON_JAVA_DEBUG` is set,
  nothing else (each spec prints its own one debug line, format `Horizon: server event <SimpleName> <detail>`, `<SimpleName>` as `ExplosionEvent.Start`).

### `B/GameBridge.java`
- Make `safely` package-private (it is `private` today).
- In `register`: `Crossing.game("server_events", GameBridge::serverEvents)`, `Crossing.game("level_event",
  GameBridge::levelEvent)`, `Crossing.game("player_event", GameBridge::playerEvent)`.
- `serverEvents(In in, Out out)`: `out.writeLong(ServerEvents.census(in.readLong()))`.
- `levelEvent(In in, Out out)`: `var level = levelOf(in); int id = in.readByte();` then
  `ServerEvents.level(id, level, in, out)`; then `writeEffects(out)`.
- `playerEvent(In in, Out out)`: `int id = in.readByte(); var call = fill(in, false);` (the id comes **before** the
  use header, so an event can act before the view is filled: SPEC-8's `Clone` renews the view there) then
  `ServerEvents.player(id, call, in, out)`; then the use reply's tail.
- Extract the tail of `use` (from `out.writeInt(player.experienceLevel)` through `writeEffects(out)`) into
  `static void writeUseTail(Call call, Crossing.Out out)` and call it from `use` and `playerEvent`. No behaviour change.
- `ServerEvents.level`/`player` are `switch (id)` with only `default -> throw new IllegalArgumentException("server event "
  + id)` in this spec; each later spec adds its cases. Each case writes `bool canceled` then `int n` + n bytes of result
  (`Crossing.Out.writeInt` then the bytes; an empty result writes `false`, `0`).

## Rust

### `G/java_events.rs` (new, `#[cfg(feature = "java-mods")]` module beside `java_entities.rs`; add the `mod` line where
`game.rs` lists the `java_*` modules)
```rust
/// The ids this build posts from a native site; each W13 spec adds its own.
pub(super) const SITED: &[JavaEvent] = &[];
pub(super) fn sited_mask() -> u64
```
`JavaEvent` itself lives in `java_hooks.rs` (always compiled, so sites build without the feature):
```rust
/// A NeoForge server event W13 posts from a native site; the value is its wire id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum JavaEvent { ExplosionStart = 0, /* … every sited id above, up to NeighborNotify = 27 */ }
```
- `JAVA_SERVER`'s state struct (in `java_mods.rs`) gets `listened: u64`, zeroed where `started` is set.
- `impl Game<'_>`:
  - `pub(super) fn java_census(&mut self)`: `horizon_java_host::get()`, then `java_request("server_events", &sited_mask().to_be_bytes())`,
    read one `long`, store it in `JAVA_SERVER.listened`. Called at the end of `java_world_phase(true)`, after `entity_types`.
  - `pub(super) fn java_level_event(&mut self, ev: JavaEvent, body: impl FnOnce(&mut Out)) -> Option<(bool, Vec<u8>)>`:
    as `java_level_call` (same header, `java_bind_states`, `java_push_stale`, `with_active`), writing `ev as u8` first,
    and returning `(canceled, result)` after applying the effects. Make `java_level_call` and this one share one private
    helper (`java_level_reply`); `java_level_call` keeps its signature.
  - `pub(super) fn java_player_event(&mut self, ev: JavaEvent, tail: Vec<u8>) -> Option<(bool, Vec<u8>)>`: as
    `java_call_use` with op `player_event`, the request being the id byte, then `java_use_request()`'s header, then
    the tail; parse `bool`, `bytes`, then the shared tail. Split `java_apply_use` into the two `bool`+`int` head and `java_apply_use_tail(&mut In) -> Option<()>`
    (xp, stacks, effects), used by both. Then `java_drain_block_events()` as `java_call_use` does.
- `G/java_hooks.rs`: the gate, both arms:
  ```rust
  /// Whether a loaded mod listens to `ev` this server session (census at server start).
  pub(super) fn java_listens(&self, ev: JavaEvent) -> bool
  ```
  `cfg(feature = "java-mods")`: `JAVA_SERVER.with_borrow(|s| s.listened >> ev as u8 & 1 == 1)`. `cfg(not)`: `false`.
  Both arms use the `JavaEvent` defined in `java_hooks.rs`.

## Test mod (`T/`)
- New family `ChecksServerEvents` in `Checks.runAll`'s `families`. Listeners (server side only,
  `!level.isClientSide()`): one on `BlockEvent` (the base class), one on `ExplosionEvent.Start`.
- Checks (`Checks.check`): `census_sees_base_listener`: `ServerEvents.listened` has the bits of ids 5, 6, 7 and 27
  (all `BlockEvent` subclasses) set; `census_sees_direct_listener`: bit 0 set; `census_skips_horizon`: a listener the
  host registers itself (add none for this: assert that no owner printed for id 0 starts with
  `dev.horizonlab.javamods.boot`).

## Unit check `U/dev/horizonlab/javamods/boot/ServerEventsCheck.java`
- Every `NAMES` entry resolves on the boot classpath to a class assignable to `net.neoforged.bus.api.Event`.
- `NAMES.length <= 64`; names are distinct.
- Reads `crates/horizon-client/src/game/java_hooks.rs`'s `JavaEvent` enum (path from a system property the `unit`
  runner sets, as other checks read source paths; if none does, compare against a list in the check and add a Rust test
  that holds the same list) so the two id tables can't drift. Prints `ServerEventsCheck ok`.

## Rust tests (`java_events.rs`)
- `the_sited_mask_has_one_bit_per_sited_event`: with a local slice of two events, the mask is `1<<a | 1<<b`.
- `an_event_reply_reads_canceled_and_result`: feed `[1, 0,0,0,2, 7,9]` + an empty use tail to the reply parser; get
  `(true, vec![7, 9])`.
- `the_ids_match_the_java_names` (only if the unit check can't read the Rust file): the 28 names in id order.

## Done bar
- `java_mods.py build`, `unit` (`ServerEventsCheck ok`), `check` clean; `bridges` shows the three new CHECKs ok.
- `cargo test -p horizon-client --features java-mods --lib java_events`, clippy `-D warnings` with and without
  `--features java-mods`, `cargo fmt`.
- A boot log of install-rs shows `Horizon: server event ChunkTicketLevelUpdatedEvent listened by mekanism...` and, since
  nothing is sited yet, the WARNING line for it.

## Live test (lead)
`export LIVE_STATE=/home/umceko/hz-poc/agents/w13/live.json LIVE_JAR=/home/umceko/hz-poc/agents/w13/host.jar`, inside
`nix-shell -p jdk25`: `live.sh start w13`, wait for the world, `python3 tools/logsum.py <log> --grep "server event"`.
Expect one `listened by` line per listened class (Mekanism: `ChunkTicketLevelUpdatedEvent`, `PlayerRespawnEvent`,
`PlayerChangedDimensionEvent`) and a WARNING for each (no sites yet). `live.sh stop`.
