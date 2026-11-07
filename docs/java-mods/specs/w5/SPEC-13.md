# SPEC-13 (W5): a mod's patch to vanilla's text box, routed for native's boxes in shells

Read first: `SPEC-7.md` (§1, §4), `/home/umceko/hz-poc/agents/w12/DESIGN.md` §2.1, vanilla's `EditBox.java`, native's boxes:
creative search (`G/clipboard.rs` `creative_search_*`, `creative_screen.search`), the recipe-book search (`G/recipe_book.rs`),
the anvil name box (`G/stations.rs`). Paths as SPEC-2.

Depends on: SPEC-3 (creative shell), SPEC-4 (crafting/furnace/inventory recipe book), SPEC-5 (anvil), SPEC-7 §1 and §4.

## Goal (user ruling: routed, not just reported)

Puzzles Lib patches `EditBox#keyPressed`, `#onClick`, `#onDrag`, `#getWordPosition`. On Forge every vanilla text box gets that
behaviour. In Horizon the boxes of W5's screens are native ports of `EditBox`: when any `EditBox` key is `on`, their input
goes through the Java shell's own vanilla `EditBox` (the one its `init()` made), so the patched methods run there, and native
takes the resulting text, cursor and selection. Boxes outside W5's screens (chat, signs, command blocks, book editing) stay
W12-deferred (`client: W6`).

## Shell state for a box

`Input::TextBox` (kind 25): `byte box` (1 creative search, 2 recipe-book search, 3 anvil name), `text value`, `int cursor`,
`int highlight`, `bool focused`. Sent before a box input, in the same `client_shell_input`.

## Java (`B/Shells.java`, `B/mixin/TextBoxAccess.java`)

- `wanted()` is also true when an `EditBox` key is `on` and the screen has one of these boxes (D5: a routed method of the screen).
- Find the box: creative `searchBox`, `RecipeBookComponent.searchBox` (accessor), `AnvilScreen` name box (accessor).
- Write the state through field accessors (`value`, `cursorPos`, `highlightPos`, `focused`), never `setValue` (it runs the
  responder: creative's search refresh, the anvil's rename packet). Null the box's `responder` for the call, restore it after.
- Dispatch the input to the box itself, as vanilla's screen would reach it: a key → `box.keyPressed(event)`, a char →
  `box.charTyped(event)`, a click → `box.mouseClicked(event, doubleClick)` (reaches `onClick`), a drag → `box.mouseDragged`
  (reaches `onDrag`). The merged methods run (W12 body or edge: an edge stub's body here is vanilla's own Java body, since the
  box is not backed: W12 §2.2 "non-backed receivers take the original body").
- Reply: owner `JAVA` if the box handled it, then `Input::TextBox` state back (value, cursor, highlight, focused).

## Rust

- Active only when an `EditBox` key is `on` (`routes()`), the shell is open and the input targets one of the three boxes
  (focused for keys and chars, under the cursor for clicks and drags). Mark native's box input sites with `gui_route!`.
- Send the box state and the input; on `JAVA`, write value, cursor, highlight and focus into native's box, then run what native
  runs after an edit (creative's filter, the recipe-book filter, the anvil's rename send). Do not run native's own editing.

## Tests
- Rust: `Input::TextBox` writes what `events` reads; applying a reply updates native's creative search and re-filters once.
- Java unit: with a plain `EditBox` and no mods, the dispatch of Ctrl+Backspace on `"foo bar|"` gives `"foo |"` (vanilla), and
  the responder is not called.
- Test mod: a HEAD inject into `EditBox#keyPressed` that inserts `!` on F12; `bridges` types F12 into creative's search through
  `horizon-ctl` and checks the box text ends in `!` (`text_box_routed`).
- `java_mods.py check`, `unit`, `bridges`, `cargo test ... --bins java_`, clippy, default build.

## Scenario (`scn-13.json`, showcase install, `HORIZON_JAVA_DEBUG=1`)
`/gamemode creative`, `{"wait": 200}`, `key.inventory`, check `route net/minecraft/client/gui/components/EditBox#keyPressed.* on`
since start and `Java shell 2 opened`; no exceptions.

## User steps (showcase install, compared with the same pack on a real NeoForge client)
1. Creative search: type `oak planks`, use Ctrl+Left/Right, Ctrl+Backspace, Shift+arrows, double click a word, drag to select:
   each behaves as on the Forge client, and the item list filters as you type.
2. The same in the crafting table's recipe-book search and the anvil's name box (the name still applies).
