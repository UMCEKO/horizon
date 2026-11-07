# SPEC-2-16: `EntityTickEvent.Pre` and `Post`, per entity, in order

Index: `SPEC-2-8.md` (E4, E7). Size: ~220 lines (Rust ~90, Java ~60, test mod ~70). Needs 2-1 and 2-14.

## Vanilla, patched (javap)
- `ServerLevel.tickNonPassenger(e)`: `if (!EventHooks.fireEntityTickPre(e).isCanceled()) { e.commonTick(); e.tick(); EventHooks.fireEntityTickPost(e); }`, then the passengers **whatever Pre said**.
- `Entity.rideTick()`: `setDeltaMovement(ZERO)`; `if (!fireEntityTickPre(this).isCanceled()) { tick(); fireEntityTickPost(this); }`; then `positionRider` if a passenger. `commonTick` ran before, in `tickPassenger`, and is **not** skipped by a cancel.

## Rust
### Sites (`game/tick_entities.rs`)
```rust
// tick_non_passenger
if !self.java_hook_entity_tick(e, Phase::Pre) { self.turn(pass, e, false)?; self.java_hook_entity_tick(e, Phase::Post); }
for p in self.passengers_of(e) { self.tick_passenger(pass, e, p)?; }
// rideTick inside turn(.., riding: true): zero motion; if !pre-canceled { tick body; post }; position_rider
```
`turn(.., riding: true)` splits so `commonTick` runs before the Pre call and the body after it.

### `java_hooks.rs` pair
`java_hook_entity_tick(&mut self, e: Ent, phase: Phase) -> bool` (true = canceled; always false for `Post` and in the not-`java-mods` arm).

### `java_entities.rs` `java_entity_tick(e, phase) -> bool`
1. `let hook = if Pre { Hook::EntityTickPre } else { Hook::EntityTickPost };`
2. `if !java_level_running() || !facts.hook_admits(hook, type_id(e), false) { return false; }` — one bit, before anything else is built.
3. `mask = facts.reads(type) | facts.hook_reads(hook)`; write the request: `int id` (players: their network id, plus `bool player` and the UUID), `byte phase`, `int mask`, the group bytes (`write_groups`).
4. Send op `entity_tick` through `java_level_call`; read `bool canceled`; apply the effects; `note_touched` for what Java wrote (the bridges already do) and `refresh_pass` at the call site.

## Java
- `GameBridge.register`: `Crossing.game("entity_tick", GameBridge::entityTick)`.
- `entityTick(In, Out)`: the subject is `EntityViews` `LIVE.get(id)` (or `GameBridge.playerView(uuid)`); `EntityViews.apply(view, mask, in)`; then
  - Pre: `canceled = safely("EntityTickEvent.Pre", () -> EventHooks.fireEntityTickPre(view).isCanceled())`;
  - Post: `safely("EntityTickEvent.Post", () -> EventHooks.fireEntityTickPost(view))`;
  - write `canceled`, then `effects()`.
- A missing view is a bug: log once with the id and type, reply not canceled.

## Bridges
Every writer 2-1 lists under `entity_tick_pre`/`entity_tick_post` must be bridged (expect none beyond what mods already need; the hooks construct and post). `java_mods.py check --analysis` shows it.

## Test mod (`TestMod`, `ChecksEntities`)
- **Listeners:**
  - `TickOrder` (Pre and Post, no guard): while `ChecksEntities.recording`, appends `(phase, id, y)` for entities carrying the tag `horizon_order`;
  - `LivingTicks` (Post, Mekanism's guard shape: `isClientSide → return`, `!(e instanceof LivingEntity) → return`, `e instanceof Player → return`): counts per server tick;
  - `Freeze` (Pre): cancels for entities tagged `horizon_frozen`.
- **`run`:** summon in order a pig, an item, an armor stand, each tagged `horizon_order`, all 3 blocks up; start recording.
- **`later` checks:**
  - `tick_events_interleave`: the first recorded tick is exactly `Pre pig, Post pig, Pre item, Post item, Pre stand, Post stand`;
  - `tick_body_runs_between`: for the item, Post's `y` < Pre's `y` (it fell during its tick);
  - `pre_cancel_skips_the_tick`: a zombie tagged `horizon_frozen` 3 blocks up hasn't moved after 20 ticks, and its `tickCount` read through a bridge query is unchanged;
  - `living_count_matches`: `LivingTicks`' count for the last full tick equals the server's living non-player entities in ticking range at that tick, which `Checks.command("execute if entity @e[type=!player,…]")` cannot filter by range: compare instead with the count native reports through a test-only `HORIZON_JAVA_DEBUG` log line `Horizon: entity_tick post n=<k>` per tick.
- `java_mods.py bridges` must list these names (add them to its required list).

## Measurement (E7, DESIGN §10 risk 1)
Run the 360-mob scenario with the pack (Mekanism listens to Post) under `HORIZON_PROFILE`; report `entity_tick` calls per tick and mean µs from `profile_report.py`, and the tick's mean and max. This is the input for the host-crossing target (≤ 1 µs per crossing).

## Tests to run
- `cargo test -p horizon-client --features java-mods --bins java_entities` (the request layout, against a Rust mirror of Java's reader).
- `java_mods.py build`, `check --analysis`, `bridges`.
- `python3 tools/parity.py server_entities mob_ai` (unchanged: no listener without mods).
