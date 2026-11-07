# SPEC-2: the player view moves by groups (the hot-route cost, fixed at the root)

Design: `DESIGN.md` §1.4, §2.2, decisions C1 and C2. Needs nothing. Size: ~450 changed lines (Rust ~250, Java ~200).

Paths: `G/` = `crates/horizon-client/src/game/`, `B/` = `mods/horizon/src/java-mods-host/boot/dev/horizonlab/javamods/boot/`, `P/` = `mods/horizon/src/java-mods-host/plugin/dev/horizonlab/javamods/plugin/`, `U/` = `mods/horizon/src/java-mods-host/unit/dev/horizonlab/javamods/boot/`.

Read first: `G/java_mods.rs` `java_use_request`, `java_apply_use`, `java_refresh_player`, `java_player_left`, the `players_removed` and `server_stop` calls; `G/java_routing.rs` `route_call`, `java_player_arg`; `B/GameBridge.java` `fill`, `use` (its end: the changed-slot diff), `refresh`, `removePlayer`, `closeWorld`; `B/PlayerSync.java` `fill`; `P/BridgeProcessor.java` `routeStub`, `nativeCall`.

Comments in code: one line at most, only for a non-obvious why.

## Why
A route on a player costs two full player fills (about 100 µs each), and every `use`-family op (`player_tick` each tick for each player, item clicks, mining) pays one: 145–456 µs per `player_tick` in the `bridges` session (`agents/w12s3/scratch/1/java-ticks.log`). Almost all of it is the 41 stacks encoded to NBT in Rust and decoded through `ItemStack.CODEC` in Java, plus the effect map rebuilt, whether anything changed or not.

## 1. Wire format (both sides)
Every request that today starts with `java_use_request()`'s bytes starts with this instead:
```
long uuid_hi, long uuid_lo, text name, text dim, long game_time, int hand (0 main, 1 off), int mask
then, for each bit of mask in this order:
  POSE      1    text pose, double x, y, z, float yaw, pitch, bool shift
  MODE      2    int game_mode
  SELECTED  4    int slot
  XP        8    int level, float progress, int total
  VITALS    16   float health, int food, float saturation, float exhaustion, float absorption
  EFFECTS   32   int n, n x (text id, int duration, int amplifier, bool ambient, bool visible)
  MOTION    64   double vx, vy, vz, bool on_ground
  STACKS    128  int n, n x (int slot, bytes tag)        bytes of length 0 = empty slot
  FULL      1<<30  every group above is present and the view must take them as its whole state
```
`hand` moved up because it is a call argument, not view state. A reply that carries changed slots is unchanged.

## 2. Rust (`G/java_mods.rs`)
- A plain snapshot, so the encoder is testable without a `Game`:
  ```rust
  /// What the acting player's Java view is filled from.
  pub(super) struct PlayerSnapshot { pose: String, at: [f64; 3], yaw: f32, pitch: f32, shift: bool, mode: i32, selected: i32, xp: (i32, f32, i32), vitals: (f32, i32, f32, f32, f32), effects: Vec<(String, i32, i32, bool, bool)>, motion: ([f64; 3], bool), stacks: Vec<Option<ItemStack>> }
  ```
  `fn java_player_snapshot(&mut self) -> Option<PlayerSnapshot>` builds it from the same fields `java_use_request` reads today (stacks: `java_slot(0..=OFFHAND)`, 41 entries).
- `#[derive(Default)] pub(super) struct Sent { primed: bool, pose: Option<(String, [f64; 3], f32, f32, bool)>, mode: Option<i32>, selected: Option<i32>, xp: Option<(i32, f32, i32)>, vitals: Option<…>, effects: Option<Vec<…>>, motion: Option<…>, stacks: Vec<Option<ItemStack>> }`.
- `thread_local! { static SENT: RefCell<HashMap<[i32; 4], Sent>> }`, keyed by `acting_uuid()`. The server thread owns it, as `java_entities.rs`'s `VIEWED`.
- `pub(super) fn encode_player_groups(now: &PlayerSnapshot, sent: &mut Sent, out: &mut Out) -> u32`: writes the mask and the groups (format §1) and updates `sent`; returns the mask. Rules:
  - `!sent.primed` → FULL: every group, every slot (`stacks` written for the 41 slots, empty ones as length 0); then `primed = true`.
  - Otherwise a group goes when its value differs from `sent`'s (`f32`/`f64` compared with `to_bits`, so `-0.0` and NaN behave). STACKS lists only slots whose `Option<ItemStack>` differs (`ItemStack: PartialEq`, `horizon-items/src/items.rs:330`).
