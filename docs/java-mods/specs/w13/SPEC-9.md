# SPEC-9 (W13): `PlayerSetSpawnEvent`, `PlayerWakeUpEvent`, `AdvancementEvent.*`, `GameRuleChangedEvent`

Design: `DESIGN.md` §1.3–1.4. Read first: SPEC-1, `G/game.rs` (`try_sleep` near 15776, `wake_up` near 15982),
`G/respawn_anchor.rs` `respawn_anchor_used`, `G/commands.rs` (`cmd_spawnpoint`, `set_game_rule`),
`G/server_travel.rs` `resolve_server_respawn`, `G/lan_actions.rs` `wake_guest_from`, `G/advancements.rs`
(`advancement_progress`, `award`, where criteria are revoked by `/advancement revoke`), the patched bytecode of
`ServerPlayer.setRespawnPosition`, `Player.stopSleepInBed`, `PlayerAdvancements.award` and its `lambda$award$0`,
`MinecraftServer.onGameRuleChanged` (`javap/callers.javap`).

Depends on SPEC-1. About 350 changed lines. Code comments: one line at most, only for a non-obvious why.

## Vanilla + NeoForge
- `ServerPlayer.setRespawnPosition(config, message)`: first `EventHooks.onPlayerSpawnSet(this, config)`; true
  (canceled) returns before anything changes. `config` may be null (clearing).
- `Player.stopSleepInBed(wakeImmediately, updateLevelForSleepingPlayers)`: first `EventHooks.onPlayerWakeup(this,
  wakeImmediately, updateLevel)` (not cancellable). Callers: `ServerLevel.wakeUpAllPlayers` → `(false, false)`; leaving
  the bed (`ServerGamePacketListenerImpl` `STOP_SLEEPING`) → `(false, true)`; `Player.tick`'s "bed is gone / it's day"
  check → `(true, true)`.
- `PlayerAdvancements.award(holder, criterion)`: `grantProgress` true → `EventHooks.onAdvancementProgressedEvent(player,
  holder, progress, criterion, GRANT)` → if it just became done: rewards, then for an advancement **with a display**:
  the chat announcement (when `announceToChat` and the rule allow) then `EventHooks.onAdvancementEarnedEvent(player,
  holder)`. `revoke` posts the progress event with `REVOKE`. An advancement without a display (recipes) is never "earned"
  for the event.
- `MinecraftServer.onGameRuleChanged(rule, value)`: the server's reaction (broadcasts) then
  `EventHooks.onGameRuleChanged(server, rule, value)` last.

## Wire (`player_event` unless noted)
- id 13 `SetSpawn`: `bool present`; if present `text dimension`, `long pos`, `float yaw`, `float pitch`, `bool forced`.
  Result empty; `canceled`.
- id 14 `WakeUp`: `bool wakeImmediately`, `bool updateLevel`.
- id 15 `AdvancementEarn`: `text advancement id`.
- id 16 `AdvancementProgress`: `text advancement id`, `text criterion`, `byte type` (0 GRANT, 1 REVOKE), `int n` x
  `text` (the criteria done now, after this change).
- id 17 `GameRuleChanged` (`level_event`, overworld header): `text rule id`, `text value` (as `/gamerule` prints it).

## Java (`ServerEvents`)
- case 13: build `ServerPlayer.RespawnConfig` (javap it: its record components, the `LevelData.RespawnData` inside) or
  null; `canceled = EventHooks.onPlayerSpawnSet(call.player, config)`.
- case 14: `EventHooks.onPlayerWakeup(call.player, a, b)`.
- case 15/16: `var holder = server.getAdvancements().get(Identifier.parse(id))` (the real manager the data packs
  loaded). Progress: a fresh `AdvancementProgress`, `update(holder.value().requirements())`, then `grantProgress` for
  each done criterion in the list, so `isDone()`/`getPercent()` answer as native's state. Post
  `onAdvancementProgressedEvent(call.player, holder, progress, criterion, type)` / `onAdvancementEarnedEvent`. An id the
  manager doesn't know: log once and skip (a Horizon-only advancement can't exist; the log names it).
- case 17: `var rule = BuiltInRegistries.GAME_RULE.getValue(Identifier.parse(id))`; parse the value with the rule's own
  command parser (javap `net.minecraft.world.level.gamerules.GameRule`; use what `GameRuleCommand` uses). Before
  posting, write the value into the Java server's `GameRules` **without** its change callback (an accessor mixin on the
  field that holds the values; find it with javap) so `event.getGameRules()` answers the new value, then
  `EventHooks.onGameRuleChanged(server, rule, value)`.
