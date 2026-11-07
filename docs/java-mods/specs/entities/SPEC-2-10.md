# SPEC-2-10: one turn per entity for every other kind

Index: `SPEC-2-8.md` (E3). Size: ~300 lines, mostly moved code. Needs 2-9 (same pattern) and 2-4. **No behaviour change.**

Apply `SPEC-2-9.md`'s pattern to:

| Loop (file) | Turn | Notes |
|---|---|---|
| `tick_tnt` (`tnt.rs:196`) | `turn_tnt` | already re-entrant by id (1c2a); just factor |
| `tick_falling` (`game.rs`) | `turn_falling` | |
| `tick_bolts` (`lightning.rs:327`) | `turn_bolt` | the `thunder_hit` lend goes: the vector is in `Game` |
| `tick_boats` (`boats.rs:544`) | `turn_boat` | "every boat but the ridden one": keep that exclusion as it is; 2-14 replaces it |
| `tick_carts` (`minecarts.rs:824`) | `turn_cart` | the same |
| `tick_armor_stands` (`armor_stands.rs:678`) | `turn_stand` | |
| `tick_item_frames` (`frames.rs:345`), `tick_paintings` (`paintings.rs:255`) | `turn_frame`, `turn_painting` | `BlockAttachedEntity.tick`'s 100-tick check |
| `tick_cushions` (`cushions.rs:424`) | `turn_cushion` | |
| end crystals (`dragon.rs`) | `turn_crystal` | |
| `tick_ominous_item_spawners` (`ominous_item_spawner.rs:101`) | `turn_spawner` | its `tickClient` particles: server `Game`s don't draw; keep the existing gate |
| `others` (markers, displays, interactions…) | `turn_other` | whatever native ticks for them today; a kind native doesn't tick has an empty turn (vanilla `Marker.tick` is empty) |
| the dragon (`dragon.rs`) | `turn_dragon` | `EnderDragon.tick`/`aiStep`; `EnderDragonFight.tick` is **not** the dragon's turn (2-13 runs it before the loop) |
| `tick_fishing` (2-4's loop) | `turn_hook` | the body still runs as its owner (`as_guest`) |
| knots (2-4) | `turn_knot` | the 100-tick `survives` check |
| boats' and mobs' leads (`tick_leads`, the boat part) | inside `turn_boat` | `Leashable.tickLeash`, as 2-8b did for mobs |

The local player's mount (`position_rider`, `mounts.rs:399`) and the "ridden boat ticks with its rider" paths are riding: 2-14.

## Tests
- `crossing_tests.rs`: `a_tnt_turn_sees_every_tnt_in_place`, `a_boat_turn_takes_only_its_boat_out`.
- Harnesses `--changed-files` lists (expect `tnt`, `falling_block`, `lightning`, `boat`, `minecart`, `armor_stand`, `item_frame`, `painting`, `dragon`, `fishing`, `leash`): identical.
- `python3 tools/lan_e2e.py --check boat --check boats --check minecarts --check fishing`.