- `java_use_request` keeps its name and callers and writes header + `encode_player_groups`. The NBT encoding of a stack (`item_to_nbt` + `write_tag`) moves inside the STACKS loop, so unchanged stacks cost nothing.
- `java_apply_use` (and any reply reader that applies changed slots): after writing a slot natively, set `sent.stacks[slot]` to the new value, and `sent.xp` to the experience it applied. Java's view already holds them; the next request must not send them back.
- `java_refresh_player` is the same request (op `refresh`): it now carries only what changed since the call began.
- **Route replies carry the acting player's changes.** Today only `use`-family replies return changed slots and experience; a `route` call whose receiver or argument is the acting player (`Recv::Player`, `PlayerArg`) drops what Java changed. Java's `Routing.run` ends such a reply, after the value and before the effects, with the same block `use` writes (`int level, float progress, int total, int n, n × (int slot, bytes)`) against `SYNCED`; `route_call` applies it through the code path `java_apply_use` uses (so `SENT` follows). Routes without a player write `int -1` there. Bump the `route` reply layout in `RoutingArgsCheck` and `read_value`'s tests.
- `pub(super) fn java_player_view_reset(uuid: Option<[i32; 4]>)`: removes one entry (`Some`) or all (`None`). Call it:
  - in `java_player_left` before the request (its view is dropped by `removePlayer`);
  - before `players_removed` and on `server_stop` (all);
  - wherever SPEC-1d's `player_level` op (if landed) makes Java rebuild a player's view.
  Grep `PLAYERS.remove` and `PLAYERS.clear` in `B/` and pair each with one of these; list them in the report.
- New bridge, `G/java_bridges/player.rs`: `"dev/horizonlab/javamods/boot/PlayerSync".stacksWritten "([B)V" => fn player_stacks_written(game, slots: Bytes)`. It reads `int n, n x (int slot, bytes tag)`, writes each slot of the acting player (`java_slot`, `hand_stack_replaced` as `java_apply_use` does), and updates `SENT`. Check `java_mods.py check` accepts a native on a Horizon class (`LevelApi` is the precedent).

## 3. Java
- **`B/PlayerSync.java`:** `static final Map<UUID, ItemStack[]> SYNCED` (41 entries, copies). `static void forget(UUID)` and `forgetAll()`, called from `GameBridge.removePlayer` and `closeWorld`.
- **`GameBridge.fill(in, refresh)`:**
  - read the header; `int mask`; for each present group run today's code for it (POSE: `setPose`, `absSnapTo`, head rotation, shift; MODE: the mode block with `Menus.gameModeChanged`; SELECTED; XP; VITALS + EFFECTS: `PlayerSync.fill` split into `fillVitals` and `fillEffects`, so `sendEffectChanges` still runs only when effects came; MOTION).
  - `joined` (a new view) without FULL: throw `IllegalStateException("Horizon: the player view of " + name + " got a delta before its full fill")`. Never fill from defaults.
  - FULL: `inventory.clearContent()` and set the 41 slots, as today; `SYNCED[uuid]` = copies.
  - STACKS without FULL: for each listed slot decode, `inventory.setItem(slot, stack)`, `SYNCED[slot] = stack.copy()`. No `clearContent()`.
  - Drift (decision C2): before applying STACKS, for every slot not listed, if `!ItemStack.matches(inventory.getItem(slot), SYNCED[slot])`, set the slot back to a copy of `SYNCED[slot]` and `Profile.event("player view drift", 0)` when `Profile.ON`. This is today's outcome (the next fill overwrote it).
  - `refresh` mode applies XP and STACKS too now: a native body's writes are later than Java's uncommitted ones, so they win, and `SYNCED` follows.
  - `Call.before` becomes `SYNCED[uuid]` (copies), so `use`'s end-of-call diff is against what native holds. After the diff, `SYNCED[slot] = copy(view)` for each changed slot.
