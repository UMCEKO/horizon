# SPEC-8 (W5): item tooltips mods change, including the recipe book's (revision 2)

Read first: as SPEC-7, plus `SPEC-7.md`, `G/java_item_text.rs`, `B/ItemText.java`, vanilla's
`AbstractContainerScreen.extractTooltip`/`getTooltipFromContainerItem`, `Screen.getTooltipFromItem`, `ItemStack.getTooltipLines`,
`RecipeBookPage.extractTooltip`, `GhostSlots.extractTooltip`, `RecipeButton.getTooltipText`. Paths as SPEC-2.

Depends on: SPEC-7 (routes, the `TOOLTIP` step). SPEC-3/4/5 for the screens it shows on.

## Goal

Tooltips show what mods add, everywhere native shows an item tooltip, with the least Java (D5):
- **Lines only** (most mods): native keeps drawing the tooltip; its lines come from Java, which runs vanilla's line method
  with the mods' patches and events. Enchantment Descriptions, Shulker Box Tooltip's removed content lines, AE2's
  `getTooltipFromItem`, Jade's mod name in creative, every `ItemTooltipEvent` listener.
- **Drawn by Java** only when a mod changes how a tooltip is drawn: `RenderTooltipEvent.GatherComponents`/`Pre`/`Texture`
  listeners (AppleSkin's food icons, Shulker Box Tooltip's preview and background) or a routed tooltip drawing method.
- The recipe book's hover tooltip gets the same (D12: no debt).
Where no mod changes tooltips, nothing crosses.

## 1. Lines (`G/java_item_text.rs`, `B/ItemText.java`)

- **When:** a mod listens (owner a mod's, `Shells` reader) to `ItemTooltipEvent`, `AddAttributeTooltipsEvent` or
  `GatherSkippedAttributeTooltipsEvent` (checked at each world join and at each shell open: JEI-style late listeners), or one of
  these keys is `on` in `routes.bin`: `ItemStack#getTooltipLines`, `#addDetailsToTooltip`, `#addToTooltip`, `Item#appendHoverText`,
  `Screen#getTooltipFromItem`, `AbstractContainerScreen#getTooltipFromContainerItem` and its vanilla overrides, and every
  vanilla implementation of `TooltipProvider#addToTooltip` (list them from the vanilla jar with `javap`; showcase:
  `ItemEnchantments`, `ItemContainerContents`). Status bit `bool linesRouted`. Mark native's tooltip-line site with
  `gui_route!` for these keys (W12 gate list).
- **What:** `java_item_text` asks for every stack while `linesRouted` (today: mod items only). The key gains `byte context`
  (0 plain `Screen.getTooltipFromItem`; 1 the open shell's `getTooltipFromContainerItem`, so creative's override and Jade's
  patch apply) and `byte modifiers` (Shift, Ctrl, Alt held: lines that depend on them change with them).
  Java runs that vanilla method whole on the Java stack (a line list is one recording per stack, W12 §2.5).
- **When asked:** a stack hovered in a screen and not cached is asked **synchronously** on the window thread (one crossing,
  as a click; log waits over 5 ms like `java_click_canceled`), so the first frame already has the mods' lines; afterwards the
  hovered stack is re-asked with each tick (lines can change with time or state); others stay cached as today.
- Native draws its tooltip with these lines at every site: slot tooltips, stations, creative, the recipe book.

## 2. Drawn by Java (`B/Shells.java`)

- **When:** a mod listens to `RenderTooltipEvent.GatherComponents`, `.Pre` or `.Texture`, or one of these keys is `on`:
  `GuiGraphicsExtractor#setTooltipForNextFrame` (all overloads), `#setComponentTooltipForNextFrame`, `#tooltip`,
  `AbstractContainerScreen#extractTooltip`. Read with `wanted()` at shell open; `wanted()` is true then too.
- **Slot tooltip:** the `TOOLTIP` step calls the shell's `extractTooltip` (vanilla's: hovered slot, carried-item rule,
  `setTooltipForNextFrame`), drawn at `DEFERRED` with NeoForge's events and components; `routed` bit set, native skips.
  Running it is allowed by D5 (it posts the events mods listen to).
- **Recipe book tooltip** (`BOOK_TOOLTIP`): the hovered recipe lives in native, so native sends it:
  `Input::BookHover` (kind 24): `bytes stack` (the display stack native's book shows under the cursor, or the hovered ghost
  slot's item), `bool more` (the button has several recipes), `bool ghost`, `int x, int y`; empty stack = none. Java does what
  `RecipeBookPage.extractTooltip`/`GhostSlots.extractTooltip` do with it: `graphics.setComponentTooltipForNextFrame(font, lines,
  x, y, stack.get(TOOLTIP_STYLE))` with `lines = Screen.getTooltipFromItem(mc, stack)` plus `RecipeButton`'s more-recipes line
  (static field accessor) when `more` and not `ghost`. `routed` bit for `BOOK_TOOLTIP` set; native skips its book tooltip.
- In lines-only mode the book tooltip is native's with §1's lines plus its own more-recipes line.
- Check, debug only: the hovered slot's item and count in Java equal native's (add them to `ShellLayout` behind
  `HORIZON_JAVA_DEBUG`); log once `Horizon: the shell's slot <i> holds <x>, native shows <y>`.

## 3. Timing (D2)
Cursor moves already re-record; key presses and releases over a shell are inputs whose replies re-record (Shift-dependent
tooltips change that frame). In lines-only mode, the modifier byte in the key does the same.

## Tests
- Rust: the text key with `context` and `modifiers` (extend `key_ignores_the_count_and_follows_components_and_flags`);
  `linesRouted` false asks only for mod items; `Input::BookHover` writes what `events` reads.
- Java unit `U/TooltipRoutingCheck.java`: the "a mod listens to one of these classes" reader over a fake listener map: a mod's
  `ItemTooltipEvent` listener sets lines mode, a NeoForge one does not; a mod's `GatherComponents` listener sets drawn mode.
- `java_mods.py check`, `unit`, `cargo test ... --bins java_`, clippy, default build.

## Scenario (`scn-8.json`, showcase install, `HORIZON_JAVA_DEBUG=1`)
SPEC-2's floor, survival, `/give` bread, `minecraft:enchanted_book[stored_enchantments={"minecraft:sharpness":3}]`,
`minecraft:shulker_box[container=[{slot:0,item:{id:"minecraft:diamond",count:3}}]]`, `{"wait": 200}`, `key.inventory`,
`{"wait": 40}`; checks: `log` `routes .*TOOLTIP` (debug line at shell open listing routed steps), `the shell's slot .* holds`
absent, no exceptions; close.

## User steps (showcase install)
1. Inventory, chest, furnace, anvil, creative's inventory tab: bread shows AppleSkin's icons; the enchanted book its
   description (Shift if asked); the shulker box its preview, without "Contents" lines.
2. Creative, any tab: Jade's mod-name line ("Minecraft", "Mekanism").
3. Recipe book (crafting table): hover the bread recipe: AppleSkin's icons and Jade's line show on it too; a button with
   several recipes still says so.
4. Move quickly across slots: no stale tooltip, no one-frame tooltip without the mods' lines.
5. Hold an item on the cursor: no tooltip over slots (vanilla), except bundles.
