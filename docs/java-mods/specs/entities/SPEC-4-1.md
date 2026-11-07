# SPEC-4-1: a living entity dies at the killing blow

Design: `DESIGN.md` §4 (`LivingDeathEvent` at `LivingEntity.die`). Index: `SPEC-2-8.md`. Size: ~250 lines. Needs phase 2. Rust only. **Changes behaviour toward vanilla.**

## Why
Vanilla `LivingEntity.hurtServer` calls `die(source)` **inside** the hurt, right after the health reaches 0 (`if (this.isDeadOrDying()) { if (!checkTotemDeathProtection(source)) { … this.die(source); } }`). `die` does the death message, `LivingDeathEvent` (patched), the kill credit, `dropAllDeathLoot` (drops, `LivingDropsEvent`), `createWitherRose`, the sculk catalyst, `setPose(DYING)`. Native defers it:
- mobs killed outside their own turn die "first" at the next pass (`begin_pass`'s prelude, kept by 2-7);
- mobs killed in their own body die at the end of their turn (2-7);
- the drops therefore appear a tick late in the first case, and every death event would fire at the wrong place.

## Change
- `fn die(&mut self, victim: Target, source: &Source)` in `game/combat.rs` (or wherever `mob_died` lives: `creatures.rs:1120`): today's `mob_died` body, in `LivingEntity.die`'s order (read it: `ServerLevel` check, `isRemoved`/`dead` guard, the killer's `awardKillScore`, `dead = true`, `getCombatTracker().recheckStatus()`, `level.broadcastEntityEvent(3)`, `dropAllDeathLoot`, `createWitherRose`, `setPose(DYING)`; then subclass overrides: `TamableAnimal`, `Fox`, `Villager`, `WitherBoss`, `Player`/`ServerPlayer` …). `self.died` stays the `dead` flag.
- Every native path that sets a living entity's health to 0 calls `die` at that point:
  - the mob hurt path (`damage`, `actuallyHurt` port), from any attacker: player attack, mob melee (`handle_mob_events`), projectiles, explosions, `/kill` (`kill_target` → `Entity.kill` → `hurtServer(genericKill)`), fall and fire damage inside the mob's own lib tick;
  - for damage inside the lib tick (fall, drowning, burning, suffocation: `m.tick` changes `m.health`): the lib records `Died { source }` in the turn's events at the hurt, and the turn's event handling (2-7) calls `die` — the first effect processed after that hurt, before the rest of the turn's events. The lib can't call the bin; this keeps the order of everything the bin does.
- Delete `begin_pass`'s "killed since the last tick die first" prelude and 2-7's `dead` list.
- Players (guests) already die in `guest_die` at the hurt (`lan_actions.rs`): check it is at the hurt, not at the next guest tick, and fix if not.
- Totems: `checkTotemDeathProtection` runs before `die` in the same place; native's totem code moves with it.

## Tests (`game_tests`)
- `a_mob_killed_by_the_player_drops_its_loot_in_the_same_tick`;
- `a_mob_killed_by_an_arrow_dies_in_the_arrows_turn`;
- `a_mob_that_falls_to_death_dies_inside_its_turn_before_its_later_events`;
- `a_totem_saves_before_die_runs`.
- Harnesses `--changed-files` lists (expect `mob_ai`, `server_player_damage`, `mob_drops`, `death_messages`, `sculk`): moved numbers only with the oracle agreeing.
- `python3 tools/lan_e2e.py --check` the combat checks `--list` names.
