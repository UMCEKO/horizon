# SPEC-15: player packets and pick block (Architectury, Travelers Backpack, Lootr)

Design: `DESIGN.md` §2.6 row 15. Needs SPEC-2 (route replies carry the player's stacks), SPEC-3 (`Recv::Player` projected to `player.connection`, new arguments). Size: ~350 changed lines.

Paths as SPEC-2. Read first: vanilla `ServerGamePacketListenerImpl.java` (`handlePickItemFromBlock`, `handlePickItemFromEntity`, `tryPickItem`, `handleUseItemOn`); `G/lan_actions.rs` the `S::PickItemFromBlock` (~2965), `S::PickItemFromEntity` (~2986) and `S::UseItemOn` (~2367) arms; `G/pick_entity.rs`; `G/inventory.rs` (W12 SPEC-5's `pick_slot`, `add_and_pick_item`, `free_slot`, already gated); `G/java_mods.rs` (`java_use_request`, the item-click paths: which `UseItemOn`s already go to Java's `gameMode.useItemOn`).

Comments in code: one line at most.

## Keys
| Key | Kind | Mod | Seam |
|---|---|---|---|
| `ServerGamePacketListenerImpl#tryPickItem(Lnet/minecraft/world/item/ItemStack;)V` | edge HEAD | travelersbackpack | `try_pick_item(stack)`: the shared tail of both pick arms (vanilla: both handlers end in `tryPickItem`) |
| `ServerGamePacketListenerImpl#handlePickItemFromBlock(Lnet/minecraft/network/protocol/game/ServerboundPickItemFromBlockPacket;)V` | body (wrap `BlockState.getCloneItemStack`) | architectury | `handle_pick_item_from_block(pos, include_data)`: the whole arm |
| `ServerGamePacketListenerImpl#handlePickItemFromEntity(…ServerboundPickItemFromEntityPacket;)V` | body (wrap `Entity.getPickResult`) | architectury | `handle_pick_item_from_entity(id, include_data)` |
| `ServerGamePacketListenerImpl#handleUseItemOn(…ServerboundUseItemOnPacket;)V` | body (wrap `ServerLevel.mayInteract`) | lootr | `handle_use_item_on(packet fields)`: the arm |

All four are `Recv::Player` (projected to `player.connection`, SPEC-3 §4), run through `as_guest` for guests as the arms already do.

## 1. Packet arguments
A `ServerboundPickItemFromBlockPacket`, `…FromEntityPacket` or `ServerboundUseItemOnPacket` crosses as its fields, and Java rebuilds the packet record with its public constructor (26.3 records: check `ServerboundUseItemOnPacket(InteractionHand, BlockHitResult, int sequence)`): `PacketArg` writers per type in `G/java_routing.rs`, readers in SPEC-3's table (`ServerboundUseItemOnPacket`: `int hand`, `double[] hit` for `Flat.toBlockHitResult`, `int sequence`; pick from block: `long pos`, `bool includeData`; from entity: `int id`, `bool includeData`).

## 2. Bodies in Java
- **Pick handlers.** Vanilla's bodies check reach and loaded state, build the stack (`getCloneItemStack` / `getPickResult`, wrapped by Architectury), add block-entity data for creative with `includeData`, then call `tryPickItem`. In Java, `tryPickItem` is a backed route stub (edge, since Travelers Backpack's change is HEAD): its native body is `try_pick_item_body` (`G/inventory.rs`'s pick logic: `find_slot_matching_item`, `pick_slot`, `add_and_pick_item`, already W12 SPEC-5's seams). Closure (from the 11-06 report): `isWithinBlockInteractionRange`, `hasInfiniteMaterials`, `isInValidBounds`, `BlockEntity#saveCustomOnly`, `collectComponents`, `getEntityOrPart`, `getPickResult` (for native entities: a bridge to native's pick result, `G/pick_entity.rs`), `canUseGameMasterBlocks`, `isWithinEntityInteractionRange`… Bridge each to native; `BlockState.getCloneItemStack` of a vanilla block runs in Java (pure, reads the level view) unless `check` lists a native-owned read.
- **`handleUseItemOn`.** Vanilla's body: validity and reach checks, `mayInteract` (wrapped by Lootr: spawn protection doesn't block Lootr chests), then `gameMode.useItemOn(…)`, then the block update acks and the `sendBlockUpdate`s. In Java:
  - `ServerPlayerGameMode#useItemOn` becomes a bridge to native's use-on path (the rest of today's `S::UseItemOn` arm after its checks: native block use, item use, placing, and the existing Java paths for mod items and mod blocks inside it). So vanilla blocks stay native; the Java body owns only the packet handler's own steps.
  - `ServerLevel#mayInteract` is a bridge to native's spawn-protection check (`lan_actions.rs` has it: lift it to a fn).
  - The acks and block updates the body sends through `player.connection.send(…)`: `Connection.send` from a Java server view reaches the Java client only (`ConnectionMixin`); these packets must reach the native player instead. Add bridges `ServerGamePacketListenerImpl#ackBlockChangesUpTo` → native's ack, and `ServerCommonPacketListenerImpl#send` of `ClientboundBlockUpdatePacket` → native's block update send, both guarded to backed (server-view) connections of native players. Other packets the body may send: list them from the source; each needs a native counterpart.
  - Closure from the report (30): `openMenu`, `sendBuildLimitMessage`, `sendOverlayMessage`, `sendSpawnProtectionMessage`, `setCamera`, `drop`, `broadcastSystemMessage`… bridge each to native (overlay and system messages through native chat; `openMenu` of a vanilla menu through native's open).
- Cost: right clicks are per input, not per tick: no cost bar beyond "no visible delay" (report `route:` means).

## 3. Tests
- Test mod mixins (`J/testmod/…/mixin/PacketRouteMixin.java`):
  - `tryPickItem` HEAD, cancellable: for `minecraft:diamond_block`, cancel (the pick does nothing);
  - `handlePickItemFromBlock` wrap of `getCloneItemStack`: for `minecraft:stone`, return `minecraft:cobblestone` ×1;
  - `handleUseItemOn` wrap of `mayInteract`: always `true` for blocks at `y == 200` (a test "protected" platform), recorded.
- `ChecksRouting` (session: creative; pick a stone block; pick a diamond block; survival with spawn protection on a dedicated test server is not available in `bridges`, so instead set the world spawn and `spawn-protection` in the session's server properties if the harness supports it, else drive `mayInteract` through a non-op player on a LAN guest in `lan.sh`):
  - `pick_stone_gives_cobble`: the hotbar holds cobblestone after picking stone (native inventory);
  - `pick_diamond_cancelled`: inventory unchanged;
  - `use_item_on_still_places`: placing a dirt block against a stone block works and the client got its ack (no ghost block: `horizon-ctl state` shows dirt);
  - `may_interact_wrapped`: the wrap recorded one call per use.
- `lan.sh` checks for block placing and picking (whatever `--check` names cover them), `python3 tools/parity.py --changed-files <files> --list`.
- `cargo test` of the touched modules; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run.

## Done bar
Checks pass; four gates; no unbridged callee in `check --analysis` for the three body routes; lint passes.

## For the user
- Creative, middle-click blocks and mobs: the right items come to the hotbar (with Ctrl, with their data). Travelers Backpack: middle-click a block whose item is in your worn backpack: it is pulled from the backpack, as on NeoForge.
- Lootr on a dedicated server with spawn protection: a non-op player can open a Lootr chest inside the protected spawn area but can't break blocks there, as on NeoForge.
