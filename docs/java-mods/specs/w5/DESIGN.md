# W5: mods' additions to native vanilla screens (and the HUD)

Evidence: `run1/` (one `hz_scenario.py` run on a copy of `install-rs`, `bin-next`, JFR `jdk.MethodTrace` with stacks).
`run1/w5.jfr` (34 MB, delete when done), `run1/stacks.txt` (one or two stacks per traced method), `run1/mek.png`.
Repro: `env.sh` (JFR filter), `scenario.json`, `c.sh` (horizon-ctl on the kept game).

## 1. What JEI does, measured

Session: survival, a Mekanism Metallurgic Infuser screen (a real Java `AbstractContainerScreen`, recorded and drawn natively)
for 33 s, then the native survival inventory (Java holds `NativeScreen`).

JEI already works over Java-drawn container screens: list, page counter "1/74", search box, bookmark buttons
(`run1/mek.png`). So everything JEI needs from a container screen is in this trace.

| JEI handler (`mezz.jei...`) | Event (bus) | Fired from (vanilla / NeoForge site in the stack) | Count |
|---|---|---|---|
| `JeiGuiStarter.start` (runtime start, registers every screen listener below) | `RecipesReceivedEvent` | `ClientPayloadHandler.handle(RecipeContentPayload)` in `ClientBridge.receive` | 1, 11:46:04, after join |
| `GuiEventHandler.onGuiOpen` | `ScreenEvent.Opening` | `Gui.setScreen` line 260 | 2 |
| `GuiEventHandler.onGuiInit`, `ClientInputHandler.onGuiChanged` | `ScreenEvent.Init.Post` | `Screen.init(int,int)` line 344 | 2 (Mekanism screen, then `NativeScreen`) |
| `ClientInputHandler.onGuiChanged` | `ScreenEvent.Closing` | `Gui.setScreen` | 1 |
| `GuiEventHandler.onClientTick` | `ClientTickEvent.Post` | `Minecraft.tick` | 856 (every tick, any screen) |
| `drawForScreenBackground` → `updateOverlayProperties`, `drawOverlayBackgrounds` | `ScreenEvent.Render.Background` | `Screen.extractRenderStateWithTooltipAndSubtitles` line 111, right after `extractBackground` | 3969 (every frame) |
| `drawForScreenForeground` → `drawOverlayForegrounds`, `drawPostForeground` | `ScreenEvent.Render.Foreground` | `AbstractContainerScreen.extractRenderState` line 93, after `extractContents` (widgets, labels, slots), before the carried item and tooltip | 3969 |
| `ClientInputHandler.onGuiMouseClicked`, `onGuiMouseReleased` | `ScreenEvent.MouseButtonPressed.Pre`, `MouseButtonReleased.Pre` | `ClientHooks.onScreenMouseClickedPre` from `MouseHandler.onButton` line 109; the screen's `mouseClicked` is line 111 | 3 |
| `ClientInputHandler.onGuiMouseScroll` | `ScreenEvent.MouseScrolled.Pre` | `ClientHooks.onScreenMouseScrollPre` from `MouseHandler.onScroll` line 199 | 1 |
| `ClientInputHandler.onKeyboardKeyPressedPre` → `handleKeyPress` (R, U) | `ScreenEvent.KeyPressed.Pre` | `ClientHooks.onScreenKeyPressedPre` from `KeyboardHandler.keyPress` line 513 | 2 |

What JEI reads from the screen, per frame (`mezz.jei.neoforge.platform.ScreenHelper` → vanilla):

