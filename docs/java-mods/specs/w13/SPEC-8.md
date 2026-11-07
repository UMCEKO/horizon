# SPEC-8 (W13): `PlayerEvent.Clone`, `PlayerRespawnEvent`, `EntityTravelToDimensionEvent` (players), `PlayerChangedDimensionEvent`

Design: `DESIGN.md` §1.4, D5. Read first: SPEC-1, W9 `entities/SPEC-1d.md` (player views joining and leaving levels,
the `player_level` op, `PLAYERS`), `G/lan_actions.rs` `guest_respawn` (lines ~3827–4010, its comments name each vanilla
step), `G/server_travel.rs` (`server_player_portal_allowed`, `handle_server_portal`'s credits branch,
`start_server_trip`, `arrive_server_trips`, `run_level_teleports`, `tell_mods_changed_dimension`), `B/GameBridge.java`
(`fill`: how a player view is made on first sight).

Depends on SPEC-1 and W9 SPEC-1d (a respawn swaps the player's view object; 1d owns the view's level membership and its
join/leave events). About 350 changed lines. Code comments: one line at most, only for a non-obvious why.

## Vanilla + NeoForge (patched jar)
- `PlayerList.respawn(player, keepAllPlayerData, reason)`: `findRespawnPositionAndUseSpawnBlock` →
  `removePlayerImmediately` → `firePlayerRespawnPositionEvent` → `new ServerPlayer(...)` → `restoreFrom(old,
  keepAllPlayerData)`, whose last step is `EventHooks.onPlayerClone(newPlayer, old, !keepAllPlayerData)` → packets →
  `addRespawnedPlayer` → `AttachmentSync.syncInitialPlayerAttachments` → `EventHooks.firePlayerRespawnEvent(newPlayer,
  keepAllPlayerData)`. Death respawn: `keepAllPlayerData` false (`Clone.isWasDeath()` true, `PlayerRespawnEvent.
  isEndConquered()` false). Leaving the End through the exit portal: true / false / true.
- NeoForge's own `Clone` listener copies the attachments marked `copyOnDeath` (and all of them when not a death) from
  the old object to the new; mods (Carry On, CreativeCore, architectury) copy their data there too. The two objects are
  **different** `ServerPlayer`s.
- `ServerPlayer.teleport(transition)` to another level: `CommonHooks.onTravelToDimension(this, newDimension)` first;
  canceled → returns null, the player stays. At the end, `EventHooks.firePlayerChangedDimensionEvent(this, from, to)`.
  `PlayerList.respawn` doesn't go through `teleport`: a respawn posts no travel event.

## Java
- `GameBridge.fill(in, refresh)` gains a static flag `renewNext` (set by `playerEvent` before calling `fill`): when set,
  `fill` moves the existing view for the header's uuid to `ServerEvents.original` and builds a **new** view object for
  the uuid exactly as first sight does (without `PlayerLoggedIn`: this is not a login), registers it where 1d keeps
  player views (`PLAYERS`), then fills it from the header. The flag clears after one use.
- `ServerEvents.player`:
  - case 9 `Clone`: `boolean wasDeath = in.readBoolean();` `EventHooks.onPlayerClone(call.player, original,
    wasDeath)`; then `original = null`. Result empty.
  - case 10 `Respawn`: `EventHooks.firePlayerRespawnEvent(call.player, in.readBoolean() /* endConquered */)`.
  - case 11 `TravelToDimension`: `var to = ResourceKey.create(Registries.DIMENSION, Identifier.parse(readText(in)));`
    `canceled = !CommonHooks.onTravelToDimension(call.player, to)`.
  - case 12 `ChangedDimension`: two texts, `EventHooks.firePlayerChangedDimensionEvent(call.player, from, to)`.
  - `GameBridge.playerEvent` sets `renewNext = (id == 9)` before `fill`.
- Debug lines: `Horizon: server event PlayerEvent.Clone wasDeath=<b> same=<old == new>` (must print `same=false`),
  `PlayerEvent.PlayerRespawnEvent endConquered=<b>`, `EntityTravelToDimensionEvent to=<dim> canceled=<b>`,
  `PlayerEvent.PlayerChangedDimensionEvent <from> -> <to>`.

## Rust
- `java_hooks.rs` pairs (not-arms: nothing / false), each a `player_event` through the acting player, so call them
  where the guest is swapped in (`as_guest`) or is the acting one:
  - `java_hook_clone(&mut self, was_death: bool)`; tail `bool`.
  - `java_hook_respawned(&mut self, end_conquered: bool)`; tail `bool`.
  - `java_hook_travel(&mut self, to: Dim) -> bool` (true = canceled); tail `text dimension id`.
  - `java_hook_changed_dimension(&mut self, from: Dim, to: Dim)`; tail two texts.
  Each returns at once unless `java_listens(...)` for its id. `Clone` must still fire when only NeoForge listens: the
  census counts NeoForge's own listener (SPEC-1), so nothing special is needed.
