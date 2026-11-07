# SPEC-17: menus and hoppers (Refined Storage, AE2, Travelers Backpack, Lootr)

Design: `DESIGN.md` §2.6 row 17, question Q1. Needs SPEC-4 (guards), SPEC-5 (lib gates, for the click port in `menu.rs`), SPEC-16 (menu views). Size: ~400 changed lines.

Paths as SPEC-2; `L/` = `crates/horizon-client/src/`. Read first: vanilla `AbstractContainerMenu.doClick`, `clicked`, `getRedstoneSignalFromContainer`, `createCarriedSlotAccess`; `AnvilMenu.createResult` (NeoForge names its patched part `createResultInternal`: javap `-p` the patched `AnvilMenu` in `minecraft-server-patched-26.3.0.101.jar` for its signature); `ItemStack.overrideOtherStackedOnMe`; `HopperBlockEntity.getContainerAt`, `getBlockContainer`, `getEntityContainer` and NeoForge's `getContainerOrHandlerAt` (javap the patched class); `L/menu.rs` (the click port), `G/../game.rs` `menu_ops`, `click_inventory`, `G/stations.rs` (anvil), `G/machines.rs` (~153 hopper block container, ~347 comparator signal, ~867 entity container), `crates/horizon-items/src/item_data.rs` (`override_other_stacked_on_me`).

Comments in code: one line at most.

## Keys
| Key | Kind | Mod | Seam | Gate |
|---|---|---|---|---|
| `AbstractContainerMenu#doClick(IILnet/minecraft/world/inventory/ContainerInput;Lnet/minecraft/world/entity/player/Player;)V` | edge HEAD | refinedstorage | the `Game` fn that calls `menu.rs`'s click for a native menu (server side; guests through `as_guest`): `menu_do_click(slot, button, input)` | `route!`, `Recv::Player` → `containerMenu` (the inventory menu included) |
| `AnvilMenu#createResultInternal(…)` | body (expression) | ae2 | the anvil result recompute in `G/stations.rs`: `anvil_create_result()` | `route!`, `Recv::Player` → `containerMenu` |
| `ItemStack#overrideOtherStackedOnMe(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/inventory/Slot;Lnet/minecraft/world/inventory/ClickAction;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/entity/SlotAccess;)Z` | inject (SPEC-1) | travelersbackpack | inside `menu.rs`'s click, where vanilla calls it on the carried stack | `lib_route!`, `item =` the carried stack (SPEC-4: `recv_item` = backpacks) |
| `AbstractContainerMenu#getRedstoneSignalFromContainer(Lnet/minecraft/world/Container;)I` | edge HEAD, static | lootr | comparator output of a container in `G/machines.rs` ~347: `container_redstone_signal(pos) -> i32` | `route!`, `Static`; `ContainerArg(pos)`; `block =` the state at pos |
| `HopperBlockEntity#getBlockContainer(…Level;…BlockPos;…BlockState;)…Container;` | edge (`wrap_method`), static | lootr | `G/machines.rs` ~153: `hopper_block_container(pos) -> ContainerRef` | `route!`, `block =` the state at pos |
| `HopperBlockEntity#getEntityContainer(…Level;DDD)…Container;` | edge (`wrap_method`), static | lootr | `G/machines.rs` ~867: `hopper_entity_container(at) -> ContainerRef` | `route!`, no narrowing expected |
| `HopperBlockEntity#getContainerOrHandlerAt(…)` | edge (`wrap_method`) | lootr | where native's hopper asks for the container or item handler at a position (with java-mods, the path that reaches mods' item handlers) | `route!`, `block =` |
| `SimpleContainer#setChanged()V` | inject | puzzleslib, refinedstorage, shulkerboxtooltip, sophisticatedbackpacks, toms_storage | — | only if SPEC-4 doesn't prove it inert: then stop and raise DESIGN Q1 with SPEC-4's guard report |

