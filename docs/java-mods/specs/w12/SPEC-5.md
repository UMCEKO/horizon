# W12 SPEC-5 (target T1): Carry On's patches of the player's inventory

Design: `DESIGN.md` §4 (T1). Needs SPEC-3 (`route!`, `route_call`, `setSelectedSlot` gated) and SPEC-4 (lint, `deferred.tsv`). Clean room: never read Carry On's source or decompiled classes. What its handlers do is observed, never read.

## Routes (from the showcase `replaces.json`; all `conditional`, config plugin `CarryOnMixinConfigPlugin`)

| Key | Change | Kind | Native site today |
|---|---|---|---|
| `Inventory#setSelectedSlot(I)V` | inject HEAD | edge | gated by SPEC-3 |
| `Inventory#pickSlot(I)V` | inject HEAD | edge | inline in `pick_item.rs` `pick` (`Some(slot) =>` arm), called from `lan_actions.rs` `guest_pick_item` |
| `Inventory#addAndPickItem(Lnet/minecraft/world/item/ItemStack;)V` | inject HEAD | edge | inline in `pick_item.rs` `pick` (`None if infinite_materials` arm) |
| `Inventory#getFreeSlot()I` | wrap_operation at `NonNullList.get` | body | inlined in every `Inventory.add` port: `game.rs` `give` (~2857), `game/native_server.rs` `add_to` (~442), `menu.rs` `inventory_add` (~1830), `cauldron.rs` (~20); and in `pick` |

`Player#readAdditionalSaveData` (RETURN) needs the save-stream family (T2). Leave it deferred `view: save_stream`.

## 1. Seams (vanilla first: read `Inventory.pickSlot`, `addAndPickItem`, `getFreeSlot`, `getSuitableHotbarSlot`, `findSlotMatchingItem` and `ServerGamePacketListenerImpl.tryPickItem` in `.local/minecraft/26.3/sources/server/`)
- **`pick_item.rs` (lib):** split `pick` into vanilla's pieces as free functions over `(items, selected)`:
  - `find_slot_matching_item`;
  - `suitable_hotbar_slot` (exists);
  - `pick_slot_body(items, selected, slot)`;
  - `add_and_pick_item_body(items, selected, stack, free_slot)`, where `free_slot` is the slot the caller got from `getFreeSlot`.

  Keep `pick` only if a remaining caller needs it (see the last bullet).