| Read | Vanilla | Per frame |
|---|---|---|
| `getGuiProperties` → `getLeftPos`, `getTopPos`, `getImageWidth`, `getImageHeight` | `AbstractContainerScreen` getters of `leftPos`, `topPos`, `imageWidth`, `imageHeight`; plus `Screen.width`, `height` | 2 each |
| `getGuiExclusionAreas` → `ToastGuiHandler.getGuiExtraAreas` → `getToastsArea` | the Java client's `ToastManager` | 1 |
| `getGuiClickableArea` | per-screen-class handlers (`InventoryScreen.class`...) | 1 |
| on R/U or a click only: `getClickableIngredientUnderMouse` → `getHoveredSlot` | `AbstractContainerScreen.getHoveredSlot()` (field `hoveredSlot`, set by vanilla in `extractContents`) | 5 in the session |
| `MouseUtil` | `MouseHandler.xpos/ypos` scaled | on input |

JEI adds no widgets (no `addRenderableWidget` from `Init`): its overlay is drawn in the two Render events and takes input
only in the input `Pre` events (it cancels them). It never calls vanilla's drawing.

Costs under tracing (inflated): `drawForScreenForeground` 0.18 ms mean, `drawForScreenBackground` 0.11 ms, `onGuiInit` up to
22 ms (once per open). The Java screen path asks `client_frame` every window frame: 3969 frames in 33 s (119/s, blocking the
window thread).

Other findings from the run:
- Java screens skip `ScreenEvent.Render.Pre`/`Post` today: `ClientBridge.frame` calls `extractRenderStateWithTooltipAndSubtitles`
  directly, while NeoForge fires Pre/Post in `ClientHooks.extractScreen(screen, screenLayers, ...)` (0 calls in the trace).
  Jade, sophisticatedcore, polytone and IPN listen to `Render.Post`. Fix in SPEC-1 step J6 (one line).
- `NativeScreen` already gets `Opening`/`Init.Post`/`Closing` (JEI's `onGuiInit` ran on it); harmless, JEI ignores non-container screens.
- My scripted clicks landed inside the Mekanism window (I assumed GUI scale 4; it is 3), so they show the routing works
  (JEI's `Pre` ran, then `GuiMekanism.mouseClicked` → `slotClicked`), not JEI's list actions. Nothing points at an input bug.
- **JEI's screen listeners exist only after its runtime starts** (`RecipesReceivedEvent`, after join) and are cleared on leave
  (`RuntimeEventSubscriptions`). The static report misses them too: JEI registers through `PermanentEventSubscriptions.register(Class, Consumer)`,
  so `replaces.json` lists only 2 JEI listeners. Neither boot-time bus reading (`WorldFrame.decide`) nor the jar scan sees them.

How common the mechanisms are (top-50 pack, `tools2/replaces/out-top50`, `top50/mixins`; RS pack `install-rs/analysis`):
- `ScreenEvent` listeners found statically: `Init.Post` 6 mods, `Opening` 5, `KeyPressed.Pre` 8, `MouseButtonPressed.Pre` 6,
  `Render.Post` 5, `Render.Pre` 3, `Render.Background`/`Foreground` 3 each. All from generic helpers too (architectury, balm, puzzleslib).
- Mixins into vanilla screen classes (non-accessor): `AbstractContainerScreen` (`mouseClicked` fancymenu, polytone; `slotClicked` IPN;
  `extractTooltip`, `isHovering` shulkerboxtooltip; `extractContents` architectury), `CreativeModeInventoryScreen` (jade, polytone, sounds),
  `InventoryScreen.extractBackground` (polytone), `Screen` (fancymenu, physicsmod, balm, tcdcommons, shulkerboxtooltip).
- RS pack: Jade into `CreativeModeInventoryScreen.getTooltipFromContainerItem` (mod name in tooltips), AE2 `modify_variable` into
  `AbstractContainerScreen.extractSlot`, AE2 and RS inject into `GuiGraphicsExtractor.item` (item decorations), AE2 into `Screen.getTooltipFromItem`.

## 2. The design: a shell screen

When native opens one of its vanilla screens and a mod listens to screen events, the Java client opens the **real vanilla
`Screen` class** for it (`new InventoryScreen(player)`), marked as a **shell**. Mods see the exact class (`instanceof`,
`getClass()`, JEI's per-class handlers all match). Vanilla's own drawing and logic do not run in it: native does them.

| Part | Runs in Java | Native |
|---|---|---|
| `Gui.setScreen(shell)`: `Opening`, vanilla `init()` (layout bookkeeping, vanilla widgets created), `Init.Pre/Post` | yes, as vanilla | opens and draws its own screen |
| Vanilla widgets made by `init()` | exist, never drawn, never get input | draws and handles its own |
| Mods' widgets (`Init` listeners' `addRenderableWidget`) | drawn (recorded), get input | paints the recording |
| `Render.Pre/Background/Foreground/Post`, deferred tooltips | fired by the shell's frame in vanilla's order, recorded | paints the 5 segments between its own layers |
| `tick`, `containerTick`, `removed`, the handler bodies `mouseClicked`... | cancelled for the shell (HEAD mixin), replaced by the dispatch below | owns vanilla logic |
| Slots, items, carried stack | the shell's menu is the Java client's real `inventoryMenu`, filled by the Java server's packets | owns clicks |

This follows the HUD precedent (vanilla layers still run for bookkeeping) only for `init()`; every vanilla draw and every
vanilla handler body is skipped. It replaces COVERAGE-SPEC W5's first idea (open the Java screen and record all of it), which
would run vanilla's screen in Java and duplicate native (principle 0).

### 2.1 Geometry

The shell carries native's real layout. Every shell request and the tick batch carry `leftPos`, `topPos`, `imageWidth`,
`imageHeight` (native's `inventory_origin`, which includes the recipe book's shift, and the window size) and the hovered
slot as the menu's index (`slot_at` → `remote::menu_slot_index`). Java writes them with an accessor mixin before any event
fires (and at `Init.Post` HIGHEST, so mods' `Init` listeners already see native's numbers). `width`/`height` come from the
existing `size()` sync; a size change runs `screen.resize` as vanilla does (re-init, `Init` events again).

Later screens add their own: the recipe book's visibility (`RecipeBookComponent` area for JEI's exclusion), creative's tab
and scroll, a container's id.

### 2.2 Slot state and the item under the cursor

- Survival inventory: the Java client's `inventoryMenu` already holds the real stacks (the Java server view's
  `broadcastChanges` sends them; the Mekanism screen showed the logs given by a native `/give`).
