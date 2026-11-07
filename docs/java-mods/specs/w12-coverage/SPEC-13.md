# SPEC-13: use paths (GlitchCore, Architectury, Balm)

Design: `DESIGN.md` §2.6 row 13. Needs SPEC-2 (route replies carry the player's stacks), SPEC-3 (receivers, `InteractionResult`), SPEC-4 (item and block admission). Size: ~400 changed lines.

Paths as SPEC-2; `L/` = `crates/horizon-client/src/`. Read first: vanilla `ItemStack.use`, `ItemStack.finishUsingItem`, `BucketItem.use`, `BlockTransformer` (26.3, `net/minecraft/core/component/BlockTransformer.java`), `BlockBehaviour.BlockStateBase.useWithoutItem`, `ServerPlayerGameMode.useItem`/`useItemOn`, `ServerGamePacketListenerImpl.handleUseItem`; `G/java_mods.rs` and `docs/java-mods.md`'s `Item.use` row (`ItemCode`: which uses already run in Java); `G/remote.rs` ~6309 (server use-item handling), `G/../game.rs` ~11129 (buckets), `tick_used_item`, `G/remote.rs` `remote_complete_using`, `L/block_transform.rs` ~226, `crates/horizon-world/src/level/interact.rs`, `G/hand.rs`.

Comments in code: one line at most.

## Keys
| Key | Kind | Mods | Seam |
|---|---|---|---|
| `ItemStack#use(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/InteractionHand;)Lnet/minecraft/world/InteractionResult;` | body (HEAD inject; wrap `Item.use`) | glitchcore, architectury | native's use-in-air branch for items **not** in `ItemCode` (those already run `gameMode.useItem` in Java, which calls the merged `ItemStack.use`): `item_stack_use(hand) -> UseResult` |
| `ItemStack#finishUsingItem(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/LivingEntity;)Lnet/minecraft/world/item/ItemStack;` | inject (SPEC-1) | glitchcore | `tick_used_item`'s completion and `remote_complete_using`, native branch only: `item_stack_finish_using(hand) -> Option<ItemStack>` |
| `BucketItem#use(…)Lnet/minecraft/world/InteractionResult;` | body (inject at `BlockHitResult.getType`) | architectury | the bucket branch of native's use (`game.rs` ~11129): `bucket_use(hand) -> UseResult`; admitted items: the bucket items (SPEC-4's item family list) |
| `BlockTransformer#transformBlock(Lnet/minecraft/world/item/context/UseOnContext;)…` and `(…UseOnContext;Lnet/minecraft/core/Holder;)…` | body (expression; wrap `getOptionalState`) | glitchcore, architectury | the `Game` method that calls `block_transform.rs`'s transform for stripping, flattening, tilling, waxing: `block_transform(ctx) -> UseResult` |
| `BlockBehaviour$BlockStateBase#useWithoutItem(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/phys/BlockHitResult;)Lnet/minecraft/world/InteractionResult;` | edge HEAD | architectury | the `Game` step of native's `useItemOn` that calls `level/interact.rs` for an empty-hand (or pass-through) block use: `block_use_without_item(pos, hit) -> UseResult`, `Recv::Block(state)` |
| `BlockBehaviour$BlockStateBase#getDestroyProgress(…)F` | edge RETURN | balm | already served: with java-mods every mining progress is Java's real `getDestroyProgress` (`docs/java-mods.md`, Mining speed). Prove it (§3) and record verdict `java-served` |

