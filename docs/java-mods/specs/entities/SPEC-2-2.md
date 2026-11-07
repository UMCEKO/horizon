# SPEC-2-2: falling blocks, tridents and rockets become kinds; ids at construction

Index: `SPEC-2-8.md` (E2). Size: ~280 lines. Needs the 1c branch and SPEC-1d. The pattern is `SPEC-1c2a.md` §1 (TNT and bolts), applied to three more vectors; read it first.

## Why
`Game.falling: Vec<FallingBlock>` (game.rs:1437), `tridents: Vec<trident::ThrownTrident>` and `rockets: Vec<fireworks::Rocket>` never go through `level_add`/`level_remove`. Their ids stay 0 until `lan_assign_ids` (`lan_entities.rs:430`) hands one out at first tracking, so they never join, have no view, and can't have a place in the tick list (2-5). Vanilla gives every entity its id in the `Entity` constructor (`ENTITY_COUNTER.incrementAndGet()`).

`lan_assign_ids` also gives ids late to `creatures.projectiles` and `crystals`, which are kinds already: a projectile can join with id 0. This step fixes those two as well.

## 1. `entity_life.rs`
- Append `Falling`, `Trident`, `Rocket` to `Kind`, after `Bolt`; `NewEntity::Falling(FallingBlock)`, `Trident(trident::ThrownTrident)`, `Rocket(fireworks::Rocket)` with their `kind()` and `id()` arms.
- `level_add` pushes into the vectors; `level_remove`, `level_remove_all`, `level_ids` get the arms (`take!` with a new `LentKinds` bit for each kind any loop lends).
- `java_entities.rs` `ALL_KINDS` grows to 18. Each struct gains `pub removed: bool` where it lacks one.

## 2. Ids at construction
Every constructor site does `self.next_entity += 1; id = self.next_entity` before building, for `FallingBlock`, `ThrownTrident`, `Rocket`, `Projectile` and `Crystal`. Find the sites with the compiler: make the `id` argument of each `new`-like constructor mandatory, or grep `FallingBlock {`, `ThrownTrident {`, `Rocket {`, `Projectile::new`, `Crystal {`.
- Remote copies (`remote_projectiles.rs`, `remote_objects.rs`) keep the server's id.
- Delete those five lines from `lan_assign_ids`. Drop the `filter(|t| t.id != 0)` in `lan_projectile_states` for them.

## 3. Sites through `level_add` / `level_remove`
The site test (`tests/test_entity_lifecycle_sites.py`) gains `falling|tridents|rockets` in `VECTORS` and lists the rest; today's direct mutations are in `game.rs` (7), `fireworks.rs`, `trident.rs`, `persist.rs`, `remote_projectiles.rs`, `remote_objects.rs`, `remote.rs`, `dispense_fixture.rs`.

| Native removal | Vanilla | Reason |
|---|---|---|
| A falling block that lands, breaks, or exceeds its time | `FallingBlockEntity.tick` → `discard()` | `Discarded` |
| A trident picked up, or its owner gone without loyalty | `AbstractArrow.tryPickup` / `ThrownTrident.tick` → `discard()` | `Discarded` |
| A rocket that explodes | `FireworkRocketEntity.explode` → `discard()` | `Discarded` |
| Chunk unload, world close | `UNLOADED_TO_CHUNK` | `UnloadedToChunk` |

- Removals decided inside the kind's own loop use `left_in_loop` with the lent vector, as `tick_bolts` does (1c2a §1.3). Each loop becomes an index loop over the taken vector with a fixed bound and a final `retain(!removed)`, as `explosion.rs` 540–546.
- Chunk loads (`persist.rs`) pass `FromDisk` and the tag.
- Dispense fixture lines stay (fixture).

## 4. Saving, commands, views
- `persist.rs` `save_entities` already writes falling blocks; add tridents (`AbstractArrow` + `ThrownTrident` keys: `Trident` item, `DealtDamage`) and rockets (`Life`, `LifeTime`, `FireworksItem`, `ShotAtAngle`) if missing, through `entity_nbt.rs` encoders and decoders, round-trip tested. Vanilla saves all three.
- `commands::Target` gains `Falling(u32)`, `Trident(u32)`, `Rocket(u32)`, handled at every `Target::Tnt` site (1c2a §1.5's list): candidates, position, type id, NBT, kill, kind, Java id, teleport.
- `java_new_entity_tag` arms with the encoders above. Their shape needs no species group in this step.

## Tests
- `game_tests`:
  - `a_falling_block_joins_fresh_and_leaves_discarded_when_it_lands`;
  - `a_trident_thrown_has_an_id_before_any_tracking`;
  - `a_rocket_leaves_before_its_explosion_damage` (the leave hook runs before the first `hurt` of the blast: vanilla `explode` discards first? check `FireworkRocketEntity.explode` and assert vanilla's order);
  - `a_projectile_joins_with_a_nonzero_id`.
- `cargo test -p horizon-client --bins entity_nbt` (trident and rocket round trips).
- `python3 -m unittest tests.test_entity_lifecycle_sites`.
- Harnesses: what `python3 tools/parity.py --changed-files … --list` names for the touched files (expect `falling_block`, `trident`, `firework`, `arrow`, `server_entities`). Unchanged results.
- `python3 tools/lan_e2e.py --check trident` and any falling or firework check `--list` shows.

## Parent check
`tools/scenarios/kinds.json` (new): floor, `tp`, `near`; `/setblock ~ ~10 ~3 sand`, `/summon firework_rocket ~ ~1 ~`, `/summon trident ~2 ~5 ~`; `wait 5`; `check {"log": "entity \\d+ minecraft:(falling_block|firework_rocket|trident) joined"}` with `HORIZON_JAVA_DEBUG=1` (1d's line); `wait 60`; `check {"no_exceptions": true}`.
