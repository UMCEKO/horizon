# SPEC-12 (W5): a mod's patch to GUI item drawing, as a W12 edge route over native's items (D11)

Read first: `SPEC-7.md` §1 (GUI bridges), `/home/umceko/hz-poc/agents/w12/DESIGN.md` §2.1–2.2, `docs/java-mods-map.md`
section 4 (items and decorations: `java_items_request`, `java_decorations_request`), `G/item_draw.rs`, `B/BlockRenders.java`
`decorations`. Paths as SPEC-2.

Depends on: SPEC-7 §1 (W12's Java-bodied stub and `routes.bin`). Independent of the screen specs.

## Rule (user, D11)

Item drawing in GUIs stays native. When a loaded mod patches vanilla's GUI item drawing method
(`GuiGraphicsExtractor#item`, every overload `routes.bin` lists `on`; showcase: AE2 and Refined Storage inject into it) with
**edge** changes, only the mods' handlers run in Java; what they draw is recorded and composed with native's item: HEAD
handlers' drawing under it, RETURN/TAIL handlers' over it; a HEAD cancel means native does not draw that item. A **body**
patch of item drawing is not routed: boot reports `Horizon: <key> patched inside its body by <mods>: not routed (D11)`.

Items that Java itself records (a mod screen, JEI's list, a shell's mod widgets) already went through the merged method in
Java: they need nothing here. This spec is for the items **native** draws: slots, the hotbar, the carried stack, creative,
stations, tooltips' item rows.

## Java (`B/GuiItems.java`, op `client_gui_items`)

- `GuiRoutes.backed` is true for the probe's `GuiGraphicsExtractor` while `GuiItems.probe` runs; the item method's GUI bridge
  row (`RES/gui_bridges.tsv`, seam `item`) makes its stub body `GuiRoutes.body(item)` = `Recorder.cut()` and "body ran".
- `probe(stack, modifiers)`: a fresh `Recorder` and graphics, call `graphics.item(stack, 0, 0)` (the merged method with the stub),
  return `bytes before`, `bool bodyRan`, `bytes after`, `bool animating` (as `HudAnimation` decides between two probes of the
  same key).
- Op `client_gui_items`: request `int n` x (`bytes stack` as `java_item_text` keys it, `byte modifiers`); reply in order.

## Rust (`G/java_gui_items.rs`, new; the call in `G/item_draw.rs`)

- Active only when an item-drawing key is `on` with kind edge (`horizon_java_host::routes()`); otherwise no change and no call.
- Cache per key `(stack without count, modifiers held)`. At the start of a frame's GUI drawing, collect the native GUI items of
  the previous frame not in the cache and ask for them in **one** synchronous request (one crossing per frame at most; log
  waits over 5 ms); re-ask the drawn ones with each tick (the tick job), and per frame those whose reply said `animating`.
- Drawing a GUI item: paint `before` translated to the item's x, y and scale, then native's item unless `bodyRan` is false,
  then `after` (the decorations path stays as it is). Mark the site with `gui_route!` for the keys served.
- A stack not answered yet this frame draws native's item alone (first frame only).

## Tests
- Java unit `U/GuiItemsCheck.java`: `probe` with a fake route whose HEAD fills a quad and whose RETURN fills another gives
  `before` with one, `after` with one, `bodyRan` true; a cancelling HEAD gives `bodyRan` false.
- Test mod: a client mixin, HEAD inject into `GuiGraphicsExtractor#item(ItemStack;II)V` drawing a 2x2 quad for one test item;
  `bridges` checks `gui_item_edge` via the op's reply for that item.
- Rust: request and reply parse; cache keys follow modifiers; a `bodyRan = false` reply skips native's draw.
- `java_mods.py check`, `unit`, `bridges`, `cargo test ... --bins java_`, clippy, default build.

## Scenario (`scn-12.json`, showcase install, `HORIZON_JAVA_DEBUG=1`)
SPEC-2's floor, survival, `/give @s ae2:crafting_pattern`, `/give @s refinedstorage:pattern` (check the ids in the install),
`{"wait": 200}`, `key.inventory`, `{"wait": 40}`; checks: `log` `route net/minecraft/client/gui/GuiGraphicsExtractor#item.* on`
since start, no exceptions, screenshot.

## User steps (showcase install)
1. With an encoded AE2 pattern and an RS pattern in the inventory, hold Shift (or whatever their docs say): each shows what
   its mod draws for it on the Forge client (its output item); release: the pattern again.
2. The hotbar shows the same.
3. Every other item in inventories, hotbar and creative looks as before; opening a full chest costs no visible hitch.
