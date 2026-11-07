# SPEC-12 (W13): `UseItemOnBlockEvent` on native clicks

Design: `DESIGN.md` §1.3 (architectury). Read first: SPEC-1, `G/game.rs` `use_hand` (from its start to the place
branch: the comments name each vanilla step: `native_right_click_block`, "MultiPlayerGameMode.useItemOn: blocks react
first", `use_item_on_branch`, "Native mods, where ItemStack.useOn and Item.use start", `java_use_item_on`), `G/java_mods.rs`
(`java_use_item_on`, `java_click_block`: which clicks Java runs whole), `docs/java-mods.md` row "`Item.use`,
`Item.useOn`, clicks on mods' blocks", the patched bytecode of `ServerPlayerGameMode.useItemOn`, `ItemStack.onItemUseFirst`,
`ItemStack.useOn`, `BlockBehaviour$BlockStateBase.useItemOn` (`javap/callers.javap`).

Depends on SPEC-1. About 250 changed lines. Code comments: one line at most, only for a non-obvious why.

## Vanilla + NeoForge
`ServerPlayerGameMode.useItemOn`: `RightClickBlock` (Horizon already posts it) → unless its `useItem` is `DENY`:
`stack.onItemUseFirst(context)`, whose first step posts `UseItemOnBlockEvent(context, ITEM_BEFORE_BLOCK)`; canceled →
returns `getCancellationResult()`; a result other than `PASS` ends the click with it. Then, when the block may react
(not a sneak that the items don't bypass, `useBlock` not `DENY`): `BlockState.useItemOn`, whose first step posts the
event with `BLOCK`; canceled → that result is the block's result (`TRY_WITH_EMPTY_HAND` → `useWithoutItem`, a consuming
result ends the click). Then `ItemStack.useOn`, whose first step posts it with `ITEM_AFTER_BLOCK`; canceled → that result
is `useOn`'s.

## Who posts what
Native posts a phase only where native runs that vanilla method. A click Java runs whole (`click_block`, the real
`ServerPlayerGameMode.useItemOn` for a mod's block) or an `ItemStack.useOn` Java runs (`use_item_on` for a mod item with
`useOn`/`onItemUseFirst` code, `ItemCode`) posts its phases in Java already: native must not post those again.

## Wire (`player_event` id 23)
Tail: `byte phase` (0 `ITEM_BEFORE_BLOCK`, 1 `BLOCK`, 2 `ITEM_AFTER_BLOCK`), `int hand`, `long pos`, `byte face` (3D
value), 3 `double` hit location, `bool inside`. Result: `byte kind` (0 `PASS`, 1 `SUCCESS`, 2 `CONSUME`, 3 `FAIL`, 4
`TRY_WITH_EMPTY_HAND`), `byte swing` (`InteractionResult.SwingSource`: 0 `NONE`, 1 `CLIENT`, 2 `SERVER`; 0 unless
`SUCCESS`). Only read when `canceled`.

## Java (`ServerEvents` case 23)
`var context = new UseOnContext(call.player, hand, new BlockHitResult(location, face, pos, inside))`;
`var event = NeoForge.EVENT_BUS.post(new UseItemOnBlockEvent(context, phase))`; write `canceled`, then the result kind
from `event.getCancellationResult()` (`instanceof InteractionResult.Success s` → `SUCCESS` with `s.swingSource()`, or
`CONSUME` when it is `InteractionResult.CONSUME`'s value; compare with the constants, javap `InteractionResult` for the
exact names in 26.3). Debug line: `Horizon: server event UseItemOnBlockEvent <phase> canceled=<b> result=<kind>`.

## Rust (`G/game.rs` `use_hand`, `G/java_hooks.rs`)
- Pair: `java_hook_use_item_on_block(&mut self, phase: UsePhase, hand: Hand, hit: &Hit) -> Option<UseVerdict>`;
  `None` = not listened or not canceled. `UseVerdict { kind: UseKind, server_swing: bool }`.
- One helper `fn apply_use_verdict(&mut self, v: UseVerdict, phase: UsePhase) -> Option<Result<bool>>`: `Some(...)` ends
  `use_hand` with that return, `None` continues to the next step. Vanilla's rules per phase (`ServerPlayerGameMode.useItemOn`):
  - `ITEM_BEFORE_BLOCK`: anything but `Pass` ends the click with that result (`if (result != PASS) return result`):
    `Success` → swing when `server_swing`, `Some(Ok(true))`; `Consume`, `Fail`, `TryWithEmptyHand` → `Some(Ok(true))`
    with nothing done. `Pass` → `None`.
  - `BLOCK`: `Success`/`Consume` (`consumesAction()`) → the end the block path has today when the block handled the
    click (the `ITEM_USED_ON_BLOCK` trigger with the stack before the use, the swing for `server_swing`);
    `TryWithEmptyHand` on the main hand → the empty-hand use (`useWithoutItem`) exactly as when the block's own
    `useItemOn` answers it; `Pass`, `Fail`, or `TryWithEmptyHand` on the off hand → `None`: go on to the item's `useOn`
    (vanilla skips only the block's own code).
  - `ITEM_AFTER_BLOCK`: the verdict is `useOn`'s result: `Success`/`Consume` → the end native has today for a consuming
    `useOn` (swing for `server_swing`, the `used` statistic as `ItemStack.useOn` awards it); `Fail` → `Some(Ok(true))`;
    `Pass`/`TryWithEmptyHand` → the path native takes today when `useOn` passes.
- Sites, each only when `java_listens(UseItemOnBlock)` and only for the hand being processed:
  1. `ITEM_BEFORE_BLOCK`: right after `native_right_click_block` let the click go on (and after the host's
     `RightClickBlock` post), when the held stack isn't empty and the click isn't one Java runs whole.
  2. `BLOCK`: immediately before the block's `useItemOn` branch (`use_item_on_branch`), only on the path where native
     runs it (a mod's block goes to Java's `click_block`, which posts the phase itself).
  3. `ITEM_AFTER_BLOCK`: immediately before `native_use_item_on`, unless the item's `useOn` is Java's
     (`Host::item_code_runs` for `useOn`, the test `java_use_item_on` already makes).
  If `use_hand`'s flow makes a site hard to place exactly, split the phase into a private fn first (no behaviour
  change), then add the hook at its top.
- `JavaEvent::UseItemOnBlock` into `SITED`.

## Tests
- Test mod (`ChecksServerEvents`): a listener cancelling `BLOCK` phases on `minecraft:lever` with `InteractionResult.FAIL`
  and `ITEM_AFTER_BLOCK` phases for `minecraft:flint_and_steel` with `SUCCESS`; it records every phase it sees, in order.
  `drive`: place a lever and right-click it holding a stick, then right-click stone holding flint and steel. Checks:
  - `use_block_cancel_keeps_lever_off`: the lever is still `powered=false` (its `useItemOn`/`useWithoutItem` never ran);
  - `use_item_cancel_lights_nothing`: no fire above the stone, and the flint and steel's damage unchanged;
  - `use_phases_in_order`: for the stick on the lever, `[ITEM_BEFORE_BLOCK, BLOCK, ITEM_AFTER_BLOCK]` (a `FAIL` from
    `BLOCK` doesn't consume the action, so vanilla goes on to `useOn`); for flint and steel on stone, the same three.
- Rust: `apply_use_verdict` table test (pure part factored out: verdict → end/continue/empty-hand), each of the five kinds
  for each phase.

## Done bar
- `bridges` CHECKs pass; `cargo test -p horizon-client --features java-mods --lib use_verdict`, clippy both ways, fmt.
- The interaction harnesses `tools/parity.py --changed-files` lists for `game.rs`'s `use_hand` (at most two) unchanged.
- M1 `w13` steps `place`, `door`, `bonemeal`, `sleep`: `UseItemOnBlockEvent` count equal Forge's where listened.
- Append the debug-line check to `tools/scenarios/w13-events.json`.

## Live test (lead, showcase jars, `HORIZON_JAVA_DEBUG=1`)
1. Right-click a lever, a chest, a door with an item in hand, and place a block: each behaves as before; the log shows
   the phases (`ITEM_BEFORE_BLOCK`, `BLOCK`, and `ITEM_AFTER_BLOCK` when the block passed).
2. A Mekanism machine right-clicked with a configurator: works as before and logs no native phase line (Java ran it).
