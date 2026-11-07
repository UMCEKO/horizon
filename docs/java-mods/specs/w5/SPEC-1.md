# SPEC-1 (W5): JEI beside the native survival inventory

Read first: `CLAUDE.md`, `/tmp/hz-poc/agents/RULES.md`, `docs/java-mods-map.md` sections 4 and 5, and
`/tmp/hz-poc/agents/w5/DESIGN.md` (why; the measured JEI calls are in its section 1). Clean room: `javap` only for NeoForge and JEI;
vanilla's decompiled sources are fine (`.local/minecraft/26.3/sources/client/`).

Paths: `B/` = `mods/horizon/src/java-mods-host/boot/dev/horizonlab/javamods/boot/`, `G/` = `crates/horizon-client/src/game/`,
`RES/` = `mods/horizon/src/java-mods-host/boot/res/`.

## Goal

With JEI loaded, opening the survival inventory (native) shows JEI's ingredient list to the right, and it works: wheel pages
it, a click on an item opens JEI's recipe view (Esc returns to the inventory), R/U over an item in the list or over an
inventory slot opens recipes/uses, the search box filters, and typing in it does not close the inventory.

How: the Java client opens a real `InventoryScreen` as a **shell** instead of `NativeScreen`. Mods' events fire on it;
vanilla's own drawing and handler bodies never run in it. Its recording is painted in 5 segments between native's layers.
Input over the inventory goes to Java first and comes back with who owns it.

Out of scope (later phases): creative, crafting table, other containers, recipe transfer, the HUD, mixin routing.

## Wire changes (big-endian, `H/wire.rs` `Out`/`In`)

1. `Input::Screen` (kind 6) gets a third field: `byte shell` (0 none, 1 survival inventory). Java: `ClientBridge.events` `case SCREEN`.
2. New `Input::ShellLayout` kind 16: `int leftPos, int topPos, int imageWidth, int imageHeight, int hovered` (menu slot index, -1 none).
3. Status (`ClientBridge.status`, read in `java_client_status`): after `tabsReady` append
   `int shell` (open shell kind, 0 none), `bool recorded` (+ `int n` = 5, then 5 x `bytes` segment), `bool shellAnimating`.
