# SPEC-8 (W11): enchantments that come from item code, for the tool of a loot roll

Read first: `DESIGN.md` G6, D7; `SPEC-1.md` (item bits 2, 3; event `GetEnchantmentLevel`); vanilla
`EnchantmentHelper.getItemEnchantmentLevel`, `ApplyBonusCount`, `EnchantedCountIncreaseFunction` (looting),
`MatchTool`/`ItemPredicate` enchantment sub-predicates; javap (patched) `EnchantmentHelper.getItemEnchantmentLevel` →
`ItemInstance.getEnchantmentLevel` (NeoForge's `IItemExtension#getEnchantmentLevel`, `GetEnchantmentLevelEvent`); Rust
`G/game.rs` `block_loot_at`, `G/creatures.rs` (the `EntityLootParams` built for a player kill, l. ~2132),
`G/spill.rs` (`block_drops_with` and its tool), `I/item_data.rs` `enchantments`, `B/Effects.java` `lootParams`.

Depends on: SPEC-1.

Comments in code: one line at most, only for a non-obvious why.

## Goal

When a native roll uses a tool whose item has enchantment code (SPEC-1 bits 2 or 3), or any tool while a mod listens to
`GetEnchantmentLevelEvent`, the roll sees the enchantment levels Java's `EnchantmentHelper.getItemEnchantmentLevel` gives
(the item's override, NeoForge's event, a mod's mixin on that method), through a copy of the tool used for that roll only.

## Step 1 (measure first, report the result)

On install-rs with a Meka-Tool that has the Silk Touch and Fortune modules installed (Modification Station), call the new
op once from a debug command or `HORIZON_JAVA_DEBUG` line and record which enchantments Java reports. If Mekanism's
modules don't surface as enchantment levels, G6 is not Mekanism's and the done bar's Meka-Tool lines become "no change"
(keep the test mod's check: the mechanism stays for any mod that does).

## Wire: op `tool_enchantments`

Request: `text dim`, `long time`, `int len` + the tool's NBT. Reply: `int n` + `(text enchantment id, int level)*` for every
registered enchantment (`level.registryAccess().lookupOrThrow(Registries.ENCHANTMENT).listElements()`) whose
`EnchantmentHelper.getItemEnchantmentLevel(holder, stack)` is above 0, in registry order.

## Java

- `B/Loot.java` `toolEnchantments(in, out)`, `Crossing.game("tool_enchantments", Loot::toolEnchantments)`.
- `static ItemStack effective(ItemStack tool, RegistryAccess access)`: a copy whose `DataComponents.ENCHANTMENTS` is the
  levels above (built with `ItemEnchantments.Mutable`). `B/Effects.java` `lootParams` flattens `effective(tool)` instead of
  the tool when `CraftFacts.enchantCode(tool.getItem())` or `GetEnchantmentLevelEvent` is listened (the facts SPEC-1
  computed, kept in `CraftFacts` statics): a mod block broken through Java then rolls natively with the same levels.

## Rust

- `G/java_loot.rs`: `pub(super) fn java_tool_for_loot(&mut self, tool: &ItemStack) -> Option<ItemStack>`: `None` unless the
  rule above holds; else `java_request("tool_enchantments", ..)` and a clone of `tool` with `data_mut().enchantments`
  replaced by the reply (sorted as the reply). No cache: one crossing per roll with such a tool.
- `java_hooks.rs`: `java_hook_tool_for_loot(&mut self, tool: Option<&ItemStack>) -> Option<ItemStack>` (None without the
  feature).
- Sites, at their start, before any `with_level`/`with_native_loot_place`:
  - `block_loot_at`: `let swapped = self.java_hook_tool_for_loot(held); let held = swapped.as_ref().or(held);`
  - the player-kill `EntityLootParams` in `G/creatures.rs`: the killer `LootEntity`'s `mainhand` `LootStack.enchantments`
    from the swapped stack (looting reads them through `LootData::enchantment_level`).
  - `G/spill.rs`'s `block_drops_with` call, when its tool is a player's held item.

## Tests

- **Test mod** (`java_mods.py play` drive or a `bridges` check that breaks through the native path): give the test mod's
  `charm` (SPEC-1: `getEnchantmentLevel` answers silk touch 1, fortune 3; no `enchantments` component), select it,
  `setblock` `minecraft:glass` in front, break it: a `minecraft:glass` item drops (vanilla's glass table drops only with
  silk touch; the expectation is vanilla's table, not this code). Without the charm (empty hand): nothing drops.
- **Java** `ChecksServer` `tool_enchantments`: the op's reply for a `charm` stack lists `minecraft:fortune` 3 and
  `minecraft:silk_touch` 1; for a plain diamond pickaxe, nothing.
- **Rust**: the gate (bits 2/3 or the event) is a pure function: false for an item with bit 0 only, true with bit 3, true for
  any item when the event is heard.
- `cargo test ... --lib java_loot`, `java_mods.py build`, `check`, `bridges`; clippy both ways; the block-drop parity
  harness that covers `block_loot_at` through `verify/parity.sh` (unchanged without mods).

## Done bar

- [ ] The glass check passes; with an empty hand no glass drops.
- [ ] Per step 1: a Meka-Tool with Silk Touch on breaks stone into stone (not cobblestone); with Fortune III on, diamond
      ore drops 1–4 diamonds (vanilla's `ore_drops` formula), as on Forge.
- [ ] W12: `deferred.tsv`'s `EnchantmentHelper#getItemEnchantmentLevel` row (AE2) stays `seam: ...item_data.rs:274` but
      gains a comment line in the tsv's notes (or the report) that loot rolls are served by W11 SPEC-8; ask the lead before
      changing its reason.

## Live steps for the lead

1. install-rs, survival: make a Meka-Tool, install Silk Touch in a Modification Station, mine stone: stone drops.
2. Swap Silk Touch for Fortune (max), mine 10 diamond ore: more than 10 diamonds in total on average (vanilla fortune III
   averages 2.2 per ore).