- **`PlayerSync.beforeNativeBody()`** (static, no arguments): for the acting player (`PlayerSync.acting`), diff the 41 slots against `SYNCED`; if any differ, call `stacksWritten(bytes)` (`static native void stacksWritten(byte[])`) with them and update `SYNCED`. No changes: no crossing.
- **`P/BridgeProcessor.routeStub`:** in the head it builds, insert `INVOKESTATIC dev/horizonlab/javamods/boot/PlayerSync.beforeNativeBody ()V` after the backed test and before the native call (inside `nativeCall`'s list, behind a flag only `routeStub` sets; plain bridges don't get it). The plugin runs before the boot classes are loaded: reference the class by name only, as `ROUTING` does.
- Profile: `Profile.event("fill", ns)` around `GameBridge.fill` when `Profile.ON`.

## 4. Tests
**Rust** (`G/java_mods.rs`, `#[cfg(all(test, feature = "java-mods"))]`), on `encode_player_groups` with a hand-built `PlayerSnapshot`; expected bytes written out in the test:
- `first_request_is_full`: mask = `FULL|POSE|MODE|SELECTED|XP|VITALS|EFFECTS|MOTION|STACKS`, 41 slot entries.
- `unchanged_player_sends_no_group`: second call with the same snapshot writes exactly `int 0` after the header (4 bytes).
- `one_moved_stack_sends_two_slots`: stone moves from slot 0 to slot 5: mask = `STACKS`, `n = 2`, slots 0 (length 0) and 5 (the stone's tag).
- `selected_slot_only`: mask = `SELECTED` (4), then `int 3`.
- `reset_makes_the_next_request_full`.
- `applied_reply_is_not_sent_back`: after the code path `java_apply_use` uses to update `Sent` sets slot 2, the next encode of a snapshot holding that stack writes mask 0.
- `negative_zero_is_a_change`: yaw `0.0` then `-0.0` sends POSE (bit comparison).
Print the first test's bytes (`println!("PLAYER_FULL {:02x?}")`) for the Java check.

**Java** (`U/PlayerGroupsCheck.java`, new, `java_mods.py unit`): factor the header and group reading into `PlayerGroups.read(Crossing.In) -> PlayerGroups` (a record of what was present, no view writes). Feed it the bytes the Rust tests print (pasted as hex) and assert each field, and that an unknown mask bit throws.

**Live `bridges` session** (test mod, `J/testmod/`): two server-side checks in `ChecksRouting` (or the test mod's player tick listener):
- `player_stack_keeps_identity`: on two consecutive `PlayerTickEvent.Post` for the same player with slot 0 unchanged, `System.identityHashCode(inventory.getItem(0))` is equal. Today it differs at every tick; vanilla keeps the object.
- `player_view_sees_native_give`: the session gives `minecraft:apple` natively (`/give`) between two ticks; the next `PlayerTickEvent.Pre` sees 1 apple in the slot `/give` filled.
- The existing `route_tail_sees_body`, `route_head_cancel`, `route_return_equals_tail` still pass.

Run: `cargo test -p horizon-client --features java-mods --lib java_mods`, `cargo clippy -p horizon-client --features java-mods -- -D warnings` and without the feature, `python3 tools/java_mods.py build` (private `HORIZON_JAVA_MODS_DIR`), `check`, `unit`, one `bridges` run (gate first).

## Done bar (the implementer checks)
- All tests above pass; `java_mods.py check` reports no new problem.
- One `bridges` run with `HORIZON_PROFILE`: report the `route`, `refresh`, `player_tick` and `fill` means next to the §1.4 table. Expected: `refresh` ≤ 10 µs, `route` ≤ 40 µs, `fill` with no change ≤ 10 µs. If one misses, report the profile; don't tune by skipping work.
- No `player view drift` events in that session (the test mod changes no stack outside a call).

## For the lead
- `python3 tools/parity.py --changed-files <files> --list` names nothing (no native behaviour changed); run what it names if it does.
- In the play install (`play/launch.sh` or `live.sh start`): mine with a Mekanism Atomic Disassembler, use a Mekanism energy tablet in a charger, put items in and out of an AE2 ME chest, throw items, die and respawn, change dimension through a nether portal, leave and rejoin. Expected: everything as before; the boot and session logs have no `got a delta before its full fill` and no `player view drift`. `profile_report.py` on the session shows `player_tick` mean well under the old 150–450 µs.
- Dedicated server with two clients (`lan.sh` or `--dedicated` per `docs/hosting.md`): both players' hotbar switches and item uses work; leave and rejoin one of them.
