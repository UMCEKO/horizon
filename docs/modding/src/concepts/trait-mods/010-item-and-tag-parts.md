<!-- chapter: Item and tag Parts -->
<!-- separator: 1 -->

For an item-specific Part, request its canonical registry key once in `register`:
`let stick = r.item_key("minecraft:stick"); r.item_part(stick, Sharp);`. Bare names use the
`minecraft:` namespace. Requests may refer to content declared later; the final item registry refuses
unknown IDs with the requesting mod. `ItemKey` equality uses its load-owned slot, and `key.id(host)`
reads its canonical diagnostic name. Item capability registration and item helpers still use strings;
their migration is a separate change.

`r.block_part_tag("minecraft:logs", Rotates)` and `r.item_part_tag("minecraft:axes", Sharp)` attach a part to
every member of a tag (its name without `#`). Each game resolves them against its own tags when its world
opens, again on `/reload`, and when a client joins a server, so the window's game and its server thread each
answer from the tags they play with; a block that leaves the tag is no longer asked. A mod's parts on one
thing run in the order it registered them, by key or by tag. A part a tag gives a thing
that already has that kind of part warns, naming both mods, and the earlier mod's part stays. A tag that
doesn't exist warns. Opening a world fails, naming the mod, when a part is attached to a block or item the game
doesn't have. Scoped hooks run on a game's thread; one dispatched from any other thread (which no game hook
does) can't see tag parts, and warns once.
