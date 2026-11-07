# SPEC-2-9: one turn per entity for items, orbs and the flying kinds

Index: `SPEC-2-8.md` (E3). Size: ~300 lines, mostly moved code. Needs 2-6 (the pattern) and 2-2, 2-3. **No behaviour change.**

## Pattern (the same as `turn_mob`, 2-6)
For each loop below:
1. Split the loop into `fn turn_<kind>(&mut self, pass: &mut TickPass, id: u32) -> Result<()>` (the per-entity body) and a loop `for id in ids_at_start { self.turn_<kind>(pass, id)? }`. The loop stays where it is today in `tick_level` / `end_pass`; 2-13 replaces the loops.
2. The turn looks its entity up by id (a per-kind index cache in `TickPass`, invalidated by a per-kind generation bumped in `level_add`/`level_remove`), returns if it is gone or `removed`, and takes **only that entity** out of its vector around the body when the body needs `&mut` to it and `&self` at once (`with_<kind>_outside`, modelled on `with_mob_outside`). It is back before any crossing.
3. A removal decided in the body is `level_remove(kind, id, why)` once the entity is back. The kind's `Lent` arm, `left_in_loop` use and the final `retain(!removed)` go.
4. Whatever the loop computed once before its first entity (player lists, `fight_players()`, the targets a hook can catch) becomes a lazily built field of `TickPass` (`Option<…>`, filled on first use, cleared by `refresh_pass` when `Touched.shared`).
5. Crossings become `cross_point(point, &mut [])` then `refresh_pass`.

## Loops
| Loop | Turn | Notes |
|---|---|---|
| `tick_items` (`game.rs`) | `turn_item` | the merge scan reads `self.dropped` live (vanilla `ItemEntity.mergeWithNeighbours` sees the level as it is) |
| `tick_experience` | `turn_orb` | the same for orb merging |
| `tick_projectiles` (`creatures.rs:3097`, called from `end_pass`) | `turn_projectile` | keep the guest `pvp` owner handling (1c2d `Owner::who`); `touch_arrows` is **not** a projectile turn: it is `Player.aiStep` and stays with the players |
| `tick_thrown_potions` (`potions.rs:913`) | `turn_potion` | the landed list it collects is applied inside the turn (`splash` at the hit), as `ThrownSplashPotion.onHit` does |
| `tick_tridents` (`trident.rs:517`) | `turn_trident` | |
| `tick_rockets` (`fireworks.rs:274`) | `turn_rocket` | |
| `tick_fangs` (`fangs.rs:72`) | `turn_fangs` | |
| dragon fireballs (in `dragon.rs`) | `turn_dragon_fireball` | |
| `tick_potion_clouds` (`potions.rs:967`) and dragon breath ticking | `turn_cloud` | one turn for both cloud uses: they are one class |
| eyes of ender (`end.rs`) | `turn_eye` | |

`tick_effects` (`potions.rs:750`) mixes the local player's effects with the thrown potions: its potion part becomes the `turn_potion` loop; the player part stays.

## Tests
- `crossing_tests.rs`: the item and orb tests pass unchanged; add `a_projectile_turn_takes_only_its_projectile_out`.
- Harnesses: `python3 tools/parity.py --changed-files <files> --list`, then run them (expect `item_entity`, `experience_orb`, `arrow`, `trident`, `firework`, `potion`, `evoker`, `dragon`): identical.
- `python3 tools/lan_e2e.py --check chest` and the projectile checks `--list` names.
- Timing of `tick_items` and `tick_projectiles` spans with the 360-mob scenario: within ±3%.