## 1. New values
- `UseOnArg` (SPEC-3 table row, both sides): `PlayerArg` fill, `int hand`, `double[] hit` as `Flat.toBlockHitResult` reads it (`Flat.java:312`); Java builds `new UseOnContext(player, hand, hit)`.
- `Recv::Block(BlockStateId)` in `Game` code (byte 6, SPEC-5's Java side): receiver = the Java state.

## 2. Bodies in Java and what they call back
Running `ItemStack.use` in Java means vanilla's body calls `Item.use` of the stack's item. For a vanilla item native ports, that must be native's port, not Java's copy of vanilla:
- New bridge `"net/minecraft/world/item/Item".use (…)` with a guard admitting only vanilla item classes native ports (`@Routing.nativeItem`: the item isn't in `BootFacts`' owned set and its class is a vanilla class), calling native's use for that item on the acting player (the body of today's native use-in-air path). Mod items keep their Java `use`.
- Likewise `"net/minecraft/world/item/Item".finishUsingItem` for vanilla items → native's completion.
- `BucketItem.use` in Java: its closure (29 in the 11-06 report: `sendParticles`, eye position and view vector, `getItemBySlot`, `ServerPlayer#level`…) bridged; the fluid pick-up and placement go through level bridges (`setBlock`, `getFluidState`), and `ItemUtils.createFilledResult`'s stack swap returns through SPEC-2's player block in the route reply.
- `transformBlock`'s closure: `Inventory#add`, `giveExperiencePoints`, `drop`, equipment reads: bridged or covered by the player block.
- Every reply that changed stacks or experience: SPEC-2's player block. No other copy-back.

## 3. `getDestroyProgress` proof
List every native caller of block break progress (`grep -rn destroy_progress` under `crates/horizon-client/src/`, plus `first_person.rs` ~612). With `java-mods` on, each must go through `Game::destroy_progress` (server) or `ClientBridge.destroyProgress` (window). If one doesn't (the analysis named `first_person.rs:612`), route it through the window's existing Java path; then add the `verdicts.tsv` row `java-served`, evidence = the list of call sites. Add `java-served` to SPEC-1's verdict vocabulary.

## 4. Tests
- Test mod mixins (`J/testmod/…/mixin/UseRouteMixin.java`):
  - `ItemStack#use` HEAD: for `minecraft:stick`, `cir.setReturnValue(InteractionResult.SUCCESS)` and give the player one `minecraft:apple` (through `player.getInventory().add`);
  - `@WrapOperation` on `Item.use` inside `ItemStack#use`: counts calls per item;
  - `BucketItem#use` inject at `getType`: records the hit type;
  - `useWithoutItem` HEAD: for `minecraft:crafting_table` while the player is sneaking-free and holding nothing, cancel with `SUCCESS` (no crafting screen) when the player is tagged `horizon_block`;
  - `BlockTransformer#transformBlock` wrap of `getOptionalState`: count.
- `ChecksRouting` (session drives: right-click air with a stick; with an empty bucket at water; with an ender pearl; an axe on an oak log; empty hand on a crafting table while tagged):
  - `stick_use_gives_apple`: one apple appears in native's inventory (`/data get entity @s Inventory` through the native command) — the player block of the route reply;
  - `pearl_use_still_native`: the pearl is thrown once (one `minecraft:ender_pearl` entity), `Item.use` wrap counted 1 for it;
  - `bucket_fills`: the bucket becomes a water bucket and the water source is gone (vanilla), the hit type recorded `BLOCK`;
  - `axe_strips_once`: the log is stripped, transform count 1;
  - `table_use_cancelled`: no crafting screen opened (native's `OpenScreen` not sent; the check reads the session's packet log or the window's screen state through horizon-ctl `state`).
- `python3 tools/parity.py --changed-files <files> --list` and what it names (`use_item`, `bucket`, `block_transform` harnesses if present): without mods nothing moves.
- `cargo test` of the touched modules; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run.

## Done bar
Checks pass; five gates plus the bridge rows; the `getDestroyProgress` verdict recorded with its call-site list; lint passes.

## For the user
With the showcase install (GlitchCore is Biomes O' Plenty's library; Architectury is the library of several pack mods): right-click with a bucket on water and lava, strip a log with an axe, till dirt with a hoe, eat food, throw an ender pearl, open a crafting table and a chest. Everything behaves as without mods, and as on NeoForge. Biomes O' Plenty's items with use behaviour work the same.
