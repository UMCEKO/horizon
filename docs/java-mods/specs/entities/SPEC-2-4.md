# SPEC-2-4: fishing hooks and leash knots become level entities

Index: `SPEC-2-8.md` (E2). Size: ~250 lines. Same kind pattern as `SPEC-2-2.md`. Independent of 2-2 and 2-3 (merge one at a time).

## Why
- **Hooks.** A cast hook lives in its owner's state: `Game.hook: Option<fishing::Hook>` for the player in place, `GuestState.hook` (`lan_actions.rs:118`) for guests, swapped by `as_guest`. Vanilla's `FishingHook` is a level entity like any other; the player only keeps a reference (`Player.fishing`). It ticks in the level's entity loop, in its own turn, and mods see it join, tick and leave.
- **Knots.** `knot_ids` (`lan_entities.rs:430`) derives `LeashFenceKnotEntity`s each tick from the mobs' and boats' leash holders, for tracking only. Vanilla adds a knot entity when a lead is tied to a fence (`LeashFenceKnotEntity.getOrCreateKnot`) and discards it when its last lead goes or its fence does.

## 1. Hooks
- `Game.hooks: Vec<fishing::Hook>` (the level's), swapped per level in `levels.rs` `Parked`. `Hook` gains `owner: Ent` (`Ent::Guest(c)`, or `Ent::Host` in-frame) and `removed: bool`.
- `Kind::Hook`, `NewEntity::Hook`, and the usual arms. The id at construction (`fishing.rs` already numbers it: make that `next_entity`).
- The player's `fishing: Option<u32>` replaces `Game.hook` and `GuestState.hook`: the swap in `as_guest` swaps the id, not the hook.
- `tick_fishing` becomes a loop over `self.hooks` (fixed bound, `removed` marks, `left_in_loop`), and each hook's body runs **as its owner**: `Ent::Guest(c)` → inside `as_guest(host, c, …)`, so `self.player`, `holds_rod()` and the targets are the owner's, exactly what the per-guest call does today. Step 2-10 makes this the hook's turn.
- Removals (`FishingHook.tick` and `retrieve` → `discard()`): `Discarded`, through `level_remove` / `left_in_loop`. The owner's `fishing` is cleared in the same call (vanilla `FishingHook.setRemoved`... check `onClientRemoval`/`updateOwnerInfo(null)` in `FishingHook.java` and port that order).
- Tracking: `lan_entities.rs` reads `self.hooks` with the owner's network id for the spawn data (`ClientboundAddEntityPacket` data = owner id), replacing the per-guest scan.
- Not saved: `FishingHook` doesn't serialize (`EntityType.FISHING_BOBBER` has `noSave()`; check `EntityType.java`).

## 2. Knots
- `Game.knots: Vec<Knot>` (new struct in `game/leads.rs`: `id`, `pos: [i32; 3]`, `removed`, `check_interval`), `Kind::Knot`, the usual arms; swapped per level instead of `knot_ids`.
- **Added** where native ties a lead to a fence: the port of `LeadItem.bindPlayerMobs` / `Leashable.dropLeash` sites (grep `Holder::Knot(` constructors in `game/`): `getOrCreateKnot(pos)` returns the existing knot at `pos` or `level_add`s a new one (`Fresh`), then plays `LeashFenceKnotEntity.playPlacementSound` as vanilla does if native doesn't already.
- **Removed** (`Discarded`):
  - when its last leash goes (`LeashFenceKnotEntity` has no leashed entity: vanilla discards in `Leashable.dropLeash` → `notifyLeasheeRemoved`?, verify in `Leashable.java`);
  - `BlockAttachedEntity.tick`: every 100 ticks `!survives()` → `discard()` then `dropItem`; a knot drops nothing;
  - a player's `interact` with an empty hand (`LeashFenceKnotEntity.interact`), as vanilla.
- **Loaded:** vanilla saves knots (`LeashFenceKnotEntity` serializes). On chunk load, a mob's saved leash holder that is a block position resolves to the knot entity there (`Leashable.restoreLeashFromSave`); knots load with `FromDisk` before the mobs that name them, as section storage does by insertion order. If native has no knot save, add the encoder (`BlockAttachedEntity` keys: `block_pos`).
- `lan_assign_ids`' knot block is deleted; tracking reads `self.knots`.

## Tests
- `game_tests`:
  - `a_hook_ticks_as_its_owner_and_leaves_when_reeled` (a guest's hook: its id is in `self.hooks`, the guest's `fishing` names it, and reeling removes it with `Discarded`);
  - `two_players_hooks_both_tick` (host in-frame skipped; two guests);
  - `a_knot_joins_when_tied_and_leaves_with_its_last_lead`;
  - `a_knot_whose_fence_is_broken_leaves_within_100_ticks`.
- `python3 -m unittest tests.test_entity_lifecycle_sites` (add `hooks|knots`).
- Harnesses `--changed-files` lists (expect `fishing`, `leash`); `python3 tools/lan_e2e.py --check fishing --check lead` (or the names `--list` shows).

## User steps (LAN)
1. The guest fishes; the host watches. The bobber and its line show on both screens, and a catch lands as before.
2. Tie a lead from a cow to a fence, save, quit, reload: the knot and the lead are there.
