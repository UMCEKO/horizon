# SPEC-2-3: dragon fireballs, area effect clouds, evoker fangs, ominous item spawners, thrown potions and eyes of ender become kinds

Index: `SPEC-2-8.md` (E2). Size: ~300 lines. Same pattern as `SPEC-2-2.md` (read it and `SPEC-1c2a.md` §1 first). Independent of 2-2 and 2-4; the three touch `Kind` in turn, so merge them one at a time.

## Vectors
| Storage | Vanilla class | New `Kind` | Where |
|---|---|---|---|
| `Game.dragon_fireballs: Vec<dragon::DragonFireball>` | `DragonFireball` | `DragonFireball` | `dragon.rs` |
| `Game.breath_clouds: Vec<dragon::Breath>` (dragon breath **and** lingering potions' clouds, `potions.rs:967`) | `AreaEffectCloud` | `Cloud` | `dragon.rs`, `potions.rs` |
| `Game.fangs.list: Vec<fangs::Fangs>` | `EvokerFangs` | `Fangs` | `fangs.rs` |
| `Game.ominous_spawners: Vec<OminousItemSpawner>` | `OminousItemSpawner` | `Spawner` | `ominous_item_spawner.rs`, `raids.rs`? (grep) |
| `PotionState.thrown: Vec<ThrownPotion>` (`potions.rs:321`) | `ThrownSplashPotion`, `ThrownLingeringPotion` | `Potion` | `potions.rs` |
| `EndState.eyes: Vec<EyeOfEnder>` (`end.rs:145`) | `EyeOfEnder` | `Eye` | `end.rs` |

`PotionState.clouds` is the **client's** tracked copy (`RemoteCloud`); it stays as is.

## Changes, per vector
1. `Kind` arm, `NewEntity` arm, `level_add` push, `level_remove`/`level_remove_all`/`level_ids` arms, `ALL_KINDS`, `removed: bool` where missing, a `LentKinds` bit if a loop lends it.
2. Id at construction (`next_entity += 1`); delete its line from `lan_assign_ids` (`thrown_ids()`, `eye_ids()`, `breath_clouds`, `dragon_fireballs`, `ominous_spawners`; fangs have their own counter at `fangs.rs`: replace it with `next_entity`).
3. Every push, `retain`, `clear` and `take` through `level_add`/`level_remove`/`left_in_loop`, with reasons:
   - fireball hitting (`DragonFireball.onHit` → `discard`), cloud expired or empty (`AreaEffectCloud.tick` → `discard`), fangs after their 22 ticks (`EvokerFangs.tick` → `discard`), spawner done (`OminousItemSpawner.tick` → `discard`), potion hit (`ThrownSplashPotion.onHit` → `discard`), eye's end (`EyeOfEnder.tick` → `discard`, after it drops or shatters): `Discarded`;
   - unload and world close: `UnloadedToChunk`.
4. `tests/test_entity_lifecycle_sites.py`: add the six storages to `VECTORS` (the two inside structs as `potions\.thrown`, `end\.eyes`, `fangs\.list`).
5. `commands::Target` arms (`DragonFireball`, `Cloud`, `Fangs`, `Spawner`, `Potion`, `Eye`) at every `Target::Tnt` site.
6. Saving: vanilla saves all six. `save_entities` writes each through `entity_nbt.rs` encoders (port the `addAdditionalSaveData` of each class; add a decoder to `spawn_loaded_entities` for each id). `EvokerFangs`: `Warmup`, `Owner`. `OminousItemSpawner`: `spawn_item_after_ticks`, `item`. `AreaEffectCloud`: `Age`, `Duration`, `WaitTime`, `ReapplicationDelay`, `DurationOnUse`, `RadiusOnUse`, `RadiusPerTick`, `Radius`, `Owner`, `custom_particle`, `potion_contents`, `potion_duration_scale`. `DragonFireball` and potions: `Projectile` keys (`Owner`, `HasBeenShot`, `LeftOwner`) plus `Item` for potions, `power` for the fireball (`AbstractHurtingProjectile`). `EyeOfEnder`: `Item`.
   Native that doesn't save one today now does: name each in your report (a P1 fix: vanilla keeps them across a reload).

## Tests
- `game_tests`: one join/leave test per kind, with its reason (`a_cloud_leaves_discarded_when_its_duration_ends`, `fangs_leave_after_22_ticks`, …).
- `cargo test -p horizon-client --bins entity_nbt` round trips for the six encoders.
- `python3 -m unittest tests.test_entity_lifecycle_sites`.
- Harnesses `--changed-files` lists (expect `potion`, `dragon`, `evoker`, `ominous`, `eye_of_ender`, `server_entities`); unchanged.
- Scenario: extend `tools/scenarios/kinds.json` with `/summon area_effect_cloud`, `/summon evoker_fangs`, `/summon ominous_item_spawner`; each logs `joined`, and the cloud and fangs log `left DISCARDED` within 200 ticks.