- Debug lines: `Horizon: server event PlayerSetSpawnEvent present=<b> canceled=<b>`, `PlayerWakeUpEvent
  <a> <b>`, `AdvancementEvent.AdvancementEarnEvent <id>`, `AdvancementEvent.AdvancementProgressEvent <id> <criterion>
  <type>`, `GameRuleChangedEvent <id>=<value>`.

## Rust
- `java_hooks.rs` pairs:
  - `java_hook_set_spawn(&mut self, config: Option<&RespawnConfig>) -> bool` (true = canceled). Use the type the
    four sites already build for `native_player_spawn_point_set` (check `try_sleep` and `cmd_spawnpoint`); `None` when
    the site clears the point.
  - `java_hook_wake_up(&mut self, wake_immediately: bool, update_level: bool)`.
  - `java_hook_advancement_earned(&mut self, id: &str)`, `java_hook_advancement_progress(&mut self, id: &str, criterion:
    &str, revoked: bool)`.
  - `java_hook_game_rule_changed(&mut self, rule: &str, value: &str)`.
- Sites (each runs with the player swapped in; check each site already is, the native-mod hook beside it says so):
  - `try_sleep`, `respawn_anchor_used`, `cmd_spawnpoint` (per target player, inside its `as_guest`),
    `resolve_server_respawn` (the clear): **before** the native spawn write and before
    `native_player_spawn_point_set`; canceled → the point is not changed (no message, no native-mod hook). Vanilla's
    `setRespawnPosition` is the whole change, so skip exactly what that call does in each site, nothing else (the bed
    still puts the player to sleep in `try_sleep`: vanilla's `startSleepInBed` sets the spawn then sleeps).
  - `wake_up` and `wake_guest_from`: first statement, before `native_player_bed_leave`; pass the flags of the vanilla
    caller the site mirrors (its comments say which; the morning wake of all players is `(false, false)`, the leave-bed
    button `(false, true)`, the bed-gone check `(true, true)`). If one site serves two callers, add a parameter for the
    flags at its call sites.
  - `advancement_progress` after `native_player_advancement_progress` (both the grant and the revoke call);
    `award` after `native_player_advancement_done` and after the chat announcement, only when the advancement has a
    display.
  - `set_game_rule`: last, after the rule is stored and broadcast; the value text is what `/gamerule <rule>` prints.
- `JavaEvent::SetSpawn`, `WakeUp`, `AdvancementEarn`, `AdvancementProgress`, `GameRuleChanged` into `SITED`.

## Tests
- Test mod (`ChecksServerEvents`): a `PlayerSetSpawnEvent` listener cancelling when `getNewSpawn()` is at the marked
  bed position; listeners recording wake-ups (both flags), advancement earns and progress, and game rule changes with
  `event.getGameRules().get(rule)` read inside the listener.
  `drive`: `/spawnpoint @s <marked pos>`, `/spawnpoint @s <other pos>`, `/advancement grant @s only
  minecraft:story/mine_stone`, `/gamerule keepInventory true`, sleep through a night (`/time set 13000`, use the bed,
  `wait 120`). Checks:
  - `set_spawn_cancel_keeps_point`: after the first `/spawnpoint` the player's respawn point is unchanged (native's, read
    through `getRespawnConfig()` after a refresh), after the second it is the other position;
  - `advancement_earned_once`: one Earn for `minecraft:story/mine_stone`, one Progress `GRANT` before it;
  - `game_rule_seen_new`: the listener read `true` for `keepInventory` (independent: the command's own value);
  - `wake_up_after_night`: one wake-up with `(false, false)`.
- Rust: the advancement site fires Earn only for a displayed advancement: unit test on the decision helper
  (`fn earns_event(display: bool, done_now: bool) -> bool`).

## Done bar
- `bridges` CHECKs pass; `cargo test -p horizon-client --features java-mods --lib advancements`, clippy both ways, fmt.
- Harnesses (at most two, through `verify/parity.sh`): the advancement and sleep ones `tools/parity.py --changed-files`
  lists for the touched files, unchanged.
- M1 `w13`: `AdvancementEvent$AdvancementProgressEvent` and `$AdvancementEarnEvent` over the whole run, `PlayerSetSpawnEvent`
  and `PlayerWakeUpEvent` (step `sleep`), `GameRuleChangedEvent` (step `rule`) equal Forge's where listened.
- Append the five debug-line checks to `tools/scenarios/w13-events.json`.

## Live test (lead, showcase jars, `HORIZON_JAVA_DEBUG=1`)
1. Sleep in a bed at night: "Respawn point set", the night skips, one `PlayerSetSpawnEvent` and one `PlayerWakeUpEvent`
   line. With Traveler's Backpack equipped nothing changes visibly.
2. Mine a stone block in a new world: the "Stone Age" toast; one Earn line (Lootr and architectury hear it).
3. `/gamerule keepInventory true`: the chat confirmation as before and one `GameRuleChangedEvent` line.
