# SPEC-7 (W5): mods' mixins into vanilla screen methods, routed per method through W12 (revision 2)

Read first: as SPEC-2, plus `SPEC-2.md`, `SPEC-6.md`, `/home/umceko/hz-poc/agents/w12/DESIGN.md` (§2.1 edge/body, §2.2 the
route stub, §2.5 the Window context, §2.7 `routes.bin`, §2.8 the gate list) and W12 `SPEC-2.md`/`SPEC-3.md`. Paths as SPEC-2;
`J/` = `mods/horizon/src/java-mods-host/`, `H/` = `crates/horizon-java-host/src/`.

Depends on: SPEC-2, SPEC-6, W12 SPEC-2 (`routes.bin`, `Routing.confirm`, route stubs) and the W12 additions in §1 below.

## Rule

Only a vanilla method a loaded mod patches runs in Java, at the place vanilla calls it, and only for shells (D3, D5).
Its kind comes from W12: **edge** (the mods' handlers surround the body: native keeps the body) or **body** (the vanilla body
runs in Java). The showcase cases are in the table at the end.

## 1. What W5 needs from W12 (agreed interface; W12 SPEC-2 implements the Java-body stub)

- `routes.bin` and `Routing` cover client keys (`side: client` rows of `routing.json`). Java: `Routing.route(String key)` →
  route id or -1, `Routing.kind(id)` (edge/body), `Routing.on(id)`. Rust: `horizon_java_host::routes()`.
- **GUI bridges, a Java-bodied variant of W12's stub.** W5 ships `RES/gui_bridges.tsv`: `key \t seam`, one row per vanilla
  method that has a native counterpart in a shell (each SPEC-2 step method of `ShellLeaves.methods`, the hovered query, the
  handlers below). `BridgeProcessor` treats a row like a bridge whose native body is Java: the stub's "backed" test is
  `dev.horizonlab.javamods.boot.GuiRoutes.backed(this)` (a shell being recorded or handling input), and its body call is
  `GuiRoutes.body(seam, this, args)` instead of a JNI native. For a row not routed, the stub is today's plain bridge head (a
  shell never runs vanilla's body); for an edge-routed row, it is W12's route stub (HEAD handlers, body, RETURN/TAIL handlers,
  exact TAIL); for a body-routed row, no stub (W12's rule: the body runs in Java).
- **The gate list.** Each native site that serves a GUI key carries `gui_route!("key")` (a marker: it expands to W12's
  `static Gate` only), so `java_routing.py gates` lists it and the lint stops asking for a `deferred.tsv` row. Client keys
  without a W5 site stay deferred as `client: phase C`.
Send this section to the W12 designer before starting; if W12 changes a name, follow W12.

## 2. Steps of the shell's recording (`B/Shells.java`, `B/GuiRoutes.java`)

`Shells.routes(shell, leaf)` (SPEC-2) becomes: any key of `ShellLeaves.methods(shell.getClass(), leaf)` is `on`.
`step(leaf)` then calls the method on the shell through an `@Invoker` (`B/mixin/ShellInvokers.java`: `extractBackground`,
`extractLabels`, `extractSlots`, `extractSlotHighlightBack/Front`, `extractCarriedItem`, `extractTooltip`, and SPEC-5's
class-specific ones):
- **body**: it draws in Java; set the `routed` bit; native skips its step. Items it draws are `ITEM` entries native draws
  (item drawing stays native, D11).
- **edge**: the stub's body is `GuiRoutes.body(seam)` = `Shells.nativeStep()`: cut the recording there. HEAD handlers' drawing
  lands in the gap before the step, RETURN/TAIL handlers' in the gap after, native draws its own step between. A HEAD handler
  that cancels (the body never ran) sets the `routed` bit with empty content: native skips its step, as vanilla drew nothing.
- **Structure methods** (`Screen#extractRenderStateWithTooltipAndSubtitles`, `AbstractContainerScreen#extractRenderState`,
  `#extractContents`, `MerchantScreen#extractContents`...) are not steps. When one is routed, `record` calls it instead of the
  span of steps it covers: edge → its stub body runs SPEC-2's step sequence for that span; body → vanilla's structure runs in
  Java and each step method it calls goes through its GUI bridge (not routed: `nativeStep` cut, native draws; routed: as
  above). List each structure method's span in `ShellLeaves` (`span(key) -> int[] {firstLeaf, lastLeaf}`).

## 3. The hovered slot (a recording, W12 §2.5)

Keys `AbstractContainerScreen#isHovering` (both overloads) and `#getHoveredSlot(DD)`. When one is `on`: in `apply`, Java
sets `hoveredSlot = getHoveredSlot(mouseX, mouseY)` (vanilla's, with the patch) instead of native's index; `ShellReply` and
the status carry `int hovered` (menu index, -1 none, -2 not routed). Rust: while the shell is open and `hovered != -2`,
`slot_at` for it returns Java's slot (`menu_slot`): highlight, tooltip and clicks follow it. Mark `slot_at` with
`gui_route!` for these keys. The frame request already runs on each cursor move.

## 4. Handler bodies and native ports on input (synchronous, W12 §2.5 Window)

- A routed handler of the shell's class (`mouseClicked`, `mouseReleased`, `mouseDragged`, `mouseScrolled`, `keyPressed`,
  `charTyped`, `keyReleased`): `ShellMixin`'s HEAD cancel is skipped for it (`GuiRoutes.handlerRouted(class, name)`).
  - body, or edge with RETURN/TAIL changes: vanilla's body runs in Java; its effects reach native through SPEC-6's outbox and
    state blocks (`ShellState`; `ShellCreative`, `ShellBook` sent back when changed); owner `JAVA` (rename `WIDGET` → `JAVA`).
  - edge with HEAD changes only: the stub's body is `GuiRoutes.body` = "native handles it": owner `NATIVE`; a HEAD cancel
    gives `JAVA`.
- `AbstractContainerScreen#slotClicked` routed (IPN): native's screen click (the `menu_click` call made by a click over a
  shell, not by the outbox) instead sends `Input::SlotClicked(slot, button, input)` (kind 22) in a `client_shell_input`;
  Java calls the merged `slotClicked` on the shell (its body reaches `handleContainerInput` → outbox → native performs).
  `gui_route!` on that call site.