- `guest_respawn` (death): right after `self.native_player_clone(cloned, &name, true)`:
  `self.as_guest(host, conn, |g| g.java_hook_clone(true))` (the guest's restored state is what fills the new view, as
  `restoreFrom` has run). Right after `g.native_player_respawned(respawned, &name)` inside its `as_guest`:
  `g.java_hook_respawned(false)`.
- The End exit (`handle_server_portal`'s credits branch): after `self.native_player_clone(uuid, &name, false)`:
  `self.as_guest(host, conn, |g| g.java_hook_clone(false))`. Mark the trip it starts: `start_server_trip` gets the trip
  record a new field `respawn_end_conquered: bool` (set only from this branch; add a parameter or a setter used right
  after the call); `arrive_server_trips`, where it places a player whose trip has the flag, calls
  `java_hook_respawned(true)` with the player swapped in, after the arrival's own placement steps.
- Travel: in `server_player_portal_allowed`, after the native-mod check and only if it allowed:
  `!self.as_guest(host, conn, |g| g.java_hook_travel(target))` (the signature takes `&LanHost`; change it to `&mut`
  as `as_guest` needs). In `run_level_teleports`, per teleport whose level differs from the guest's, before anything
  moves: the same hook; canceled → skip that teleport (`continue`), the player stays where it is.
- `tell_mods_changed_dimension`: inside its `as_guest`, after `native_player_changed_dimension`:
  `g.java_hook_changed_dimension(from, to)`.
- `JavaEvent::Clone`, `Respawn`, `TravelToDimension`, `ChangedDimension` into `SITED`.

## Tests
- Test mod (`ChecksServerEvents`): an attachment type `TEST_DEATH_DATA` registered with `copyOnDeath()` (NeoForge's
  `AttachmentType.builder(...).copyOnDeath()`), and a `Clone` listener recording `(wasDeath, original != player)`; a
  `PlayerRespawnEvent` listener recording `isEndConquered()`; a travel listener cancelling travel to `minecraft:the_end`.
  `drive`: set the attachment on the player (`player.setData(TEST_DEATH_DATA, 42)` from a test command or the wand),
  `/kill @s`, respawn (horizon-ctl), `/execute in minecraft:the_end run tp @s 0 100 0`, `/execute in
  minecraft:the_nether run tp @s 0 100 0`, back to the overworld. Checks (`later`):
  - `clone_has_two_objects`: one Clone with `wasDeath` true and `original != player`;
  - `clone_copies_death_data`: after respawn `player.getData(TEST_DEATH_DATA) == 42` (NeoForge's own copy ran: an
    independent expectation from NeoForge's documented `copyOnDeath`);
  - `respawn_not_end_conquered`: one `PlayerRespawnEvent` with `false`;
  - `travel_cancel_keeps_level`: the player is not in the End after the first teleport;
  - `changed_dimension_once`: exactly one `PlayerChangedDimensionEvent` overworld → nether, one nether → overworld.
- Rust: `the_end_exit_trip_says_end_conquered` (unit test on the trip record: the credits branch's trip has the flag,
  a portal trip doesn't).

## Done bar
- `bridges` CHECKs pass (both sessions). `cargo test -p horizon-client --features java-mods --lib server_travel`,
  clippy both ways, fmt.
- `lan.sh` respawn-type LAN check (the one `tools/lan_e2e.py --list` names for respawn; at most one) unchanged.
- M1 `w13` steps `nether` and `death`: `PlayerEvent$Clone`, `PlayerEvent$PlayerRespawnEvent`,
  `EntityTravelToDimensionEvent`, `PlayerEvent$PlayerChangedDimensionEvent` equal Forge's.
- Append to `tools/scenarios/w13-events.json`: the four debug-line checks (Clone with `same=false`).

## Live test (lead, install-rs, `HORIZON_JAVA_DEBUG=1`)
1. Mekanism: put on a Mekanism jetpack (or any item with Mekanism player data), die, respawn: keepInventory off, the
   items drop as usual; no `a Java mod threw during PlayerEvent.Clone` / `PlayerRespawnEvent`.
2. Build a nether portal, go through and back: two `PlayerChangedDimensionEvent` lines; Mekanism's per-dimension data
   (a radiation reading or a QIO dashboard opened before and after) behaves as before.
3. Dedicated server with two clients: the second player dies and respawns; its Clone line shows `same=false`.