- **`game/inventory.rs`** (SPEC-3's file) gains the ports, each with its gate first and its body fn:
  ```rust
  /// `Inventory.pickSlot`: a main-inventory stack swaps into a suitable hotbar slot.
  pub(in crate::game) fn pick_slot(&mut self, slot: i32) { route!(self, "net/minecraft/world/entity/player/Inventory#pickSlot(I)V", Recv::Player, (slot)); self.pick_slot_body(slot) }
  /// `Inventory.addAndPickItem`: a creative pick puts the stack in a suitable hotbar slot.
  pub(in crate::game) fn add_and_pick_item(&mut self, stack: &ItemStack) { route!(…"#addAndPickItem(Lnet/minecraft/world/item/ItemStack;)V", Recv::Player, (stack)); self.add_and_pick_item_body(stack) }
  /// `Inventory.getFreeSlot`: the first empty main-inventory slot, or -1.
  pub(in crate::game) fn free_slot(&mut self) -> i32 { route!(…"#getFreeSlot()I", Recv::Player, ()); self.free_slot_body() }
  ```
  - `add_and_pick_item_body` follows vanilla's order exactly:
    1. `self.set_selected_slot(suitable)`: the gated port, because vanilla calls the method;
    2. if the selected stack isn't empty, `let free = self.free_slot();` (gated), and move it there if not -1;
    3. set the selected slot to the stack.
  - `pick_slot_body` sets `self.selected` directly, as vanilla assigns the field.
- **`guest_pick_item`** becomes vanilla's `tryPickItem` order inside `as_guest`:
  - `find_slot_matching_item`, then hotbar → `g.set_selected_slot(slot)`;
  - main inventory → `g.pick_slot(slot)`;
  - none and infinite materials → `g.add_and_pick_item(&stack)`.

  The held-slot packet stays as it is.
- **`getFreeSlot` in `Inventory.add`:**
  - `game.rs` `give`'s empty-slot loop and `native_server.rs` `add_to` (if it runs with `&mut Game`; check its callers) take their slot from `self.free_slot()`, once per placement as vanilla's `addResource` asks. Don't change their order otherwise. The inventory harnesses must stay identical.
  - `menu.rs` `inventory_add` and `cauldron.rs` are lib code without `Game`. Don't touch them. Add one `deferred.tsv` row each: `…#getFreeSlot()I \t seam: crates/horizon-client/src/menu.rs:1830` and the cauldron one. SPEC-4 allows these partial-seam rows.
- `game.rs` ~14226 and ~14237 call `pick` too. If they're the deprecated in-frame path (CLAUDE.md "The two single-player paths"), leave them on `pick` and say so in the report. If they're live, they go through the same ports.

## 2. New `RouteArg` / argument reader: `ItemStack`
- **Rust:** `impl RouteArg for &ItemStack`, written as `bytes tag`, the same encoding `java_use_request` uses for inventory stacks. Find that function and call it.
- **Java:** `Routing`'s argument reader handles `Lnet/minecraft/world/item/ItemStack;` with the same decoder `GameBridge.fill` uses for slots.
- Extend `RoutingArgsCheck` with one stack.

## 3. Body bridges (`G/java_bridges/player.rs`, `route` entries, each ending with `game.java_refresh_player()`)
- `Inventory.pickSlot "(I)V"` → `pick_slot_body`.
- `Inventory.addAndPickItem "(Lnet/minecraft/world/item/ItemStack;)V"` → `add_and_pick_item_body`, with the stack argument through `Flat`. If no `Arg` for `ItemStack` exists in `H/bridge.rs`, add it as `Bytes` of the tag, mirroring §2.
- No bridge for `getFreeSlot`: it is a body route in this pack, so the class processor would skip it anyway (SPEC-2). It's a pure read, so nothing needs writing through.

## 4. Test-mod coverage of a body route
- Add `InventoryFreeSlotMixin` to the test mod: `@WrapOperation` on the `NonNullList.get` call inside `Inventory.getFreeSlot`. While the player holds a tag `horizon_route_skip0` (`player.getTags()`; check the player view carries tags, and if it doesn't, use the selected slot being 8 as the trigger), it reports slot 0 as full: it returns a dummy non-empty stack for index 0. Otherwise it calls the original.
- `ChecksRouting`:
  1. in the session, empty the inventory, then `give` one stack with slot 0 empty and the trigger on;
  2. `Checks.check("route_body_free_slot", …)`: the stack landed in slot 1, not 0.

  Drive it with `/tag @s add horizon_route_skip0` and `/give @s minecraft:stone 1` in the `bridges` script, or the selection trigger.
- Add `Inventory#getFreeSlot()I` to `tools/java_routing/test_routes.tsv`.

## 5. Lint and data
- Remove the T1 keys from `deferred.tsv`, except the partial-seam rows of §1 and `readAdditionalSaveData`.
- `python3 tools/java_routing.py gates` regenerates `route_gates.tsv`. `lint` and `lint --ledger` must pass. Report what `--ledger` flags for `pick_item.rs` and `player_inventory.rs`, and resolve each:
  - route the caller through the port;
  - or, for a harness-only port, add a `tools/port_map` row naming `game/inventory.rs` as the port.

## Tests
- `cargo test -p horizon-client --lib pick_item`: the split keeps the current tests passing. Add one per new fn with vanilla's cases:
  - the suitable slot is empty;
  - the suitable slot is full, so the item moves to a free slot;
  - there's no free slot, so the item is replaced.
- `cargo test -p horizon-client --features java-mods --bins java_routing inventory`; clippy with and without the feature.
- `python3 tools/parity.py --changed-files <your files> --list`: give the list to the parent. Don't run broad sets.
- One `bridges` run: SPEC-3's three checks plus `route_body_free_slot`.

## How the parent tests it
1. `bridges` shows the four `route_*` checks ok.
2. Boot the showcase pack (SPEC-2's install). The boot log shows `route …Inventory#pickSlot(I)V edge on gated (carryon)`, the same for `addAndPickItem`, `setSelectedSlot`, and `getFreeSlot … body on gated (partial: 2 sites)`.
3. **User test (window, showcase pack):**
   - Survival, empty hand. Shift-right-click a chest to pick it up with Carry On. If pickup itself doesn't work, that's outside W12: report it to the parent.
   - While carrying:
     - press number keys 1–9. Another player on the same world (second client) sees the carrier's held item unchanged, and the carried chest stays carried;
     - middle-click a block whose item is in the main inventory: nothing moves into the hotbar.
   - Put the chest down. Number keys and middle-click work normally again.
   - The client-side slot highlight may still move while carrying (T1b, Carry On's client patch, isn't routed yet). Note it; it isn't a failure of this spec.

## Not in this spec
`Player#readAdditionalSaveData` (T2), the client's `Minecraft#handleKeybinds` (T1b), riding (T4), `ServerGamePacketListenerImpl#tryPickItem` and `handlePickItemFromBlock` (travelersbackpack and architectury: family `connection`), and the lib `Inventory.add` ports (the next seam spec unifies `Inventory.add` into one `Game` port).
