# SPEC-9 (W4): `ItemAttributeModifierEvent` at native's equipment collection (server)

Design: `DESIGN.md` §1.3 (Mekanism step-assist row), §2.5, Q4 (W13's table gives this event to W11; written here because W4's
accept needs it; the lead may move it). Size: ~330 changed lines (Java ~90, Rust ~240). Depends on: nothing.

Read first: `G/game.rs` `apply_equipment` (13865–13900) and `equipment_modifier_applies`, `G/lan_attributes.rs`
`equipment_attributes` (124) and `mob_attributes`, `G/native_mobs.rs` 17 and 59 (the mobs' and players'
`collectEquipmentChanges`), `B/GameBridge.java` (`register`, op pattern, how a request's `bytes tag` becomes an `ItemStack`:
`GameBridge.fill` reads inventory stacks the same way), `G/java_mods.rs` (`java_level_running`, where the server's Java side is
known started). Vanilla: `LivingEntity.collectEquipmentChanges`, `ItemStack.forEachModifier(EquipmentSlot, …)`,
`ItemAttributeModifiers`. NeoForge (javap): `IItemStackExtension.getAttributeModifiers()`,
`CommonHooks.computeModifiedAttributes(ItemStack, ItemAttributeModifiers)` (posts the event).

Code comments: one line at most.

## Goal

When a mod listens to `ItemAttributeModifierEvent` (Mekanism does), the attribute modifiers native applies for a worn or held
stack are the stack's real ones as NeoForge computes them (default component, then the event), asked once per distinct stack and
cached. Native then applies, saves and syncs them as any equipment modifier, so the client's movement reads them (step height,
jump strength, speed) from native's attribute packets.

Accept: Meka-Suit boots' step assist and the Free Runners' step height work on Horizon as on Forge.

## Java (~90 lines)
- `B/GameBridge.java`: op `item_attribute_modifiers` (a `Crossing.game` op, server thread): request `int n` × `bytes tag`; for each,
  the `ItemStack` from the tag (the existing stack decoder), `stack.getAttributeModifiers()` (NeoForge's extension: the component
  then the event); reply `int n` × (`int m` × (`text attribute`, `text id`, `double amount`, `byte operation` 0 add_value /
  1 add_multiplied_base / 2 add_multiplied_total, `text group` as `EquipmentSlotGroup.getSerializedName()`)). An empty stack replies
  `m = 0`.
- `static boolean attributeEventHeard()`: `Listeners.modListens(NeoForge.EVENT_BUS, List.of(ItemAttributeModifierEvent.class), …)`;
  reported to Rust in the reply of the op that tells Rust the Java server started (append `bool` to it, both sides, map §5), and
  logged `Horizon: item attribute modifiers heard by <owner>`.

## Rust (~240 lines)
- `G/java_mods.rs`: `JAVA_SERVER` state gains `attribute_event: bool` (from the start reply) and `stack_modifiers:
  HashMap<u64, Arc<[StackModifier]>>` keyed by a hash of the stack's full tag (item, count ignored, components); cleared when the
  server stops and on a data reload.
- `G/java_hooks.rs` two-arm: `pub(super) fn java_hook_stack_modifiers(&mut self, stack: &ItemStack) -> Option<Arc<[StackModifier]>>`:
  `None` without the feature or when `attribute_event` is false; else the cache, asking `item_attribute_modifiers` for a miss (one
  request per call site batch: collect the misses of one equipment pass and ask once).
- A seam for "the modifiers of this stack": `fn stack_attribute_modifiers(&mut self, stack: &ItemStack) -> Cow<'_, [ModifierRow]>`
  (in `G/lan_attributes.rs`), returning Java's when `Some`, else the item type's `info.modifiers` as today. Use it in
  `apply_equipment`, `equipment_attributes` and the two `native_mobs.rs` sites in place of `world_items!(self).get(&s.item)…modifiers`.
  The slot filter (`equipment_modifier_applies` / `slot_in_group`) applies to Java's rows by their group name unchanged.
- Changing a worn stack's components (a Mekanism module toggled) changes its tag hash, so the next equipment pass asks again; vanilla
  also recomputes on every `collectEquipmentChanges` when the stack differs (`ItemStack.matches`).

## Test mod (~35 lines)
Item `horizon_java_test:step_boots` (equippable `FEET`). `ItemAttributeModifierEvent` listener: for that item,
`event.addModifier(Attributes.STEP_HEIGHT, new AttributeModifier(Identifier.parse("horizon_java_test:step"), 0.5, ADD_VALUE),
EquipmentSlotGroup.FEET)`.
Checks (server, `ChecksPlayer.later`): `step_boots_attribute`: wearing them, native's `/attribute @s minecraft:step_height get`
answers 1.1 (vanilla base 0.6 + 0.5); `sword_unchanged`: holding a vanilla iron sword, `/attribute @s minecraft:attack_damage get`
answers 6.0 (base 1 + the sword's 5), the same with and without the listener (Java's rows for a vanilla stack equal native's).
Drive: `item replace entity @s armor.feet with horizon_java_test:step_boots`, `wait 5`, the commands, then a one-block ledge walk
(`fill` a step, `key w down`, `wait 20`, `key w up`): check `stepped_up_one_block` from the window's `state` (y rose by exactly 1.0).

## Tests
- Rust: the reply parser (bytes typed in the test); the seam returns Java's rows when cached and native's when the event isn't heard;
  a cache hit makes no request (count requests through a test sink); two stacks differing only in a component get two entries.
- `cargo test -p horizon-client --features java-mods --lib lan_attributes java_mods`, clippy with and without the feature,
  `java_mods.py build` (private dir), `unit`, `check`, `bridges`.
- Harnesses: `server_entities`, `attributes` (or what `parity.py --changed-files` names for `G/lan_attributes.rs`, at most two):
  unchanged (no Java in them).

## Done bar
Tests pass; harnesses unchanged; `bridges` checks ok; install-rs boot log shows Mekanism as a listener.

## Live test (lead; play install, survival)
1. Meka-Suit boots with Hydraulic Propulsion's step assist set to each level: the player walks up 1-block (and higher, per level)
   steps without jumping, as on Forge; turning it off restores vanilla's 0.6.
2. `/attribute @s minecraft:step_height get` matches Forge's value for the same setting.
3. Hover the boots in the inventory: the tooltip's attribute lines (Java's tooltip, W5/W6) and the behaviour agree.
