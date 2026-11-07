# SPEC-11 (W5): the showcase install, the W5 acceptance scenario, and Visual Workbench (revision 2)

Read first: `CLAUDE.md`, `RULES.md` (games, the gate, memory), `docs/java-mods-map.md` section 7, `tools/hz_scenario.py --help`,
the showcase report `/home/umceko/hz-poc/showcase-pack/replaces/replaces.md`. No game code changes unless a check below fails
and the cause is in a W5 file; otherwise report the evidence.

Depends on: nothing for §1 (do it first: SPEC-6..13 test on this install). §2 after each spec lands. §3 any time.

## 1. A showcase install

The showcase pack (46 mods) is `/home/umceko/hz-poc/play/pack3` (symlinks to jars). It has a report but no install.
- `cd /projects/horizonmc && TMPDIR=/tmp nice nix-shell -p jdk25 pkg-config --run 'python3 tools/java_mods.py install --pack /home/umceko/hz-poc/play/pack3 --out /home/umceko/hz-poc/showcase-pack/install --java <install-rs JDK home>'`
  (reuse `install-rs`'s JDK and the download cache; `install-rs/install.json` names the JDK). The install writes
  `analysis/` (`mod_replaces.install_analysis`): check `analysis/entity-facts.json` exists and the run printed no refused jar.
- Boot it once: `java_mods.py run --install /home/umceko/hz-poc/showcase-pack/install` with the gate exported
  (`HORIZON_GAME_GATE`, `HORIZON_GFX_ENV`, `GATE_OWNER=w5-showcase`). Report boot time, the refused or failed mods if any
  (`python3 tools/logsum.py LOG --grep 'refus|failed|threw'`), and the `screen events heard by` lines (expect
  architectury, balm or puzzleslib helpers, jei, mousetweaks, sophisticatedcore, travelersbackpack, kuma_api).
- A user-level note for the report: "Inventory Tweaks" is not in the pack and has no 26.3 build; Inventory Profiles Next
  (top-50 pack, `/home/umceko/hz-poc/top50/pack/mods`) is the modern equivalent and is what SPEC-6 and SPEC-7 cite.

## 2. The acceptance scenario: `tools/scenarios/w5_screens.json`

One scenario that opens every W5 surface and checks the log, for the parent to rerun after each W5 merge
(`python3 tools/hz_scenario.py run --install /home/umceko/hz-poc/showcase-pack/install --script tools/scenarios/w5_screens.json --out OUT`,
`HORIZON_JAVA_DEBUG=1`). Steps, each block followed by `{"check": {"no_exceptions": true}}` and a screenshot:
1. Floor (forceload, fill, tp, `near`), survival, `/give` bread, an enchanted book, a shulker box with diamonds,
   64 cobblestone; `{"wait": 200}` (JEI's runtime).
2. Inventory (`key.inventory`): `Java shell 1 opened`; with SPEC-8: `routes .*TOOLTIP`.
3. `/gamemode creative`, inventory: `Java shell 2 opened` (SPEC-3). `/gamemode survival`.
4. For chest, furnace, hopper, shulker box, anvil, enchanting table, brewing stand, beacon, stonecutter, loom, smithing table,
   grindstone, cartography table, crafter: setblock, `lookat`, `use`, `Java shell 3 opened`, close (SPEC-4, SPEC-5).
5. A librarian villager with `NoAI` (SPEC-5's command): `Java shell 3 opened`.
6. HUD: `/effect give @s minecraft:hunger 30 2`, a boss bar, a title, a sidebar (SPEC-9, SPEC-10); screenshot.
7. Visual Workbench (§3): a vanilla crafting table, `use` it.
8. Items and text boxes (SPEC-12, SPEC-13): an AE2 pattern in the inventory; creative with the search tab.
9. End: `{"check": {"log": "a forwarded .* failed|lacks its state|the shell's slot .* holds", "present": false, "since": "start"}}`,
   and the W12 boot report for client keys: every `route net/minecraft/client/gui/... on` line has a W5 `served by` or a
   W12 deferral reason (`python3 tools/logsum.py LOG --grep 'route net/minecraft/client/gui'`).
Steps for specs not merged yet are left out by the parent; mark each block with a `{"name": "SPEC-n ..."}` entry.

## 3. Visual Workbench: what it needs and what to verify

From the report and `javap` (signatures only): Visual Workbench has no screen-event listeners and no mixins. It registers its
own block (`CraftingTableWithInventoryBlock`, a block entity that keeps the grid), its own menu type and menu
(`VisualCraftingMenu extends CraftingMenu`, overriding `getType`), converts vanilla crafting tables to its block when used
(`BlockConversionHandler.onUseBlock`, through puzzleslib's `UseBlock`, i.e. NeoForge's `PlayerInteractEvent.RightClickBlock`),
draws the grid's items on top of the table (`CraftingTableBlockEntityRenderer`), and its client shows vanilla's
`CraftingScreen` for its menu type (registered by puzzleslib's `RegisterMenuScreensEvent`). So the whole screen is a mod's menu
served by the Java server and drawn as a Java screen today; W5 shells are not involved (Decision D10). Verify, one run:
- Place a vanilla crafting table, right-click it: the log shows the Java server's menu open and the Java client's
  `CraftingScreen` (debug lines of Java screens); no "cannot show" chat line; no exception.
- `data get block X Y Z` after the click: the block id is Visual Workbench's (the conversion ran); if it is still
  `minecraft:crafting_table`, report whether `RightClickBlock` fired for it (`python3 tools/mod_parity.py explain --run DIR`
  or a JFR filter on `BlockConversionHandler`), and stop: that is a server-event gap for W13, not W5.
Report the result; the visual parts are user steps.

## User steps (showcase install)

The union of SPEC-1..10's user steps, plus Visual Workbench:
1. Right-click a crafting table: the crafting screen opens (looks vanilla), JEI's list is beside it, JEI's `+` buttons show
   in recipe views (pressing them: DESIGN D4).
2. Put items in the grid and close: the items stay on the table, drawn on its top face (rotating or flat per its config).
   Reopen: they are still in the grid. Break the table: they drop.
3. Two players at one table see the same grid.
