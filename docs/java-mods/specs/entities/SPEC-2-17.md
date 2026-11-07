# SPEC-2-17: `MobDespawnEvent` and `LivingBreatheEvent`

Index: `SPEC-2-8.md` (E4). Size: ~260 lines. Needs 2-16 (the event op pattern).

## 1. `MobDespawnEvent`
**Vanilla, patched** (javap of `Mob.checkDespawn`): the first instruction is `if (EventHooks.checkMobDespawn(this)) return;`, then vanilla's body. `WitherBoss`, `EnderDragon` and `ShulkerBullet` override `checkDespawn` without `super`: no event for them.

**Rust:** in 2-13's `check_despawn`, Mob arm (not Wither):
```rust
if self.java_hook_mob_despawn(id) { return; }   // the hook handled it
horizon_client::mobs::ai::despawn_check(…)      // vanilla's body, as now
```
`java_mob_despawn(id) -> bool`: `hook_admits(Hook::MobDespawn, type)` bit first; then op `mob_despawn` (`int id`, `int mask`, groups); reply `bool handled` + effects.

**Java:** `mob_despawn` → `safely("MobDespawnEvent", () -> EventHooks.checkMobDespawn((Mob) view))`. Whatever the hook writes on the view (2-1's `mob_despawn` writes; expect `Entity.discard()V` and a no-action-time setter) reaches native through bridges: add each missing bridge 2-1 lists, in `G/java_bridges/entities.rs`, with a refresh of its group. A discard inside the hook removes the mob at once (`level_remove`); 2-13's detached-turn rule then applies: it still ticks this turn if in range.

## 2. `LivingBreatheEvent`
**Vanilla, patched** (javap of `LivingEntity.baseTick`, offsets 205–231): vanilla's air block (`LivingEntity.java:441–463`) is replaced by
```java
int air = getAirSupply();
CommonHooks.onLivingBreathe(this, level, air - decreaseAirSupply(air), increaseAirSupply(air) - air);
```
after the in-wall and border damage and before `lastPos`/`onChangedBlock`. `decreaseAirSupply` runs **every** tick there (it draws from the entity's random when its oxygen bonus is above 0), where vanilla calls it only underwater.

**The rule (E4):** for an admitted subject, native skips its own air block and Java runs the hook, which does the air logic on the view (its writes bridged). For others, native runs vanilla's block as today.

**Rust, mobs** (`mobs/ai.rs:847` `base_tick`, inside the lib's `Mob::tick`):
- Split the lib tick at the air block: `Mob::tick_head(&mut self, w, rng, events) -> Head` (everything up to and including the in-wall/border damage), `Mob::breathe(&mut self, w, rng)` (today's air block, unchanged), `Mob::tick_tail(&mut self, w, rng, events, head) -> bool` (the rest). `Mob::tick` stays as `head; breathe; tail` for every caller that doesn't cross.
- `turn_mob`: if `hook_admits(Hook::LivingBreathe, type)`: `head`; compute `consume = air - decrease_air_supply(air)` (with the mob's random, as the patched code) and `refill = increase_air_supply(air) - air`; put the mob back; op `living_breathe` (`int id`, `int consume`, `int refill`, `int mask`, groups); take it out again and rebuild `w`; `tail`. Otherwise one `tick` as now.
- **Armor stands** (`turn_stand`) and **players** (the guest's `doTick` air code: find it with `port_ledger.py where LivingEntity.baseTick`, `motion/environment.rs`) get the same site: native air block or the crossing. If a kind has no native air today (check armor stands), add the air field and vanilla's block for it (ledger: `LivingEntity.baseTick` lists its ports).

**Java:** `living_breathe` → `safely("LivingBreatheEvent", () -> CommonHooks.onLivingBreathe(view, level, consume, refill))`, then effects. Bridges for each writer 2-1 lists under `living_breathe` (expect `Entity.setAirSupply(I)V`, `hurtServer` with the drown source, `Level.broadcastEntityEvent(Entity, B)V` → native entity event 67 to trackers, `stopRiding`): add the missing ones, each refreshing its group. The view's reads (2-1's `hook_reads`) are in the mask: if a read group native doesn't write yet (eye-in-fluid, say), add it to `write_groups`/`EntityViews.apply` as a new bit, with its native source.

Events the hook posts besides `LivingBreatheEvent` (2-1's `events`, `LivingDrownEvent` if it is there) are covered by the same crossing: `hook_admits` ORs them.

## Test mod
- Listeners: `LivingBreatheEvent` sets `canBreathe` true for entities tagged `horizon_gills`; `MobDespawnEvent` (signatures: `javap -p net.neoforged.neoforge.event.entity.living.MobDespawnEvent`) sets DENY for tag `horizon_keep` and ALLOW for tag `horizon_go`.
- Checks (`later`):
  - `breathe_event_keeps_air`: a zombie tagged `horizon_gills` held underwater (a water column, `NoAI`) has full air after 400 ticks; an untagged one beside it is drowning (air ≤ 0, health below max);
  - `despawn_deny_keeps_a_far_mob`: a zombie tagged `horizon_keep` 140 blocks from every player is still there after 40 ticks;
  - `despawn_allow_removes_a_near_mob`: a persistent-looking zombie tagged `horizon_go` next to the player is gone within 2 ticks, and its leave reason is `DISCARDED`.

## Scenario (parent): the hook equals native
`tools/scenarios/breathe.json`: a `NoAI` zombie in a water column, `wait 200`, `data get entity @e[type=zombie,limit=1] Air`; run it twice, once with `HORIZON_TEST_BREATHE=1` (the test mod then registers a do-nothing `LivingBreatheEvent` listener, so the hook runs in Java) and once without. Both `Air` values and the zombie's health match.

## Tests to run
- `cargo test -p horizon-client --lib mobs` (the head/breathe/tail split equals `tick`: `tick_split_equals_tick` over a drowning and a breathing mob).
- `python3 tools/parity.py mob_ai server_player_damage drowning` (or the air harnesses `--changed-files` names): unchanged.
- `java_mods.py build`, `check --analysis`, `bridges`.
- Profile: `living_breathe` and `mob_despawn` calls per tick with the pack (expect 0: nobody listens in the pack).
