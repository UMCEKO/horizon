# SPEC-2-18: `StartTracking` and `StopTracking`

Index: `SPEC-2-8.md` (E4, E5). Size: ~150 lines. Needs 2-15 (pairings at vanilla's moments, reported at once) and 2-16.

## Vanilla, patched (javap of `ServerEntity`)
- `addPairing(player)`: the pairing bundle sent, `entity.startSeenByPlayer(player)`, then `EventHooks.onStartEntityTracking(entity, player)`.
- `removePairing(player)`: `entity.stopSeenByPlayer(player)`, the `ClientboundRemoveEntitiesPacket`, then `EventHooks.onStopEntityTracking(entity, player)`.

## Change
After 2-15, each pairing change reaches Java at once through `java_tracking` (kind 4: player UUID, entity id, paired). In `B/Tracking.java` `apply`, case 4:
1. Update `SEEN_BY` as now.
2. If the pair's hook is admitted for the entity's type (Rust sends a flag, below), with the entity's view and the player's view:
   - paired: `entity.startSeenByPlayer(player)` only if 2-1 or the route facts show a mod overriding or patching it (otherwise native did vanilla's part, which is nothing on `Entity`); then `safely("StartTracking", () -> EventHooks.onStartEntityTracking(entity, player))`;
   - unpaired: the same with `stopSeenByPlayer` and `onStopEntityTracking`.

**Rust:** the pairing record gains `bool fire` = `hook_admits(Hook::StartTracking or StopTracking, type, false)`, computed where 2-15 hands the changes over; when it is set, the record also carries `int mask` and the entity's group bytes (`hook_reads | reads`), applied before the hook. Players as entities (a player paired with another): the player view by UUID.

A pairing whose entity has no view (an entity added before Java's level ran, before 1d's catch-up) is a bug: log once.

`startSeenByPlayer`/`stopSeenByPlayer` overrides by mod entity types are phase 6's routes; this step only calls them when the facts say they are changed.

## Bridges
Writers 2-1 lists under `start_tracking`/`stop_tracking` (expect none).

## Test mod
- Listeners on `PlayerEvent.StartTracking` and `StopTracking` (server side) record `(phase, target id, player)`; a `ServerPlayer` field read in the listener (`player.connection != null`) proves the player view is real.
- `later` checks:
  - `start_tracking_on_spawn`: a pig summoned 5 blocks from the player fires `StartTracking` in the same tick it joins (its join tick from 1d's recorder equals the tracking tick);
  - `stop_tracking_before_leave`: discarding it fires `StopTracking` before `EntityLeaveLevelEvent` (the recorder's order);
  - `start_tracking_on_approach`: a pig 100 blocks away (outside its type's tracking range) fires `StartTracking` the tick the player is teleported next to it, before that pig's next `EntityTickEvent.Pre`.

## Scenario (parent)
Extend `tools/scenarios/entities.json` (1d) with `HORIZON_JAVA_DEBUG=1`: after the pigs, `check {"log": "Horizon: tracking start \\d+ minecraft:pig"}` (a debug line in `Tracking.apply`), and after the kill, `tracking stop` lines before the `left KILLED` lines.

## Tests to run
- `cargo test -p horizon-client --features java-mods --bins java_mods` (the extended pairing record).
- `java_mods.py build`, `check --analysis`, `bridges`.
- Live, by the parent: a Mekanism world; no `a Java mod threw during StartTracking`.
