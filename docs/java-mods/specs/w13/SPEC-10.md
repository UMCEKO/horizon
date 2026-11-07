# SPEC-10 (W13): `PlayerXpEvent.PickupXp`, `PlayerXpEvent.XpChange`, `PlayerXpEvent.LevelChange`

Design: `DESIGN.md` §1.3 (travelersbackpack, Clumps, puzzleslib). Read first: SPEC-1, `G/lan_actions.rs` `guest_orbs`
(~5561–5670), `G/commands.rs` (`xp_change`, `native_player_level_changed`, the `/xp` command near line 3357),
`G/game.rs` `give_orb_points`, `G/stations.rs` `give_levels`, `crates/horizon-client/src/entities.rs` (`Experience::give`,
`needed`, `LevelUpSound`), `B/EntityViews.java` (an orb's view by id), the patched bytecode of
`ExperienceOrb.playerTouch`, `Player.giveExperiencePoints`, `Player.giveExperienceLevels`.

Depends on SPEC-1 and W9 SPEC-1c (live views of native entities; the orb's view by id). If W9's views don't cover
experience orbs yet, this spec waits for that, it doesn't build its own. About 300 changed lines.
Code comments: one line at most, only for a non-obvious why.

## Vanilla + NeoForge
- `ExperienceOrb.playerTouch(player)`: if `player.takeXpDelay == 0`: post `PlayerXpEvent.PickupXp(player, orb)`;
  canceled → return (no delay set, the orb stays); else `takeXpDelay = 2`, `take`, `repairPlayerItems` (mending),
  `giveExperiencePoints(rest)`, `count--`, discard at 0.
- `Player.giveExperiencePoints(amount)`: post `XpChange(player, amount)`; canceled → return; `amount =
  event.getAmount()`; then progress, total, and per level crossed (`while progress >= 1`): `progress = (progress - 1) *
  needed`, `giveExperienceLevels(1)`, `progress /= needed` (and the negative loop with `giveExperienceLevels(-1)`).
- `Player.giveExperienceLevels(levels)`: post `LevelChange(player, levels)`; canceled → return; `levels =
  event.getLevels()`; `experienceLevel += levels` (reset to 0 below 0), the level-up sound rule. Also reached directly
  from `/xp add <n> levels`, enchanting and anvils (the level costs, negative).

## Wire (`player_event`)
- id 18 `PickupXp`: `int orb id`. Result empty; `canceled`.
- id 19 `XpChange`: `int amount`. Result `int amount`.
- id 20 `LevelChange`: `int levels`. Result `int levels`.
The use header carries the player's xp as it is at the call (level, progress, total), so each site must have written its
current experience into `self.experience` before crossing; the reply's xp block is ignored for these three (they change
no xp in Java; native applies the result).

## Java (`ServerEvents`)
- case 18: `var orb = (ExperienceOrb) EntityViews` lookup by id (throw `IllegalStateException` if absent: a Horizon
  bug); `canceled = NeoForge.EVENT_BUS.post(new PlayerXpEvent.PickupXp(call.player, orb)).isCanceled()`.
- case 19: `var e = NeoForge.EVENT_BUS.post(new PlayerXpEvent.XpChange(call.player, amount));` canceled, result
  `int e.getAmount()`.
- case 20: the same with `LevelChange` and `getLevels()`.
- Debug lines: `Horizon: server event PlayerXpEvent.PickupXp orb=<id> canceled=<b>`, `.XpChange <in>-><out>
  canceled=<b>`, `.LevelChange <in>-><out> canceled=<b>`.

## Rust
- `java_hooks.rs` pairs (each returns its input untouched when not listened or without the feature):
  - `java_hook_pickup_xp(&mut self, orb: u32) -> bool` (true = canceled);
  - `java_hook_xp_change(&mut self, amount: i32) -> Option<i32>` (`None` = canceled);
  - `java_hook_level_change(&mut self, levels: i32) -> Option<i32>`.
- `guest_orbs`: compute `listened` as today **or** `java_listens(PickupXp)`; inside the same `as_guest` call, after
  `native_experience_orb_pickup` and only if it didn't cancel: `canceled = game.java_hook_pickup_xp(orb.id)`. Nothing
  else moves: on cancel the function returns before the delay, as today.
- `xp_change` (`commands.rs`): today it returns early when no native mod is loaded. Make it: native mods' hook (only when
  `mod_api::mods()` isn't empty, as now), then `self.java_hook_xp_change(amount)`; the native early-return must not skip
  the Java hook. Every `giveExperiencePoints` site already goes through `xp_change` (`give_orb_points`, `/xp` points,
  advancement rewards): check with `grep -n "experience.give(" -r crates/horizon-client/src/game*` that none bypasses it;
  route any that does through it.
- Level changes: replace `Experience::give`'s loop at the `Game` level when `LevelChange` is listened.
  - In `entities.rs`, split `Experience::give` into `add_points(&mut self, points)` (progress and total) and
    `fn next_level_up(&mut self) -> bool` (when `progress >= 1`: `progress = (progress - 1) * needed(level)`, returns
    true; the caller then adds the level and divides `progress` by the new `needed`). `give` keeps its behaviour on top
    of them.
  - New `Game` fn `give_experience_points(&mut self, gained: i32)` used by `give_orb_points` and the other
    `xp_change` callers in place of `self.experience.give(...)`: when `java_listens(LevelChange)` is false, it calls
    `give` exactly as today; when true, it runs `add_points`, then `while self.experience.next_level_up()`: `let levels =
    self.java_hook_level_change(1)`; on `Some(n)`: `self.experience.level += n` (clamped at 0 as vanilla), the level-up
    sound rule (`play_level_up(n, level)`); then `progress /= needed(level)` in both cases (vanilla divides after the
    call either way).
  - `/xp add <n> levels` and `stations.rs` `give_levels` (enchanting, anvils): call `java_hook_level_change(n)` first;
    `None` → nothing changes; `Some(m)` → apply `m`.
- `JavaEvent::PickupXp`, `XpChange`, `LevelChange` into `SITED`.

## Tests
- Rust (`entities.rs`): `split_give_equals_give`: for amounts 1, 7, 100, 10000 from levels 0, 14, 29, 31, `add_points` +
  the `next_level_up` loop with +1 per level gives the same `(level, progress, total)` as `give` (bit-equal floats; the
  expected values come from `give`, which the XP harnesses pin against vanilla).
- `a_canceled_level_up_keeps_the_level_and_spends_the_progress`: the same loop where the hook answers `None`: level
  unchanged, progress as vanilla leaves it (reduced, then divided by `needed(level)`). Hand-computed for 20 points at
  level 0, every level-up canceled (needed 7 throughout): 20/7 → (20/7 - 1) * 7 = 13 → 13/7 → (13/7 - 1) * 7 = 6 → 6/7;
  so level 0, progress 6/7 (within 1e-6), total 20.
- Test mod (`ChecksServerEvents`): listeners: `PickupXp` cancels for orbs whose value is 3; `XpChange` doubles amounts;
  `LevelChange` records. `drive`: `/summon minecraft:experience_orb ~ ~ ~ {Value:3}` and `{Value:5}` at the player.
  Checks: `pickup_cancel_keeps_orb` (a value-3 orb still exists after 40 ticks), `xp_change_doubles` (total experience
  rose by 10 from the value-5 orb), `level_change_fires_on_level_up` (one `LevelChange(+1)` after `/xp add @s 7 points`
  from 0).

## Done bar
- `cargo test -p horizon-client --lib entities`, `cargo test -p horizon-client --features java-mods --lib java_events`,
  clippy both ways, fmt. The XP harness (`tools/parity.py --changed-files` for `entities.rs`, at most two) unchanged.
- `bridges` CHECKs pass. M1 `w13` step `xp`: `PlayerXpEvent$PickupXp`, `$XpChange`, `$LevelChange` equal Forge's where
  listened.
- Append the three debug-line checks to `tools/scenarios/w13-events.json`.

## Live test (lead, showcase jars, `HORIZON_JAVA_DEBUG=1`)
1. Kill a few mobs and pick up the orbs: XP rises as before; Clumps merges orbs (one orb with a big value); one
   `PickupXp` line per orb taken, `XpChange` after each.
2. Enchant an item at a table: the levels go down by the cost (`LevelChange -n`).
3. Reach level 5 from orbs: the level-up sound plays once.
