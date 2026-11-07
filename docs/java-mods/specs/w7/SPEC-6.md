# SPEC-6 (W7): buckets and mod fluids

Read first: `DESIGN.md` §1.1 G5, §2.2; `crates/horizon-client/src/game/java_mods.rs` (the use hook at line ~54 with `item_code_runs`, `java_call_using`), `game.rs` `fn use_item` and the bucket branch that reaches `horizon-world/src/level/buckets.rs` `bucket_pickup`; `GameBridge` op case `0` (`player.gameMode.useItem`); `ItemCode.java` (`ownCode`, `NATIVE`); vanilla `BucketItem.use`, `ItemUtils.createFilledResult`, `LiquidBlock.pickupBlock`, `Item.getPlayerPOVHitResult`.

Depends on: SPEC-5 (a placed fluid must tick). Comments in code: one line max, only a non-obvious why.

## Goal

- A mod's bucket places its fluid (already Java via `ItemCode`; after SPEC-4/5 the fluid ticks).
- An empty vanilla bucket aimed at a mod fluid source picks it up, through the real `BucketItem.use` in Java, as a block item placing a mod's block already runs in Java ("Item.use ... a block item placing a block a mod owns runs in Java too", `docs/java-mods.md`).

## Native

- In the server-side use hook (`java_mods.rs`, where `item_code_runs(&stack.item, hit.is_some())` gates the Java call for a use in the air), add a second admission: the stack is `minecraft:bucket` **and** `self.java_bucket_aims_mod_fluid()` is true. The use then goes to Java exactly as a mod item's `use` does (same op, same copy-back of stacks and experience).
- `fn java_bucket_aims_mod_fluid(&mut self) -> bool` (new, `java_mods.rs`): vanilla's `getPlayerPOVHitResult(level, player, ClipContext.Fluid.SOURCE_ONLY)` natively: from the acting player's eye along the look vector to `blockInteractionRange()`, a clip that stops at blocks' outline shapes and at fluid **sources** (find the clip native's own bucket uses; it is the same with `SOURCE_ONLY`). True when the hit block's fluid (`FluidState` from SPEC-4's registry) is a source of a non-vanilla `FluidKind`.
- Native's own empty-bucket path (`buckets.rs` `bucket_pickup`) is unchanged for water and lava, and never runs for this use (the hook returned first).
- The window (client) side needs nothing: vanilla's client runs `BucketItem.use` for the swing and its own prediction; native's window keeps its own behaviour, and the server's inventory packets bring the filled bucket.

## Java

Nothing new: `BucketItem.use` on the view, `LiquidBlock.pickupBlock` (bridged `setBlock` to air), `getPickupSound`/`FluidType` sounds, `ItemUtils.createFilledResult`, the stat and the advancement trigger (`CriterionMixin` keeps triggers inert on views: unchanged).
Check `ItemCode`'s admission of a mod's bucket class: a mod item that is a plain `BucketItem` must be in `use_in_air` (`item_code_runs`); if `BootFacts.ownsItem` gates it, a mod's bucket passes. Prove it with the testmod check below rather than by reading.

## Tests

- **Rust unit** (`java_mods.rs` tests or a gameplay test with a fixture world and the SPEC-4 test table): a player looking at a mod source two blocks ahead → `java_bucket_aims_mod_fluid` true; looking at a mod flowing cell → false; at water → false; at a stone block in front of a mod source → false. Geometry expected values worked out by hand in the test's comment (eye height 1.62, range 4.5).
- **Testmod `ChecksFluids`:**
  - `goo_bucket_admitted`: `ItemCode.use(Goo.BUCKET.get().getClass())` is true and the item is in the exported `use_in_air` set.
- **Scenario `tools/scenarios/w7_buckets.json`** (`play/install-rs` plus the test jar, or with Mekanism's brine): floor; survival; `give @s minecraft:bucket 16`; `/setblock 0 101 2 mekanism:brine`; `look-at-block 0 101 2`; `use-item`; `wait 5`; checks: `0 101 2` is air; inventory has 15 `minecraft:bucket` and 1 `mekanism:brine_bucket` (vanilla `createFilledResult` in survival). Then `select mekanism:brine_bucket`, `look-at-block 0 100 4 up`, `use-item`, `wait 5`: `0 101 4` is `mekanism:brine` `level=0`; the slot holds `minecraft:bucket`. Creative variant: the empty bucket stays and the filled one is added once (vanilla's `hasInfiniteMaterials` branch). `no_exceptions`.

## Done bar

- `cargo test -p horizon-client --features java-mods --lib java_`; clippy both ways; `java_mods.py check`.
- The scenario passes once (gate).

## Live test (lead)

1. Survival, a stack of empty buckets, a Mekanism brine source (`/setblock`): right-click it: the source disappears with Mekanism's fill sound; one brine bucket appears, the stack drops by one.
2. Right-click the floor with the brine bucket: a brine source appears and flows (SPEC-5), the bucket is empty again.
3. Same with water and lava: unchanged (native).