- `BundleMouseActions#onMouseScrolled` routed (Shulker Box Tooltip): native's wheel over a bundle in a shell instead sends
  `Input::BundleScroll(sx, sy, slot)` (kind 23); Java calls the merged method on the shell's `BundleMouseActions` (made by
  `init`) for the hovered slot (the stack is there: SPEC-3's or SPEC-4's mirror); `toggleSelectedBundleItem`'s packet reaches
  native through the outbox (SPEC-6 already maps `SelectBundleItem`). `gui_route!` on native's bundle wheel.
- For these two the slot's stack must be in Java: a mirror exists only while a shell is open (D6); without one, nothing routes.

## 5. Boot report

After W12's line `Horizon: route <key> <edge|body> <on|off> (<mods>)`, W5 adds for client keys:
`served by shell step <LEAF>` / `hovered` / `handler <name>` / `slot click` / `bundle wheel`, or for keys W5 has no seam for
under `net/minecraft/client/gui/`: nothing (W12 reports them deferred).

## Showcase effects

| Key (showcase) | Kind (W12 rules) | Served by |
|---|---|---|
| `AbstractContainerScreen#extractTooltip` (shulkerboxtooltip: HEAD and RETURN injects + an INVOKE `wrap_operation`) | body | step `TOOLTIP` (SPEC-8 completes it) |
| `AbstractContainerScreen#isHovering` (shulkerboxtooltip HEAD) | edge | §3 hovered |
| `Screen#extractRenderStateWithTooltipAndSubtitles` (shulkerboxtooltip INVOKE inject) | body | §2 structure |
| `AbstractContainerScreen#extractContents` (architectury inject: check its `at`) | edge or body | §2 structure |
| `BundleMouseActions#onMouseScrolled` (shulkerboxtooltip `wrap_operation`) | body | §4 bundle wheel |
| `CreativeModeInventoryScreen#getTooltipFromContainerItem` (jade), `Screen#getTooltipFromItem` (ae2) | body / edge | tooltip lines, SPEC-8 |
| `GuiGraphicsExtractor#item` (ae2, refinedstorage) | edge (D11) | SPEC-12 |
| `EditBox#keyPressed`, `onClick`, `onDrag`, `getWordPosition` (puzzleslib) | per W12 | SPEC-13 |
| `AbstractContainerScreen#slotClicked`, `MerchantScreen#extractContents` (IPN, top-50) | per W12 | §4 slot click, §2 structure |

## Tests
- Java unit `U/GuiRoutesCheck.java` (no boot): with a fake `Routing` (package-private setter), `routes` answers per class and
  leaf; `span` of `AbstractContainerScreen#extractContents` is `WIDGETS..HIGHLIGHT_FRONT`; `handlerRouted` for an edge
  HEAD-only key answers "native handles" and for a body key "Java".
- Test mod (`J/testmod/`, client side, under a system property the `bridges` run sets): a mixin injecting HEAD and RETURN into
  `AbstractContainerScreen#extractLabels` that fills a 1x1 quad each; `bridges` opens the inventory once and the check
  `shell_edge_step_order` reads the shell's last recording: the HEAD quad in the gap before `LABELS`, the RETURN quad in the gap
  after, `routed` bit clear. A second mixin cancelling at HEAD: bit set, content empty (`shell_edge_step_cancel`). Add both to
  the required list.
- Rust: `ShellReply` with `hovered` and state blocks reads back; owner 1 is `Java`; `slot_at` follows Java's index;
  `Input::SlotClicked` and `BundleScroll` write what `events` reads.
- `java_mods.py check`, `unit`, `bridges`, `cargo test ... --bins java_`, clippy, default build; `java_routing.py lint`.

## Scenario (`scn-7.json`, showcase install from SPEC-11, `HORIZON_JAVA_DEBUG=1`)
SPEC-2's floor, survival, open the inventory, then a chest. Checks: `log`
`route net/minecraft/client/gui/screens/inventory/AbstractContainerScreen#isHovering.* on .*served by hovered` present since
start; `Java shell 3 opened`; no exceptions.

## User steps (showcase install)
1. A filled shulker box in a chest: hover it (preview with SPEC-8). Hold its lock key (Ctrl by default, check its config): the
   preview stays on that slot when the mouse moves away; release: it follows the mouse.
2. With it locked, scroll: the selected item in the preview moves.
3. Clicks, shift-clicks, drag-splits and number keys over that chest behave as vanilla; items stay put after reopening.
4. A bundle in the inventory: the wheel selects items as before.
5. The boot log names each patched screen method with its kind and what serves it.
