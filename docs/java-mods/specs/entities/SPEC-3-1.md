# SPEC-3-1: entity receivers for W12's gate (`Recv::Entity`)

Design: W12 `DESIGN.md` (decision U4: W12 replaces the entities phase-3 routing hook; §2.3 admitted receivers, §2.4 views, §2.5 contexts) and entities `DESIGN.md` §2 (views). Index: `SPEC-2-8.md`. Size: ~200 lines. Needs W12 SPEC-1 to SPEC-4 and entities phase 2 (every entity a kind; each tick port a `Game` fn by id).

This phase adds **no routing mechanism**: W12's `route!`, `route_call`, route stubs, `routes.bin`, the `needs-bridge` closure and the lint are the mechanism. It adds the entity receiver family and gates the pack's three entity routes (3-2).

## 1. Admitted entity types (Java, `B/Routing.java`)
W12 SPEC-2 writes `-2` (pending) for `family: entity`. Replace it:
- At the first `server_start` (where `EntityFacts.types(level)` builds `entity-types.bin`), for each route of family `entity`, the admitted set is every entity type whose class `reaches(type class, owner, name, desc, overriders)` (W12 SPEC-2's rule), vanilla and mod types alike. A mod type whose own class overrides the method isn't admitted for the mixin route; its override is phase 6's.
- Written into `routes.bin`'s admitted list by type id, the same way `block` and `item` are (W12 SPEC-2). The content cache keeps it with `entity-types.bin`.

## 2. Rust (`G/java_routing.rs`)
- `Recv::Entity(Ent)`. `Gate::admitted` for it: the route's `Admitted::Only` set holds the receiver's type id (`type_of(ent)`; resolve to a per-route bitset over entity type indices at first use, as W12 does for blocks).
- `route_call` with `Recv::Entity(e)` writes the receiver as W12 §2.4's entity ref: `int id` (players: their view's UUID instead, as `Recv::Player`), then `int mask` and the group bytes (`facts.reads(type) | route closure reads`), so `Routing.call` hands Mixin a refreshed view. Java: `EntityViews.apply` on `LIVE.get(id)` before invoking the merged method.
- **Context:** an entity route's gate is the first statement of the `Game` fn by id that phase 2 made of the port (`turn_item`'s tick part, `hurt_item_entity`, the effect add). No vector is lent in phase 2's turns, so W12's `route_lent!` isn't needed for entities; a debug assertion in `route_call` checks `self.lent()` is empty.
- After the reply: `note_touched` and the turn's `refresh_pass`, as for events.
- **Inside a mob's lib body** (an effect added from AI code): no gate there (W12 §2.5: a gate in a lib fn doesn't compile). The lib reports the add as a `MobEvent`, and the bin's handler calls the gated port, in the turn's event order (2-7). Record each such seam in `tools/java_routing/deferred.tsv` only if it can't be lifted.

## 3. Tests
- `cargo test -p horizon-client --features java-mods --bins java_routing`: `an_entity_route_admits_only_its_types` (a hand-made `Routes` with `ItemEntity#tick` admitted for `minecraft:item` only: `Recv::Entity` of a pig isn't admitted, of an item is).
- `java_mods.py unit`: `RoutingReachesCheck` for an entity class chain (a subclass overriding without `super` is not admitted; with `super_call: true` it is).
- `python3 tools/java_routing.py lint` passes.
