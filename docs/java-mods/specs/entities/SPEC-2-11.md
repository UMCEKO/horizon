# SPEC-2-11: portal and block-contact work moves into the entity's turn

Index: `SPEC-2-8.md` (E3). Size: ~250 lines. Needs 2-10. **Changes behaviour toward vanilla** (when in the tick these happen).

## Why
Three level-wide passes do, for all entities at once, what vanilla does inside each entity's own tick:

| Pass (file) | Vanilla, inside the entity's tick |
|---|---|
| `handle_entity_portals` (`nether_portal.rs:619`), `handle_end_gateway_portals` (`end.rs:1544`), called before `tick_items` | `Entity.baseTick` → `handlePortal()` (`Entity.java` ~543), at the start of the entity's tick |
| `find_entity_portals` (`nether_portal.rs:783`), called after `tick_experience` | `NetherPortalBlock.entityInside` / `EndPortalBlock.entityInside` / `EndGatewayBlock.entityInside`, reached from the entity's own move: `Entity.applyEffectsFromBlocks` (`Entity.java:938`/`980`) at the end of `move` (or `tick` for kinds that don't move) |
| `tick_sculk_entities` (`sculk.rs:770`), entity part | the movement game events (`STEP`, `SPLASH`, `SWIM`, `HIT_GROUND`…) emitted by `Entity.move` / `tryCheckInsideBlocks` of that entity; the player's warden spawn tracker part is `ServerPlayer.tick` and goes to 2-12 |

`resolve_arrivals` and `resolve_end_gateway_arrivals` are level work (entities waiting for chunks in another dimension): they stay where they are.

## Change
- **`handlePortal`:** `fn handle_portal(&mut self, e: Ent)`, the per-entity body of `handle_entity_portals` and `handle_end_gateway_portals`, called at the start of each turn's body that vanilla's `baseTick` reaches: mobs (before the lib tick: it is the first step of `baseTick`), items, orbs, boats, carts, projectiles, TNT, falling blocks, and every kind whose vanilla `tick()` calls `super.tick()` down to `Entity.baseTick`. Kinds that override `tick` without it (check each: `grep -n "super.tick()\|baseTick()" <Class>.java`) don't call it. The "positions before the entities move" capture the pass took moves into the same call.
- **`entityInside`:** `fn entity_inside_portals(&mut self, e: Ent, from: Vec3, to: Vec3)`, the per-entity body of `find_entity_portals` (the portal blocks its box swept through), called where the turn's move ends: after the lib mob tick (with the mob's previous and new position), after each kind's own move in its turn. Native's "the ones that moved this tick" filter becomes the per-turn `from != to` test that `applyEffectsFromBlocks(from, to)` itself uses.
- **Sculk:** the per-entity movement game events of `tick_sculk_entities` are emitted in the turn, at the same move ends, in the order `Entity.move` emits them; the vibration listeners already queue them per game tick, so only the emission point moves.
- Delete the three passes' entity parts from `tick_level`.

## Tests
- `game_tests`:
  - `an_item_in_a_portal_goes_through_in_its_own_turn` (its `handlePortal` runs before the next entity's turn);
  - `a_mob_walking_into_a_portal_marks_it_in_its_turn`;
  - `a_step_vibration_is_emitted_during_the_walkers_turn` (a sculk sensor's listener sees the event before the next entity's turn).
- Harnesses `--changed-files` lists (expect `nether_portal`, `end_gateway`, `end_portal`, `sculk`, `vibration`, `server_entities`); moved numbers only with the oracle agreeing.
- `python3 tools/lan_e2e.py --check portal` (or what `--list` names).
