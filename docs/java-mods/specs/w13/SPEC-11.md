# SPEC-11 (W13): `ItemTossEvent` and `ArrowLooseEvent`

Design: `DESIGN.md` §1.3 (architectury; puzzleslib). Read first: SPEC-1, `G/game.rs` (`drop_held`, `player_drop`,
`throw_from_eyes`, `spawn_item`, `release_bow`), `G/crossbow.rs` `shoot_crossbow`, `B/EntityViews.java` (`spawn`: how a
Java-made entity reaches native at once, W9 SPEC-1c), the patched bytecode of `Player.drop`,
`CommonHooks.onPlayerTossEvent` (`javap/nfhooks.javap`), `BowItem.releaseUsing`, `CrossbowItem.performShooting`.

Depends on SPEC-1 and W9 SPEC-1c (`EntityViews.spawn`, merged). About 250 changed lines.
Code comments: one line at most, only for a non-obvious why.

## Vanilla + NeoForge
- `Player.drop(stack, includeThrowerName)` → `CommonHooks.onPlayerTossEvent(player, stack, includeName, prediction)`:
  the `ItemEntity` is built (vanilla's `createItemStackToDrop`: position at the eyes minus 0.3, motion from the
  player's random), then `ItemTossEvent(itemEntity, player)` is posted; canceled → null (no entity, the stack is gone:
  the caller already took it out of the inventory); else `level.addFreshEntity(event.getEntity())`.
- `BowItem.releaseUsing`: `charge = getUseDuration - timeLeft`; `charge = EventHooks.onArrowLoose(stack, level, player,
  charge, hasAmmo)`; `charge < 0` → return false (no shot, no ammo used); else the power from `charge`.
- `CrossbowItem.performShooting` (a player shooter): `EventHooks.onArrowLoose(stack, level, player, 1)` first; `< 0`
  → return (nothing shot, the charged projectiles stay).

## Wire (`player_event`)
- id 21 `ItemToss`: `bytes stack` (tag), 3 `double` position, 3 `double` motion, `int pickupDelay`, `bool
  throwerNamed`. Result `int entityId` (-1 when canceled).
- id 22 `ArrowLoose`: `int hand`, `int charge`, `bool hasAmmo`, `bool crossbow`. Result `int charge` (negative =
  canceled).

## Java (`ServerEvents`)
- case 21: `var item = new ItemEntity(level, x, y, z, stack, vx, vy, vz)`; `item.setPickUpDelay(delay)`; if named,
  `item.setThrower(call.player)`. Post `new ItemTossEvent(item, call.player)`; canceled → result -1. Else
  `level.addFreshEntity(item)` (bridged: `EntityViews.spawn` adds the native entity from this object at once and
  returns its id; the event's entity, with whatever a listener changed on it, is what enters the level, as vanilla's
  `addFreshEntity(event.getEntity())`); result the entity's id (`item.getId()` after the spawn). If `EntityViews.spawn`
  doesn't carry an item's pickup delay and thrower yet, add them to the item fields it reads.
- case 22: `crossbow` → `EventHooks.onArrowLoose(stack, level, call.player, 1)`; else `EventHooks.onArrowLoose(stack,
  level, call.player, charge, hasAmmo)` where `stack = call.player.getItemInHand(hand)`. Result the int.
- Debug lines: `Horizon: server event ItemTossEvent canceled=<b> id=<id>`, `ArrowLooseEvent <in>-><out>`.

## Rust
- `java_hooks.rs` pairs:
  - `java_hook_item_toss(&mut self, stack: &ItemStack, at: [f64; 3], motion: Vec3, delay: i32, named: bool) ->
    Option<Option<u32>>`: `None` when not listened (native spawns as today); `Some(None)` canceled; `Some(Some(id))`
    Java spawned it.
  - `java_hook_arrow_loose(&mut self, hand: Hand, charge: i32, has_ammo: bool, crossbow: bool) -> i32` (returns
    `charge` unchanged when not listened).
- `throw_from_eyes`: after the motion and `y` are computed (the random draws stay native's, in vanilla's order) and
  only when `thrown_from_hand` or not (both are `Player.drop`; pass `named = thrown_from_hand`): call the toss hook
  first. `Some(None)` → return `None` (the stack is gone, as vanilla). `Some(Some(id))` → return `Some(id)` (the thrower
  was set by Java on the entity; don't set it again). `None` → today's `spawn_item` path.
- `release_bow`: after `native_arrow_loose`, `if !canceled { charge = self.java_hook_arrow_loose(Hand::Main or the hand
  in use, charge, ammo.is_some(), false); canceled = charge < 0; }` before `bow_release_power`.
- `shoot_crossbow`: first, for the player: `if self.java_hook_arrow_loose(hand, 1, true, true) < 0 { return }` before
  anything is cleared or shot.
- `JavaEvent::ItemToss`, `ArrowLoose` into `SITED`.

## Tests
- Test mod (`ChecksServerEvents`): `ItemTossEvent` listener cancels when the stack is `minecraft:diamond`, and sets
  `setPickUpDelay(0)` on every other tossed item; `ArrowLooseEvent` listener sets the charge to 20 (full) for bows.
  `drive`: give a diamond and a stick, select each and press the drop key (horizon-ctl `key q tap`), give a bow and an
  arrow and draw it for 3 ticks (`mouse right down`, `wait 3`, `mouse right up`). Checks:
  - `toss_cancel_removes_diamond`: no diamond item entity near the player and none in the inventory (vanilla: the
    stack is gone);
  - `toss_sees_entity_and_changes_reach_level`: the stick's item entity exists and its pickup delay is 0 (independent:
    vanilla's drop key gives 40);
  - `arrow_loose_full_power`: the shot arrow's speed is 3.0 ± 0.05 blocks/tick (power 1.0 * 3; a 3-tick draw alone
    gives `BowItem.getPowerForTime(3)` = (0.15² + 2 * 0.15) / 3 ≈ 0.11, a speed near 0.32).
- Rust: none beyond the hook plumbing (covered by SPEC-1's reply parser); the existing `crossbow.rs` tests stay green.

## Done bar
- `bridges` CHECKs pass; `cargo test -p horizon-client --features java-mods --lib crossbow`, clippy both ways, fmt.
- M1 `w13` step `toss`: `ItemTossEvent` equal Forge's where listened (architectury: yes in the showcase).
- Append the two debug-line checks to `tools/scenarios/w13-events.json`.

## Live test (lead, showcase jars, `HORIZON_JAVA_DEBUG=1`)
1. Press Q on a held stack and Ctrl+Q: the item flies as before and can be picked up after the usual delay; one
   `ItemTossEvent canceled=false` line each.
2. Shoot a bow at full draw and a crossbow: the arrows fly as before; one `ArrowLooseEvent` line each.
3. Throw items out of the inventory screen (dragging out): one toss each, no duplicated item entity.
