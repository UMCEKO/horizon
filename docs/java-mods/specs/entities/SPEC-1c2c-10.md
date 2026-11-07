# SPEC-1c2c-10: passenger trees of any kind, saved and loaded

Index: `SPEC-1c2c.md`. Size: ~250 lines. Needs step 9.

## Why
Vanilla saves a passenger inside its root vehicle:
- `Entity.saveAsPassenger` writes `Passengers` from `getPassengers()`, for each passenger with `saveAsPassenger` true;
- section storage saves only root entities;
- `EntityType.loadEntityRecursive` rebuilds the tree.

Native does this for mobs and boats only (`persist.rs` `mob_tree_nbt`, `take_mob_passengers`, after step 6), so an armor stand on a boat comes back apart. `/summon x {Passengers:[…]}` uses the same loader.

## Change (`game/persist.rs`, `entity_nbt.rs`)
**Save:**
- `fn entity_tree_nbt(&mut self, e: Ent) -> Option<Tag>`: the kind's own tag (the per-kind encoders `save_entities` uses), plus `Passengers` = `entity_tree_nbt(p)` for each passenger in order.
- Skip a passenger whose `saveAsPassenger` is false:
  - players (`Player` types don't serialize);
  - a removed entity;
  - kinds whose `shouldBeSaved` is false.
- `save_entities` writes only entities that ride nothing (roots); a passenger travels inside its root. This replaces `mob_tree_nbt` and the boat's passenger saving.
- Chunk placement follows the root, as vanilla's section storage does.

**Load:**
- `fn spawn_tree(&mut self, tag: &Tag, origin) -> Option<Ent>`: spawn the root by its id (the existing per-kind loader, any kind). Then for each `Passengers` entry, `spawn_tree` it and link the child with `start_riding(child, root, true, false)`, then `positionRider`. Verify the arguments in `EntityType.loadEntityRecursive` and its caller.
- `spawn_loaded_entities` and the summon path (`commands/world_cmds.rs` or wherever `/summon` reads NBT) both call `spawn_tree`. This replaces `take_mob_passengers`.
- `/summon`: vanilla `SummonCommand.createEntity` uses `EntityType.loadEntityRecursive(tag, level, REASON, e -> { e.snapTo(pos…); return e; })`. Passengers are positioned by riding, not by `pos`.

**Player `RootVehicle`** (step 7) uses `entity_tree_nbt` / `spawn_tree` too.

## Tests
- `persist.rs`:
  - `a_tree_of_any_kinds_round_trips` (an item on an armor stand on a boat: save, clear the level, load; then the table has both links and the kinds match);
  - `a_player_passenger_is_not_saved_in_its_vehicle`;
  - `only_roots_are_saved_at_top_level`.
- Command fixture (`game/command_harness.rs`), against vanilla's outputs: `/summon boat ~ ~ ~ {Passengers:[{id:armor_stand}]}`, then `/execute as @e[type=armor_stand] on vehicle if entity @s[type=boat]` → 1.
- `python3 tools/parity.py server_entities`, plus what `--changed-files` lists for `persist.rs`/`entity_nbt.rs`.

## Scenario (parent, `tools/scenarios/riding.json`, extend step 6's)
4. `/summon pig ~ ~1 ~3 {Passengers:[{id:item,Item:{id:stone,count:1}}]}`. Save, quit, relaunch.
5. `/execute as @e[type=item] on vehicle if entity @s[type=pig]` succeeds.
