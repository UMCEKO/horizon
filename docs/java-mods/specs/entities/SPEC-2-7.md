# SPEC-2-7: what a mob's tick does happens in its turn

Index: `SPEC-2-8.md` (E3). Size: ~220 lines. Needs 2-6. **Changes behaviour toward vanilla.**

## Why
Vanilla applies everything a mob's `tick()` causes inside that call: a melee hit lands, its victim may die and drop loot, a projectile is added, a sound plays, all before the next entity ticks. Native collects them during the pass and applies them after every mob has ticked (`end_pass`: `mob_died` of `dead`, `effects_on_death` of `corpses`, the `gone` poofs, `handle_mob_events(events)`, `lan_guest_mob_events`). Consequences today:
- a mob killed by an earlier mob in the same tick still ticks;
- a projectile shot by mob A isn't in the level when mob B, later in the list, looks for projectiles;
- with events (2-16, phase 4), damage and death events would fire after every `EntityTickEvent.Post` of the tick, not between the attacker's Pre and Post.

## Change (`game/tick_pass.rs`, `creatures.rs`)
At the end of `turn_mob`'s body, after the mob is back in `self.mobs` and **before** the `MobAfter` crossing, in this order (vanilla's order inside the tick: the mob's own effects as its code produced them, then its death's consequences, then its removal):
1. `self.handle_mob_events(pass.events.drain(from..))` for the events this turn produced (`from` is `events.len()` before the tick, as now), then the guest-bound ones of this turn through `lan_guest_mob_events`. Keep the shulker-bullet target rewrite as it is, before.
2. If the mob died in its own body (`dead` today): `mob_died(&m)` now.
3. If its body is removed this turn (`death_time >= 20`, `corpses`/`gone` today): `effects_on_death` with its effects, then entity event 60 and `death_poof`, then `level_remove(Kind::Mob, id, Killed)`. That is `LivingEntity.tickDeath` → `makePoofParticles` → `remove(KILLED)` → `Entity.setRemoved`, which runs the mob effects' `onMobRemoved` (vanilla `LivingEntity.remove` with KILLED: check the order of `onMobRemoved` against `makePoofParticles` in `LivingEntity.tickDeath` and `remove`, and follow it).
4. The outbox of mobs hit during this turn (`m.outbox` of every mob touched by step 1) is handled now, not at the pass end: `handle_mob_events` of each touched mob's outbox, in the order the hits landed.
5. `refresh_pass` for every mob step 1–4 touched: their snapshot slot and grid, so the next mob sees a victim's new health or death. Native APIs that hurt, move or kill a mob during `handle_mob_events` call `note_touched(Some(id))`; add the call where it is missing (grep `fn damage`, `fn kill_target`, knockback) and list them in your report.

`end_pass` keeps only what is not a mob's own effect: pushes for every mob, the random, bindings, `tick_passengers`, and the after-pass calls 2-8 moves. The "killed since the last tick die first" prelude of `begin_pass` stays until phase 4 (death at the killing blow) deletes it.

## Cost
`handle_mob_events` is now called once per mob with a few events instead of once per pass. If it rebuilds anything per call (lookups, views), hoist that into `TickPass` and say what in your report. Report `tick_mobs` before and after with the 360-mob scenario.

## Tests (`game_tests`)
- `a_mob_killed_by_an_earlier_mob_does_not_tick_after` (two mobs adjacent; the first kills the second this tick; the second's `age` is unchanged).
- `a_projectile_shot_this_turn_is_in_the_level_before_the_next_mob` (a skeleton then a mob whose test hook records `self.creatures.projectiles.len()` at its `MobBefore`).
- `death_loot_drops_in_the_killers_turn` (the item entity exists at the next mob's `MobBefore`).
- `python3 tools/parity.py mob_ai server_entities server_player_damage`, and every harness `--changed-files` lists for `creatures.rs` and `tick_pass.rs`. A moved number is accepted only if the oracle (vanilla) agrees with the new value: name each in the report.
- `python3 tools/lan_e2e.py --check chest` and the mob checks `--list` shows (zombie, skeleton, creeper).

## Parent check
Scenario `tools/scenarios/mob_turns.json`: floor, `tp`, `near`; summon 5 skeletons and 5 zombies facing each other in a closed pen, `wait 400`, `check {"no_exceptions": true}`; compare the survivors' count with the same scenario on the base build (it may differ; report both).