- `hoveredSlot` is set from native's hovered index each request, so `getHoveredSlot()` answers what native shows; vanilla's
  geometric `getHoveredSlot(x, y)` never runs (it would duplicate native's).
- Containers native opens (chest, crafting table, furnace; phase 2): the window forwards the vanilla packets it got for that
  menu (`OpenScreen`, `ContainerSetContent`, `ContainerSetSlot`, `ContainerSetData`) to the Java client, which then builds
  the menu and screen exactly as vanilla's `MenuScreens.create` does, marked shell. The Java client learns only from server packets.
- The carried stack: native's; the shell menu's `carried` follows the same packets. JEI does not read it.

### 2.3 Detection: static, boot and open time

| What | When | How |
|---|---|---|
| A mod listens to screen events | **when native opens a screen** | The bus's listener lists for the `ScreenEvent` subclasses (as `WorldFrame.listened`), counting only listeners whose unwrapped owner class is a mod's (not NeoForge's, Minecraft's or Horizon's module). JEI's appear only after its runtime starts, so boot is too early. Reading a few array lengths at the open is not polling: it is the moment vanilla would post `Opening` |
| A mod mixes into a vanilla screen class's drawing or handler bodies | static (`mod_replaces`, `mixins`), reported at boot | These have no effect on a shell (bodies cancelled) or on native drawing. Boot log lists them per class (M2: "native-owned"). Phase 4 routes them (below) |
| Accessor/invoker mixins, mixins into `init`, getters | static | Work on the shell as on vanilla |

Rust learns whether a shell opened from the tick status (`shell` field), never by asking.

### 2.4 Frame timing

Follows the HUD rule, with input as a third trigger:
1. **Per tick:** the tick job records the shell's 5 segments after `minecraft.tick()` (as `hud()`), off the window thread.
   The first frame of a newly opened shell waits for that tick job (as an open Java screen's frame waits), so the overlay is
   on the screen's first frame, as in vanilla.
2. **Per input:** every synchronous shell input reply carries a fresh recording (a click, key or scroll changes JEI's state).
3. **Per frame while changing:** when the cursor moved since the last recording or the recording is animating (two in a row
   differ, `HudAnimation.ends`), the frame asks `client_shell_frame` with the frame's partial tick and cursor, unless the
   tick job runs (then the last recording stays). A still cursor over a static overlay costs nothing per frame.

Alternative for the user (decision D2): record every frame while a shell has mod content, as vanilla posts the Render events
every frame (about 0.3 to 0.5 ms of JEI per frame on the window thread, 119 calls/s measured on Java screens).

### 2.5 Input order

Vanilla (`MouseHandler.onButton`, `KeyboardHandler.keyPress`, `charTyped`, `onScroll`, `handleAccumulatedMovement`) runs for
real in Java: `InputEvent`s, the screen `Pre` event, the screen's handler, the `Post` event, in NeoForge's own order. Only the
shell's handler body is replaced (`Shells.java`):

```
mouseClicked:  native widget under the cursor?  -> native (vanilla widgets come first in children order)
               else mod widgets, in children order -> mod (focus, dragging as ContainerEventHandler)
               else native handles the click?    -> native
mouseReleased / mouseDragged / mouseScrolled / keyPressed / charTyped / keyReleased: the same three steps
```

The window sends, with each input, native's two answers for that input (widget hit, would handle), computed without acting.
Java replies who owns it: `EVENT` (a `Pre` listener cancelled it, the shell's handler never ran), `WIDGET` (a mod widget),
`NATIVE`, `NONE`. Native acts only on `NATIVE`. `Post` listeners see vanilla's `handled` flag. One synchronous crossing per
press, release, key, char and wheel step; moves ride the frame request.

Examples: a click on JEI's list → `EVENT` (JEI cancels `MouseButtonPressed.Pre`); `E` while JEI's search is focused → `EVENT`, the
inventory stays open; `E` otherwise → `NATIVE`, native closes it and the tick tells Java (`Screen(false)`).

### 2.6 Screens mods open over the shell

JEI's recipe view (`RecipesGui`) is a mod screen: `setScreen(RecipesGui)` → status `screen` true → native shows `Screen::Java`
without native's close side effects (vanilla keeps `inventoryMenu` open server-side). Closing it, JEI returns to its parent,
the same shell object → status `shell` open → native shows its inventory again. If `Opening` cancels or replaces the shell,
the status says so and native closes its screen or shows the Java one.

### 2.7 HUD (phase 5)

Today vanilla layers run into a discarding target and mods' layers are recorded on top. Missing: a mod cancelling a vanilla
layer (`RenderGuiLayerEvent.Pre`, travelersbackpack) or drawing in a vanilla layer's Pre/Post (puzzleslib, voicechat): native still
draws the layer, and the mod's pieces land on top of the whole HUD. Design: `HudLayers.discarding` already wraps each vanilla
layer; it records which vanilla layers were cancelled this tick (a bitmask in status) and cuts the recording into one segment
per vanilla layer boundary, so native skips cancelled layers and paints each segment right after its layer. `CustomizeGuiOverlayEvent`
(Jade's boss bar) stays with W12.

## 3. Phases

| Phase | Scope | Accept |
|---|---|---|
| 1 (SPEC-1) | Shell for the survival inventory; Render events; input routing; hovered slot; open-time detection; Render.Pre/Post for Java screens | JEI beside the survival inventory: scroll, click, R/U, search |
| 2 | Creative inventory, crafting table, chest, furnace...: native forwards the menu packets; per-screen layout extras (recipe book area, creative tabs) | JEI on every vanilla container, recipe-transfer `+` buttons visible |
| 3 | Recipe transfer: JEI's server handler runs in Java on the player's menu (a mod behaviour) with slot writes through to native, as block entities are | Shift-click `+` fills the crafting grid |
| 4 | Mods that change a vanilla screen's own drawing or logic (mixins into handler or draw bodies, `ItemSlotMouseAction`, IPN/mousetweaks/shulkerboxtooltip): per screen class, if a loaded mod patches its bodies, that class opens as a full Java screen (W5's original route) | M2 report shows none native-owned |
| 5 | HUD layer cancel/wrap; mod tooltip lines on native tooltips (Jade's mod name in creative) through `java_item_text` | travelersbackpack hides a vanilla layer; Jade's mod name |

## 4. Decisions for the user

- **D1.** The shell runs vanilla's `init()` (widget objects, recipe book component) for layout bookkeeping, never its drawing
  or handlers. Acceptable as the HUD's discarding layers are, or should `init()` be skipped too (mods reading vanilla widgets,
  JEI's recipe book area, would then see none)?
- **D2.** Frame timing: tick + input + per frame while changing (proposed), or every frame while mod content is shown?
- **D3.** Phase 4 rule: a vanilla screen class whose bodies a loaded mod patches runs fully in Java (recorded like mod screens).
  In the top-50 pack that would make every container screen Java (fancymenu, polytone, IPN patch `AbstractContainerScreen`).
  Accept, or route per method (more work, closer to "native unless detected")?
- **D4.** Phase 3: JEI's transfer is server-side mod code acting on a vanilla menu; it needs a write-through view of native's menu.
  OK to design it with W9/W12?

## Decisions (2026-11-04)
- **D1 (user):** the shell runs vanilla's `init()` for layout and the widget list only. Vanilla's widgets are never drawn, and their handlers stay native.
- **D2 (from the user's HUD rule):** recordings per tick, on input, and per frame only while changing.
- **D3 (from "only detected behaviours run in Java"):** a mod patch to a vanilla screen class routes only the patched methods to Java, never the whole screen.
- **D4:** JEI's recipe transfer (mod server code writing a vanilla menu) is designed with W9/W12, with slot writes through native.

## Specs after SPEC-1 (revision 2, 2026-11-05, after the D5–D12 rulings below)

Rule for every spec (D5/D6/D11): a shell runs `init()` and only the vanilla methods a loaded mod patches (W12 `routes.bin`)
or whose events a mod listens to; a menu is mirrored into the Java client only when a mod is detected to need it; item
drawing stays native. Each spec has files, tests, a scenario and user steps.

| Spec | Scope | Depends on | Accept (user) |
|---|---|---|---|
| [SPEC-2](SPEC-2.md) | The shell recording cut at vanilla's draw steps (14 leaves, 28 segments, `routed` bits); events still posted by `Shells`, no vanilla frame code; native draws `draw_inventory` in step order (`shell_start`, `shell_leaf`) | SPEC-1 | SPEC-1's checks unchanged |
| [SPEC-3](SPEC-3.md) | Creative shell (kind 2): tab id field and a one-slot mirror of the hovered stack (`ShellCreative`); no `selectTab`/`scrollTo` | SPEC-2 | JEI beside every tab; R/U on scrolled and searched slots |
| [SPEC-4](SPEC-4.md) | Vanilla containers (kind 3), by menu type: menu packets forwarded and native's clicks replayed through the real `handleContainerInput` **only while mirrored** (`java_mirrors`: screen listeners heard, or a route on the menu type's classes); adoption of vanilla's screen; recipe-book visibility | SPEC-2 | JEI on chests, furnaces, crafting table, hopper, dispenser, shulker box, crafter |
| [SPEC-5](SPEC-5.md) | Workstations, brewing, beacon, merchant as kind 3; per-class step methods (`ShellLeaves.methods`), `EXTRA` | SPEC-4 | JEI on all of them |
| [SPEC-6](SPEC-6.md) | Mods' actions through a shell reach native: the outbox (Java's menu packets performed by native), the input fields both ways (only with a mod accessor on the screen), drags as input (only with drag listeners), a mod's own screen class for a vanilla menu type | SPEC-4 | Mouse Tweaks' drags and wheel |
| [SPEC-7](SPEC-7.md) | W12 per-method routing for screens: GUI bridges (Java-bodied stubs), steps edge/body, structure methods, hovered query, routed handlers, `slotClicked`, the bundle wheel; boot report | SPEC-2, SPEC-6, W12 SPEC-2 + §1 additions | Shulker Box Tooltip's lock key and wheel |
| [SPEC-8](SPEC-8.md) | Tooltips: lines from Java for every stack when mods change lines (native draws); Java-drawn when mods change drawing; the recipe book's tooltip too (D12) | SPEC-7 | AppleSkin, Shulker Box Tooltip, Enchantment Descriptions, Jade's mod name, recipe book |
| [SPEC-9](SPEC-9.md) | HUD in vanilla's layer order; cancels skipped; wrapped or re-placed layers drawn by Java while detected | none | AppleSkin's overlays under/over the food bar |
| [SPEC-10](SPEC-10.md) | HUD layers mods patch (edge: handlers composed around native's layer; body: Java-drawn), layer methods from one JFR trace; forwarding the server state Java-drawn layers read | SPEC-9, SPEC-7, SPEC-4 | Jade's boss-bar placement |
| [SPEC-11](SPEC-11.md) | Showcase install, acceptance scenario `tools/scenarios/w5_screens.json`, Visual Workbench check | §1 none | Visual Workbench keeps and shows its grid |
| [SPEC-12](SPEC-12.md) | D11: a mod's edge patch to GUI item drawing runs its handlers in Java per stack (cached probe), composed around native's item; body patches reported | SPEC-7 §1 | AE2/RS pattern outputs on Shift |
| [SPEC-13](SPEC-13.md) | Puzzles Lib's `EditBox` patch routed: native's creative search, recipe-book search and anvil name boxes run their input through the shell's vanilla `EditBox` | SPEC-3/4/5, SPEC-7 | text editing as on Forge |

Order: SPEC-11 §1 first (the install), SPEC-2, then SPEC-3 ∥ SPEC-4, SPEC-5, SPEC-6; SPEC-9 any time; after W12 SPEC-2 and the
§1 additions of SPEC-7 land: SPEC-7, SPEC-8 (together), SPEC-12, SPEC-13, SPEC-10.
Not W5: recipe transfer (D4, W9/W12), mounts' screens (W2), the lectern, chat and the tab list (W6), F3 and AppleSkin's debug
entry (W8), text boxes outside W5's screens (W6).

### Wire allocations

`Input` kinds: 16 `ShellLayout`, 17 `ShellCreative`, 18 `Packet`, 19 `MenuClick`, 20 `ShellBook`, 21 `ShellState`,
22 `SlotClicked`, 23 `BundleScroll`, 24 `BookHover`, 25 `TextBox`. Shell kinds: 1 survival, 2 creative, 3 a mirrored menu.
Owners: 0 `EVENT`, 1 `JAVA` (was `WIDGET`), 2 `NATIVE`, 3 `NONE`. Leaf ids: SPEC-2. HUD layer ids: SPEC-9.

### Interface with W12 (`/home/umceko/hz-poc/agents/w12/DESIGN.md`)

W5 uses W12's mechanism unchanged and asks for two additions (SPEC-7 §1):
- `routes.bin`/`Routing` carry client keys (`side: client`); W5 reads `Routing.route/kind/on` in Java and `routes()` in Rust.
- **GUI bridges:** `RES/gui_bridges.tsv` (key, W5 seam) rows that `BridgeProcessor` handles like bridges whose native body is
  Java (`GuiRoutes.backed(this)` / `GuiRoutes.body(seam, this, args)`): a recording cut (native draws that step there), "native
  handles this input", or the item probe's marker. Edge routes get W12's exact stub around that body; body routes get none.
- **Gate list:** native sites serving a GUI key carry `gui_route!("key")` (a W12 `static Gate` marker) so `java_routing.py
  gates`/`lint` count them; client keys W5 does not serve stay in `deferred.tsv` (`client: phase C` or `client: W6`).
- Window context (W12 §2.5): W5's per-frame seams are recordings (shell steps, hovered, HUD layers, item probes, tooltip
  lines); its input seams (handlers, `slotClicked`, bundle wheel, text boxes) are synchronous shell inputs whose native effects
  come back through the outbox and state blocks, never as nested window crossings.

### What each showcase mod needs

| Mod | Mechanisms (`showcase-pack/replaces/replaces.json`) | Specs |
|---|---|---|
| Mouse Tweaks | screen events (`MouseButtonPressed.Pre`, `Released.Pre`, `MouseDragged.Pre`, `MouseScrolled.Post`); accessors and invokers on `AbstractContainerScreen` | SPEC-2..5, SPEC-6 |
| "Inventory tweaks" | not in the pack, no 26.3 build; Inventory Profiles Next (top-50) patches `slotClicked`, `handleContainerInput`, `handleContainerContent`, `MerchantScreen#extractContents`, `Hud#extractItemHotbar` | SPEC-4, 6, 7, 10 |
| Shulker Box Tooltip | tooltip events; patches `extractTooltip`, `isHovering`, `extractRenderStateWithTooltipAndSubtitles`, `BundleMouseActions#onMouseScrolled`, `KeyboardHandler#keyPress`, `ItemContainerContents#addToTooltip`, `ItemStack#addToTooltip` | SPEC-7, SPEC-8 |
| AppleSkin | its HUD layers; `RenderTooltipEvent.GatherComponents`; debug entry (W8) | SPEC-9, SPEC-8 |
| Enchantment Descriptions | patches `ItemStack#getTooltipLines`, `ItemEnchantments#addToTooltip`, `ItemStack#addDetailsToTooltip` | SPEC-8 (lines) |
| Visual Workbench | no listeners, no mixins: own block, block entity, menu type, renderer; vanilla `CraftingScreen` for its menu | already a Java screen (D10); SPEC-11 §3 |
| Puzzles Lib (ruled) | patches `EditBox` | SPEC-13 |
| AE2, Refined Storage (D11) | patch `GuiGraphicsExtractor#item` | SPEC-12 |

The superseded first-revision decision list (D5–D12 as asked) was replaced by the rulings below.

## Decisions D5–D12 (2026-11-05)
- **D5/D7 (user):** a shell runs `init()` plus only the vanilla methods a loaded mod is detected to patch, or whose events a mod listens to. Never the whole frame or creative code.
- **D6 (user):** the Java client mirrors a menu only when a loaded mod is detected to listen to or patch that menu type, or a shell for it is open.
- **D8:** accepted. Libraries listening to everything make every container a shell (no per-mod code).
- **D9:** left to the player.
- **D10:** accepted.
- **D11 (user: "We can't draw everything in java in there"):** item drawing in shells stays native. A mod's patch to vanilla's GUI item-drawing method is served as a W12 edge route: only the mod's handler runs in Java, its draws are recorded and composed over native's item. A body patch (redirect/overwrite) of item drawing is reported at boot, not routed to Java drawing.
- **D12:** rejected as debt. Mods' tooltip additions must show in the recipe book's hover tooltip too.
- Puzzles Lib's text-box patch: routed through W12 for native's search boxes; not just reported.