4. New ops, both replying `ShellReply` = `byte owner`, `bool recorded` (+ 5 x `bytes`), `bool animating`:
   - `client_shell_frame`: size (3 `int`), `float partial`, `double x, double y` (cursor, window pixels), layout (5 `int` as kind 16).
   - `client_shell_input`: size (3 `int`), layout (5 `int`), `bool nativeWidget`, `bool nativeHandles`, `int n` + `Input` entries
     (the same encoding `events` reads). `owner`: 0 `EVENT` (a listener consumed it before the shell's handler), 1 `WIDGET`
     (a mod widget took it), 2 `NATIVE`, 3 `NONE`. For `client_shell_frame` owner is always 3.

Segments, in paint order: 0 `PRE` (under native's screen), 1 `BACKGROUND` (after native's background), 2 `WIDGETS` (mods'
widgets), 3 `FOREGROUND` (after native's slots), 4 `POST` (deferred tooltips and `Render.Post`, on top). Each segment is a
complete frame in the existing GUI frame format (`int n` + entries), so `paint_frame` paints it as is.

## Java

### J1. `B/Recorder.java`: cut a recording into segments

Add `byte[] cut() throws IOException`: returns `bytes()` for the entries since the last cut, then resets `bytes` and `count`.
Unit check `RecorderCutCheck` (in `unit/dev/horizonlab/javamods/boot/`, like `BlockRendersCheck`): draw a fill, cut, draw two
fills, cut; the first cut has `n == 1`, the second `n == 2`, and the concatenated entries equal one uncut recording's.

### J2. `B/mixin/ShellAccess.java`: accessor for the shell's layout

```java
@Mixin(AbstractContainerScreen.class)
public interface ShellAccess {
    @Accessor("leftPos") void horizon$leftPos(int value);
    @Accessor("topPos") void horizon$topPos(int value);
    @Mutable @Accessor("imageWidth") void horizon$imageWidth(int value);
    @Mutable @Accessor("imageHeight") void horizon$imageHeight(int value);
    @Accessor("hoveredSlot") void horizon$hovered(Slot slot);
}
```

Plus `@Accessor("renderables") List<Renderable> horizon$renderables();` in a second accessor on `Screen` (`ScreenAccess`).
List both in `RES/horizon_java.client.mixins.json` `client` array.

### J3. `B/mixin/ShellMixin.java`: vanilla's handler bodies do not run in a shell

One mixin class, `@Mixin(value = {AbstractContainerScreen.class, AbstractRecipeBookScreen.class, InventoryScreen.class}, priority = 100)`,
HEAD injections with `cancellable = true` and `require = 0` (not every class declares every method). Each starts with
`if (!Shells.is(this)) return;` and then sets the return value from `Shells`:

| Method (26.3 names, see `javap` in DESIGN) | Shell answer |
|---|---|
| `mouseClicked(MouseButtonEvent, boolean)` | `Shells.clicked(this, event, doubleClick)` |
| `mouseReleased(MouseButtonEvent)` | `Shells.released(this, event)` |
| `mouseDragged(MouseButtonEvent, double, double)` | `Shells.dragged(this, event, dx, dy)` |
| `mouseScrolled(double, double, double, double)` | `Shells.scrolled(this, x, y, sx, sy)` |
| `keyPressed(KeyEvent)` | `Shells.key(this, event)` |
| `charTyped(CharacterEvent)` | `Shells.typed(this, event)` |
| `tick()` (final in `AbstractContainerScreen`), `containerTick()`, `removed()` | cancel, nothing |

`priority = 100` so the shell's HEAD returns before a mod's injection in the same method; such mods are phase 4.
Do not touch `init`, the getters, or `Screen` itself. List it in the `client` array.

### J4. `B/Shells.java`: the shell

State: `static Screen current` (the open shell or null), `static Set<Object> vanilla` (identity set of the widgets vanilla's
`init()` made), `static int[] layout = {0,0,176,166,-1}`, `static boolean nativeWidget, nativeHandles`, `static int owner`,
`static byte[][] last` (5 segments last sent), `static boolean animating`.

- `static boolean is(Object screen)`: `screen != null && screen == current`.
- `static boolean wanted()`: whether a mod listens to any of these on `NeoForge.EVENT_BUS`: `ScreenEvent.Opening`,
  `Init.Pre`, `Init.Post`, `Render.Pre`, `Render.Background`, `Render.Foreground`, `Render.Post`, `MouseButtonPressed.Pre/Post`,
  `MouseButtonReleased.Pre/Post`, `MouseScrolled.Pre/Post`, `MouseDragged.Pre/Post`, `KeyPressed.Pre/Post`, `KeyReleased.Pre/Post`,
  `CharacterTyped.Pre/Post`, `Closing`. Read the lists as `WorldFrame.listened` does (`EventBus.getListenerList`). A listener
  counts when its owner is a mod's: unwrap `IWrapperListener.getWithoutCheck()`, then for `ConsumerEventHandler` read its
  protected `consumer` field; the owner is that object's class; it is a mod's when `owner.getModule().getName()` is none of
  `"neoforge"`, `"minecraft"`, `Shells.class.getModule().getName()` (also skip unnamed-module classes from `java.`/`jdk.`).
  For any other listener type, use the listener's own class. Log once per distinct owner class:
  `Horizon: screen events heard by <class>`. This is evaluated at each open, never on a timer: JEI registers its listeners
  only when its runtime starts after join (DESIGN 1).
- `static void open(int kind, Minecraft mc)`: only `kind == 1` and `wanted()`: `var shell = new InventoryScreen(mc.player)`,
  `current = shell`, `mc.gui.setScreen(shell)`. If afterwards `mc.gui.screen() != shell` (`Opening` cancelled or replaced it),
  set `current = null`. Otherwise fall back to today's `NativeScreen`.
- `static void close(Minecraft mc)`: if `is(mc.gui.screen())`, `mc.gui.setScreen(null)`; then `current = null`.
  Keep `current` set while a mod's screen (JEI's recipe view) covers the shell: JEI sets the same object back on close.
- Init bookkeeping, registered in `ClientBridge.install` **after** the existing `decide()` calls:
  - `ScreenEvent.Init.Pre`, `EventPriority.LOWEST`, `receiveCanceled = true`: if `is(screen)`, `before = identity copy of screen.children()`.
  - `ScreenEvent.Init.Post`, `EventPriority.HIGHEST`: if `is(screen)`: `vanilla = children() minus before` (identity), plus every
    `renderables` entry present now; then `apply(screen)`. So mods' `Init.Post` listeners already see native's layout.
- `static void apply(Screen s)`: write `layout` with `ShellAccess` (`leftPos`, `topPos`, `imageWidth`, `imageHeight`) and
  `hoveredSlot` = `layout[4] < 0 ? null : menu.getSlot(layout[4])` (guard the index against `menu.slots.size()`).
- `static byte[][] record(Minecraft mc, int mx, int my, float partial)`: one `Recorder`, one
  `GuiGraphicsExtractor(mc, recorder, mx, my)`, `apply(current)` first, then in vanilla's order (DESIGN 1 table):
  1. post `ScreenEvent.Render.Pre(shell, g, mx, my, partial)`; `cut()` → segment 0. If cancelled, return 5 empty segments
     (log once; native still draws its screen).
  2. `g.nextStratum()`; post `Render.Background`; `cut()` → segment 1.
  3. `g.nextStratum()`; every `renderables` entry not in `vanilla`: `extractRenderState(g, mx, my, partial)`; `cut()` → segment 2.
  4. post `Render.Foreground`; `cut()` → segment 3.
  5. `g.extractDeferredElements(mx, my, partial)`; post `Render.Post`; `cut()` → segment 4.
  Catch `Throwable` around the whole thing like `ClientBridge.hud()` (log "a mod's screen overlay failed", return null).
- `static void noteRecorded(byte[][] segs)`: `animating` as `ClientBridge.noted` does for the HUD (differs from `last` → true;
  alike → `HudAnimation.ends(true, timer.getRealtimeDeltaTicks())`); `changed = !Arrays.deepEquals(segs, last)`; `last = segs`.
- Dispatch (mods' widgets = `children()` entries not in `vanilla`, in list order; follow vanilla's
  `ContainerEventHandler` defaults for focus and dragging; read its source):
  - `clicked`: `owner = NATIVE` and return true if `nativeWidget`; else the first mod widget whose `mouseClicked` returns true:
    `setFocused(it)`, `setDragging(true)` for button 0, `owner = WIDGET`, return true; else `owner = nativeHandles ? NATIVE : NONE`,
    return `nativeHandles`.
  - `released`: `setDragging(false)`; focused mod widget's `mouseReleased` true → `WIDGET`; else as above with `nativeHandles`.
  - `dragged`: if dragging and the focused widget is a mod's: its `mouseDragged` → `WIDGET`; else `nativeHandles`.
  - `scrolled`: mod widget under the mouse (`getChildAt`, skipping vanilla's) `mouseScrolled` true → `WIDGET`; else `nativeHandles`.
  - `key`: Escape → `nativeHandles` (native closes). Else the focused mod widget's `keyPressed` → `WIDGET`; else `nativeHandles`.
  - `typed`: the focused mod widget's `charTyped` → `WIDGET`; else `nativeHandles`.
  - Each sets `owner`; `ClientBridge` resets `owner = EVENT` before replaying the input, so an input whose handler never ran
    (a `Pre` listener cancelled it) answers `EVENT`.

### J5. `B/ClientBridge.java`

- `case SCREEN`: read the new `byte shell`; `nativeScreen(open, pauses, shell)`: when opening with `shell != 0`, call
  `Shells.open(shell, minecraft)` and only when that leaves `Shells.current == null` open `NativeScreen` as today. When closing,
  `Shells.close(minecraft)` before today's `NativeScreen` check. Keep the `pause` line.
- New `case SHELL_LAYOUT` (kind 16): read 5 ints into `Shells.layout`.
- `javaScreen()`: also return null when `Shells.is(screen)`.
- `status(...)`: after `tabsReady`, write `int` (`Shells.current != null && Shells.is(minecraft.gui.screen()) ? 1 : 0`), then the
  tick's recording: in `tick()`, after `hud()`, if the shell is the current screen, `segs = Shells.record(mc, mouseX, mouseY, partial)`
  using the scaled mouse as `record` does; `Shells.noteRecorded(segs)`; write `bool recorded` = changed, the 5 segments, `bool animating`.
  `input()` and `frame()` call `status(null, null)`: write `0, false, false` there (no shell data).
- `OPS.put("client_shell_frame", in -> shellFrame(in))`: `size(in)`, partial, cursor; `receive()`;
  `minecraft.mouseHandler.onMove(window, x, y)` then `handleAccumulatedMovement()` (check the 26.3 signatures with javap; this is
  what `frame()` does for Java screens), read the layout, `Shells.record`, `noteRecorded`, reply `ShellReply` with owner 3.
  If the shell isn't the current screen, reply `3, false, false`.
- `OPS.put("client_shell_input", in -> shellInput(in))`: `size(in)`, layout, `Shells.nativeWidget`, `Shells.nativeHandles`,
  `Shells.owner = EVENT`, then `events(in)` (the real `MouseHandler`/`KeyboardHandler` paths fire `InputEvent`s, screen `Pre`,
  the shell's handler, `Post`), `receive()`, `chunkEvents()`, then record and reply `ShellReply` with `Shells.owner`.
  New ops go at the end of `OPS` (Rust finds them by name; `Crossing.client` indexes them).

### J6. Java screens fire `Render.Pre`/`Post` too

In `frame()`, replace `screen.extractRenderStateWithTooltipAndSubtitles(...)` with NeoForge's own
`ClientHooks.extractScreen(screen, screenLayers, graphics, mx, my, partial)`, where `screenLayers` is `Gui`'s private
`Stack<Screen> screenLayers` (add an accessor to `Gui`). Check with `javap` that it draws the screen itself (the run showed 0
calls today); keep `graphics.extractDeferredElements` only if `extractScreen` does not already reach it (vanilla's
`extractRenderStateWithTooltipAndSubtitles` ends with it; do not draw deferred elements twice: count TEXT entries of a
tooltip in a JFR-free debug run or a unit check).

## Rust

### R1. `G/java_screens.rs`: state and wire

- `Input::Screen(bool, bool)` → `Input::Screen(bool, bool, u8)`, written as `bool, bool, byte`. `Sent.native_screen` becomes
  `Option<(bool, bool, u8)>`.
- `Input::ShellLayout([i32; 5])`, kind 16. `Sent` gets `layout: Option<[i32; 5]>`.
- `Client` gets `shell: Shell`:
  ```rust
  #[derive(Default)]
  struct Shell {
      open: bool,
      segments: [Vec<u8>; 5],
      animating: bool,
      /// The cursor of the last recording, in GUI pixels.
      drawn_at: Option<(f32, f32)>,
      /// The tick that opened it has not answered yet: the next frame waits for it.
      opening: bool,
  }
  ```
- `java_client_status`: after `world_animating`, read `reply.bool()?` (tabsReady, unused), then `int shell`, the optional
  5 segments, `bool animating` into `c.shell`. Then the screen switch becomes:
  - `open` (a Java screen) as today.
  - `!open && self.screen == Screen::Java && shell == 1` → `self.screen = Screen::Inventory` (JEI's recipe view closed back to
    the shell), `self.capture(false)`; no other open side effects.
  - `!open && self.screen == Screen::Java && shell == 0` → today's `Screen::Playing`.
  - `shell == 0 && c.sent.native_screen == Some((true, _, 1))` and `self.screen == Screen::Inventory` → the shell was refused
    by an `Opening` listener: nothing (native's inventory stays, it is `NativeScreen` in Java); log once with `HORIZON_JAVA_DEBUG`.
  When `open` turns true while the native inventory was showing, do **not** run native's container close (vanilla keeps
  `inventoryMenu` open under JEI's screen); today's code only sets `self.screen`, keep it that way.
- A test in the `tests` module: a status with `shell = 1`, two non-empty segments and three empty ones reads back into `Shell`.
  Another: `Input::Screen(true, false, 1)` and `Input::ShellLayout` write the bytes `events` expects.

### R2. Which native screen is a shell

```rust
/// The vanilla screen a Java shell stands for: 1 the survival inventory.
fn java_shell_kind(&self) -> u8
```
1 when `self.screen == Screen::Inventory && self.container.is_none() && self.craft_size == 2 && !self.mount_active()`
and no station or native mod screen is shown (use the same conditions `draw_inventory` uses to pick the plain inventory). Else 0.
In `java_client_tick_send`, `native_screen` becomes `(open, open && self.pause, if open { self.java_shell_kind() } else { 0 })`;
when it changes to a non-zero kind set `c.shell.opening = true`. Also write `Input::ShellLayout` when `java_shell_layout()`
differs from `c.sent.layout`.

```rust
/// leftPos, topPos, imageWidth, imageHeight and the hovered menu slot, as native lays the screen out now.
fn java_shell_layout(&self) -> [i32; 5]
```
From a `HudBatch::new(window size)`: `inventory_origin(&batch)` (floor to `i32`), 176, 166, and
`self.slot_at(&batch, self.cursor.0, self.cursor.1).and_then(|s| Self::menu_slot_index("inventory", s))` or -1.
`fn java_shell_open(&self) -> bool`: `self.screen == Screen::Inventory && CLIENT.with_borrow(|c| c.shell.open)`.

### R3. Frames

- `java_client_frame`: if `c.shell.opening`, collect the running tick job with `wait = true` once (as an open Java screen's
  frame waits), then clear `opening`.
- `fn java_shell_frame(&mut self)`, called from `draw_inventory` before the first segment: returns at once unless
  `java_shell_open()`. Asks `client_shell_frame` only when `!java_client_busy()` and (`drawn_at != Some(self.cursor)` or
  `animating`) and no tick status was applied this frame (`recorded`, as `hud_frame_wanted`). Request: `java_client_size`,
  `self.clock.fraction()`, the cursor in window pixels (`self.cursor * scale`, as `java_cursor` computes), `java_shell_layout()`.
  Apply the reply (shared `fn java_shell_apply(reply: &mut In) -> Option<u8>` returning the owner) and set `drawn_at`.
- `fn draw_java_shell(&mut self, batch: &mut HudBatch, segment: usize)`: paints `c.shell.segments[segment]` with `paint_frame`
  and `java_screen_item` exactly as `draw_java_hud` does. No Java call.
- `G/` `game.rs` `draw_inventory`, survival inventory path only (the branch where `container` is none), insert:
  - `self.java_shell_frame(); self.draw_java_shell(batch, 0);` before the inventory background is drawn;
  - `draw_java_shell(batch, 1)` right after the background block (before the player model and slots);
  - `draw_java_shell(batch, 2)` after `self.draw_recipe_book(batch);`;
  - `draw_java_shell(batch, 3)` after the hovered-slot highlight fill, before `self.draw_carried(batch);`;
  - `draw_java_shell(batch, 4)` at the end of `draw_inventory` (after effects).
  Add the stubs to the `cfg(not(feature = "java-mods"))` arm at the end of `java_screens.rs`.

### R4. Input

One helper, synchronous like `java_input`:
```rust
/// Input over a shell: Java's screen events and mods' widgets first, in vanilla's order; native acts only when this says `Native`.
fn java_shell_input(&mut self, inputs: &[Input], native_widget: bool, native_handles: bool) -> ShellOwner
```
Writes size, `java_shell_layout()`, the two bools, the inputs (prepend `java_cursor()` for buttons and wheel, as
`java_screen_click` does), requests `client_shell_input`, applies the reply (segments, animating), returns the owner
(`enum ShellOwner { Event, Widget, Native, None }`; anything unreadable → `Native`, so a Java failure never eats input).
Without a host or with the shell closed it returns `Native` without calling.

Hook it in `game.rs` `window_event`, each guarded by `self.java_shell_open()`, before today's handling of that event for
`Screen::Inventory`:

| Event | Inputs | `native_widget` | `native_handles` | On non-`Native` |
|---|---|---|---|---|
| `MouseInput` press/release (top of the arm, before the `container_mouse_key` block) | `Button` | cursor in the recipe book button or open book (factor the hit test out of `recipe_book_click` into `fn recipe_book_hit(&self, x, y) -> bool`) | true (vanilla's `AbstractContainerScreen.mouseClicked`/`mouseReleased` take every click; read them) | return, native does nothing |
| `MouseWheel` | `Scroll` per step | false | the cursor is over the open recipe book, or over a slot holding a bundle (vanilla's `ItemSlotMouseAction`) | skip native's scroll |
| `KeyboardInput` press/repeat/release | `Key` (vanilla name, action, `java_modifiers`) | false | `fn native_inventory_key(&self, code) -> bool`: Escape; the inventory key; hotbar 1-9, swap-offhand, drop or pick-item while a slot is hovered; any key while the recipe book search is focused (`recipe_search_focused`). Mirror `AbstractContainerScreen.keyPressed` and `AbstractRecipeBookScreen.keyPressed` | skip `key_pressed` and the native recipe-search typing |
| typed text (`KeyEvent.text`, per char, after its key) | `Char` | false | `recipe_search_focused()` | skip native typing |

Releases always send `native_handles = true` for buttons and `false` for keys (vanilla's `keyReleased` default only reaches
focused widgets). While the shell is open, `java_window_input` and `java_key_input` must not also queue the same key as
`KeyState`: return early when `java_shell_open()`.

`CursorMoved` needs nothing: native updates `self.cursor`; the next frame records if it moved (R3).

## Checks

- `python3 tools/java_mods.py unit` (`RecorderCutCheck`), `java_mods.py check`, `cargo test -p horizon-client --features java-mods --bins java_`,
  clippy `-D warnings` on `horizon-client` with `java-mods`, and the default build (stubs).
- Boot log on the RS pack: `Horizon: screen events heard by mezz.jei...` lines only from mods, none from NeoForge or Horizon.
- No live verification by you beyond one short run that the shell opens (`HORIZON_JAVA_DEBUG=1`, `hz_scenario.py` with
  `{"key": "key.inventory"}`, check the log for no exceptions); the parent and the user test the rest.

## How the parent and the user test it (`/tmp/hz-poc/play/install-rs`, survival)

1. `/gamemode survival`, press E. Expect: JEI's list right of the inventory on the first frame, the page counter, the search box
   bottom right, bookmark buttons bottom left. Inventory looks exactly as before.
2. Wheel over the list: pages (counter changes). Wheel over the inventory: nothing new.
3. Hover a list item: JEI's highlight and tooltip follow the mouse without lag.
4. Click a list item: JEI's recipe view opens. Esc: back to the native inventory, items untouched (crafting grid contents kept).
5. Hover an inventory slot holding oak logs, press R then U: recipes, then uses, of oak logs.
6. Click the search box, type `stone`: the list filters; the `e` in it does not close the inventory. Esc or a click elsewhere
   unfocuses it; then E closes the inventory.
7. Click and shift-click inventory slots: native behaviour unchanged (moves, crafting 2x2).
8. Open the recipe book (green book button): JEI's list stays right; the book button still works (native wins over any mod widget).
9. Without JEI (`install` without it): E opens the inventory with no Java call per frame (`HORIZON_JAVA_DEBUG` shows no shell lines).

Visual and real-input checks are the user's (RULES "Visual and real-input checks go to the user").
