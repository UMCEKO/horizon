# SPEC-10 (W5): HUD layers whose vanilla code mods patch, and the state Java-drawn layers read (revision 2)

Read first: `SPEC-9.md`, `SPEC-7.md` (GUI bridges, `Routing`), `/home/umceko/hz-poc/agents/w12/DESIGN.md`, `SPEC-4.md` (forwarding, `Input::Packet`), vanilla's
`Hud.java`, `BossHealthOverlay.java`. Paths as SPEC-2.

Depends on: SPEC-9, SPEC-7 (GUI bridges, W12's `routes.bin`), SPEC-4 (packet forwarding). Land right after SPEC-9: SPEC-9 can already
make Java draw a wrapped layer, and that layer needs the state below to look right.

## Goal

1. A vanilla HUD layer whose vanilla methods a loaded mod patches is drawn by Java with the patch, always, and native skips
   it (SPEC-9's `drawn` bit). Showcase and top-50 examples: Jade's `BossHealthOverlay#extractRenderState` and its
   `CustomizeGuiOverlayEvent.BossEventProgress` listener (the boss bars move below Jade's box), glitchcore's
   `Hud#extractCameraOverlays`, Inventory Profiles Next's `Hud#extractItemHotbar`.
2. A vanilla layer Java draws (routed here, or wrapped or re-placed by SPEC-9) reads the Java client's state; where the Java
   client does not learn that state today, the window forwards the server packets that carry it, as SPEC-4 does for menus.

## 1. Layer methods, from one trace (`B/HudLayers.java`)

Which vanilla methods each NeoForge layer calls is NeoForge's code, which we do not read. Get it from a runtime trace (allowed):
one `hz_scenario.py` run on the showcase install with
`HORIZON_JAVA_OPTS="-XX:StartFlightRecording=...,jdk.MethodTrace#enabled=true,jdk.MethodTrace#stackTrace=true,jdk.MethodTrace#filter=net.minecraft.client.gui.Hud;net.minecraft.client.gui.components.BossHealthOverlay;net.minecraft.client.gui.components.PlayerTabOverlay;net.minecraft.client.gui.components.SubtitleOverlay;net.minecraft.client.gui.components.spectator.SpectatorGui"`
(model: `w5/env.sh`), with a boss bar, a title, a sidebar and an effect shown (scenario below). For each traced method, the
`GuiLayerManager` lambda in its stack names the layer. Write the result as data, `HudLayers.methods(id)` (route keys with
descriptors, as `routes.bin`), with the trace file named in a one-line comment. Events posted inside a layer, from the same
stacks or NeoForge's public docs: `BOSS_OVERLAY` → `CustomizeGuiOverlayEvent.BossEventProgress`; `CHAT` →
`CustomizeGuiOverlayEvent.Chat` (`HudLayers.events(id)`).

## 2. Routing a layer (W12 kinds; `RES/gui_bridges.tsv`, SPEC-7 §1)

- Each layer method is a GUI bridge row (seam `layer <id>`); `GuiRoutes.backed` is true for the `Hud` while a HUD recording runs.
- A layer with a **body**-routed method, or whose event a mod listens to (checked at each join): SPEC-9's wrapper draws it in
  Java (`drawn` bit) every time.
- A layer whose methods are only **edge**-routed: the layer runs into the recording; the method's stub body is
  `GuiRoutes.body` = cut: HEAD handlers' drawing lands in the gap before the layer, RETURN/TAIL handlers' in the gap after,
  native draws its own layer between (no `drawn` bit). A HEAD cancel sets the layer's `cancelled` bit.
- `CHAT` and `TAB_LIST` stay native (W6): their keys stay W12-deferred.
- Mark native's layer sites with `gui_route!` for each key served (W12 gate list).
- `forwardHud` (status bit, after join): true when a layer is routed (any kind: edge handlers read state too) or a mod listens
  to `RenderGuiLayerEvent.Pre` or `.Post` (SPEC-9 may draw any layer then). That is the detection D6 asks for.

## 3. The state a Java-drawn layer reads

For each layer, read what vanilla's code reads (`Hud`, `BossHealthOverlay`, ...) and how the Java client learns it today
(the Java server's view of the player sends what `ServerPlayer` sends: check `GameBridge.fill`, `PlayerSync`, `Menus`). Fill
this table in the report; the rows marked "forward" are this spec's work:

| Layer | Reads | Java client learns it from | Work |
|---|---|---|---|
| `HOTBAR`, `SELECTED_ITEM_NAME` | inventory, selected slot, offhand, attack strength | server view's inventory menu; `Input::Hotbar`; attack strength: the window's attacks never reach the Java player (check) | report what is missing |
| `PLAYER_HEALTH`, `FOOD_LEVEL`, `AIR_LEVEL`, `ARMOR_LEVEL`, `EXPERIENCE_LEVEL` | health, food, saturation, air, armor value, xp | `ClientboundSetHealthPacket`, `SetExperience`, attributes from the server view (`PlayerSync`) | verify each |
| `VEHICLE_HEALTH` | the ridden entity | needs W2 (no entities in the Java level) | never Java-drawn until W2: log once |
| `EFFECTS` | the player's effects | `UpdateMobEffect`/`RemoveMobEffect` from the server view? | forward if not |
| `BOSS_OVERLAY` | boss events | nothing today | forward `BossEvent` |
| `OVERLAY_MESSAGE`, `TITLE` | action bar, titles | nothing today | forward `SetActionBarText`, `SystemChat` with `overlay`, `SetTitleText`, `SetSubtitleText`, `SetTitlesAnimation`, `ClearTitles` |
| `SCOREBOARD_SIDEBAR` | objectives, scores, teams | nothing today | forward `SetObjective`, `SetScore`, `ResetScore`, `SetDisplayObjective`, `SetPlayerTeam` |
| `CAMERA_OVERLAYS` | head slot (pumpkin), frozen ticks, portal, nausea, spyglass scoping, vignette | mixed: verify each | report what is missing |
| `SLEEP_OVERLAY`, `DEMO_OVERLAY`, `SPECTATOR_TOOLTIP`, contextual bars | sleep timer, demo, spectator menu, locator/jump bar | verify | report |

Forwarding: Rust, in `java_menu_packet`'s hook (SPEC-4 R1; rename it `java_forward_packet`), forwards these kinds as
`Input::Packet` while `forwardHud` is true, from the moment it turns true (send the current state once then: for the boss bar
and the sidebar, native keeps what it got; re-encode it as the packets that would rebuild it, `BossEvent` add, `SetObjective`
+ scores, and test that round trip). A layer still missing state after this spec is never Java-drawn: SPEC-9's wrapper treats
it as discarding and logs once `Horizon: <layer> changed by a mod, but the Java client lacks its state; native draws it`.

## Rust

Only the forwarding above, `forwardHud` in the status reader, and the re-encode of native's boss bars and sidebar.

## Tests

- Java unit (extend `GuiRoutesCheck`): with a fake `Routing`, a body key routes its layer as `drawn`, an edge key does not;
  a fake listener on `BossEventProgress` routes `BOSS_OVERLAY`; `forwardHud` follows the rules.
- Rust: `BossEvent` add, `SetObjective` and `SetScore` round trips through `encode`/`decode`; the status bit reads back.
- `java_mods.py check`, `unit`, `cargo test ... --bins java_`, clippy, default build.

## Scenario (`scn-10.json`, showcase install, `HORIZON_JAVA_DEBUG=1`; also the trace run of §1 with the JFR options)

SPEC-2's floor, survival, then `/bossbar add w5 "W5"`, `/bossbar set w5 players @a`, `/title @s title "W5"`,
`/scoreboard objectives add w5 dummy`, `/scoreboard objectives setdisplay sidebar w5`, `/scoreboard players set @s w5 3`,
`/effect give @s minecraft:speed 60`, `{"wait": 40}`, `{"screenshot": "hud"}`; checks: `log`
`BossHealthOverlay#extractRenderState on .*served by layer BOSS_OVERLAY` present since start; no exceptions;
`log` `lacks its state` absent.

## User steps (showcase install)

1. With the boss bar above showing, look at a block so Jade's box appears at the top: the boss bar sits where Jade puts it,
   not under the box; look away: it returns. The bar's name, colour and progress match `/bossbar set w5 value 50` changes.
2. Title, action bar (`/title @s actionbar "x"`), sidebar and effect icons look exactly as without mods.
3. Wear a carved pumpkin, stand in powder snow, look through a spyglass: the overlays look as before.
4. In the top-50 pack (IPN): IPN's hotbar additions (its locked-slot marks) show on the hotbar; scrolling the hotbar
   selection follows within a tick.