## 1. New values
- **Receiver `ItemStack` projected to the carried stack.** For `overrideOtherStackedOnMe` the receiver is the carried stack: owner `ItemStack` with `Recv::Player` (or `LibRecv::Player`) → `player.containerMenu.getCarried()`. Add the row to SPEC-3's projection table.
- `SlotArg(i)`: `int` menu slot index → `menu.getSlot(i)`; `SlotStackArg(i)` → `menu.getSlot(i).getItem()` (the live object, so a handler's in-place change is the slot's); `ClickAction` as its ordinal; `SlotAccess` → `createCarriedSlotAccess()` of the menu (an accessor).
- `ContainerArg(pos)`: `long pos` → `BlockEntities.get(level, pos)` cast to `Container`.
- `ContainerRef` return: `byte kind` (0 null, 1 block container at the given pos, 2 entity id, 3 a mod item handler at pos), Java derives it from the returned object (a block-entity view → its pos; an entity view → its id; NeoForge's `ContainerOrHandler` with a handler → 3); any other object throws naming the route. Rust maps each to native's hopper target type.
- After `doClick`'s native body, SPEC-16's menu group and SPEC-2's player block are refreshed before RETURN handlers; the reply brings back what Java's handlers changed (carried, slots), which Rust applies through native's click paths.

## 2. Bodies
- `doClick` (edge): native's click is the body; nothing else.
- `createResultInternal` (body): vanilla's anvil logic runs on the menu view (its inputs come from SPEC-16's group); the result slot and the cost data slot it writes come back in the menu block; native's anvil takes them as its result and cost (replacing its own computation for that update). Its closure from `check --analysis` gets bridges; enchantment registry reads are Java's own (data).
- Hopper `wrap_method`s (edge): the body is native's lookup; a handler that returns something else replaces it.

## 3. Tests
- Test mod mixins (`J/testmod/…/mixin/MenuRouteMixin.java`):
  - `doClick` HEAD, cancellable: for clicks on slot index 0 of a `ChestMenu` with button 1 (right click), cancel;
  - `AnvilMenu#createResultInternal`: `@ModifyExpressionValue` on the same expression AE2 targets is not knowable; instead `@Inject(at = @At("TAIL"))` setting the cost data slot to 1 for any repair (makes the body route observable);
  - `overrideOtherStackedOnMe` HEAD: carried `TestBagItem` (a test-mod item class) swallows the slot's stack (sets it empty, returns true);
  - `getRedstoneSignalFromContainer` HEAD: 15 for containers tagged by being a `TestChestBlockEntity` (test-mod BE);
  - `getBlockContainer` `@WrapMethod`: returns null for `TestChestBlock` positions.
- `ChecksRouting` (session drives native clicks through horizon-ctl):
  - `right_click_slot0_cancelled`: a stone stack in a chest's slot 0 stays after a right click; a left click picks it up;
  - `anvil_cost_from_route`: renaming-free repair of two damaged iron swords shows cost 1 (native sent the cost the route wrote);
  - `bag_swallows`: carrying a `TestBagItem` and clicking on a dirt stack empties that slot;
  - `comparator_15`: a comparator reading an empty `TestChestBlock` outputs 15;
  - `hopper_skips_test_chest`: a hopper under a `TestChestBlock` with items pulls nothing in 100 ticks; under a vanilla chest it pulls.
- `lan.sh chest` and the inventory and hopper harnesses `parity.py --changed-files … --list` names: unchanged without mods.
- `cargo test` of the touched modules; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run.
- Cost: a 20-hopper chain under a chest for 1200 ticks with `HORIZON_PROFILE`: report `route:` calls and means for the hopper keys (`getEntityContainer` has no narrowing: every hopper's pull attempt crosses).

## Done bar
Checks pass; gates for the seven keys; `SimpleContainer#setChanged` inert by proof or raised as Q1; lint passes.

## For the user
- Refined Storage: open a vanilla chest and a crafting table with an RS grid nearby; shift-click, drag and middle-click stacks; nothing behaves differently from NeoForge (RS's click hook is for its own features; compare any RS-specific click behaviour you know).
- Travelers Backpack: carry the backpack item on the cursor and click it onto a stack in a chest: the stack goes into the backpack, as on NeoForge.
- AE2: repair two AE2 tools in an anvil: the cost and result match NeoForge's.
- Lootr: a hopper under a Lootr chest doesn't drain it; a comparator next to it reads as on NeoForge.
