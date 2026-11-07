# SPEC-5 (W7): mod fluids tick in Java; native's liquid hooks step aside; native fluids meet mod fluids by table

Read first: `DESIGN.md` §2.2, D3, D4, wire allocations; `crates/horizon-world/src/level/fluids.rs` (`fluid_tick`, `can_be_replaced_with`, `can_hold_fluid`, `can_hold_specific_fluid`, `is_liquid_container`, `place_liquid`, `spread_to`, `should_spread_liquid`, `liquid_block_changed`), `level/tick.rs` (`random_tick_at`, `random_tick_candidate`, the scheduled loop near 1777 and 1826, line 1957), `level/mod.rs` (`LevelAccess::observe_block_tick`), `level/hooks.rs:85,193`; `crates/horizon-client/src/game.rs` `impl LevelAccess for GameLevel` (`observe_block_tick`), `game/java_mods.rs` (`push_event`, `note_block_tick`, `java_drain_block_events`, `BlockEvent`); `GameBridge.blockEvents` (Java); vanilla `ServerLevel.tickFluid`, `FlowingFluid.tick/spread/spreadTo/canSpreadTo/canHoldFluid`, `LiquidBlock`, `LevelChunk`'s random tick of fluid states (`ServerLevel.tickChunk`).

Depends on: SPEC-4. Comments in code: one line max, only a non-obvious why.

## Goal

A placed mod fluid flows, recedes and reacts as on NeoForge, through its own code in Java; native's water and lava treat cells holding a mod fluid and mod fluid-loggable blocks as vanilla would. One owner for each behaviour (DESIGN §2.2).

## Native (horizon-world)

1. `LevelAccess` (`level/mod.rs`, beside `observe_block_tick`): `fn observe_fluid_tick(&self, _pos: BlockPos, _state: BlockStateId, _random: bool) {}` and `fn observe_place_liquid(&self, _pos: BlockPos, _container: BlockStateId, _fluid_block: BlockStateId) {}`.
2. `Sim::fluid_tick(pos)`: after reading `kind`, `if !kind.is_vanilla() { self.level.observe_fluid_tick(pos, state, false); return; }`. The callers in `tick.rs` already check the scheduled type (`TickTypeId == tick.kind`), which is vanilla's `fluidState.is(fluid)`.
3. Random ticks (`tick.rs` `random_tick_at`): after the lava branch, `else if !fluid.is_vanilla() && table().fluid(fluid).random { self.level.observe_fluid_tick(pos, state, true) }` (vanilla `ServerLevel.tickChunk`: a fluid state that `isRandomlyTicking` gets `randomTick`). `random_tick_candidate(state)` must also accept states whose fluid is a random-ticking mod fluid, so sections holding them are visited (vanilla's section count includes fluid states).
4. The `LiquidBlock` hooks step aside for mod fluids: `liquid_block_changed(pos, state)` and `should_spread_liquid` return at once (`true` for the latter's callers to proceed as a no-op: read each caller) when `self.b.fluid(state).kind()` is not vanilla. Java's own `LiquidBlock` runs `onPlace`/`neighborChanged`/`updateShape` for owned blocks (block events 0, 1, 4).
5. Native water/lava meeting a mod fluid, in `can_be_replaced_with(target_fluid, pos, incoming, direction)` (or wherever `canSpreadTo` asks the target's fluid; follow `spread_to`): when `target_fluid.kind()` is a mod fluid, answer `table().fluid(kind).replaceable.get(index_of(target_fluid), incoming, direction, above)` with `above = fluid_at(pos.above()).kind() == table source of kind`. No table (`replaceable` absent): `false` and one log line per fluid `Horizon: fluid <id> has no replaceable table; water and lava won't flow into it` (it can only be absent for a fluid without facts).
6. Mod `LiquidBlockContainer`s: where native decides `canHoldFluid` for a state (`can_hold_fluid` / `can_hold_specific_fluid` / `is_liquid_container`), a state with `table().container(state)` is a container that accepts exactly those fluids (vanilla `canHoldFluid` → `container.canPlaceLiquid(...)`). Where native would then place the fluid into a container (`place_liquid` for vanilla's `SimpleWaterloggedBlock`), for a table container call `self.level.observe_place_liquid(pos, state, self.b.legacy_block(fluid_state))` and do nothing else: Java places it (vanilla `spreadTo` calls `container.placeLiquid` and nothing more for containers).

## Native (horizon-client)

- `impl LevelAccess for GameLevel` (`game.rs`, beside `observe_block_tick`, `#[cfg(feature = "java-mods")]`, `native_authoritative` only): `observe_fluid_tick` → `java_mods::note_fluid_tick(pos, state, random)`; `observe_place_liquid` → `java_mods::note_place_liquid(pos, container, fluid_block)`.
- `java_mods.rs`: `note_fluid_tick` pushes kind `6` if `random` else `5` with `a = java_id(state)`; `note_place_liquid` pushes `7` with `a = java_id(container)`, `b = java_id(fluid_block)`. They drain with the other block events (`java_drain_block_events`).

## Java (`GameBridge.blockEvents`)

New cases (same `try` per event):
```java
case 5 -> { var fs = now.getFluidState(); if (fs.getType() == state.getFluidState().getType()) fs.tick(level, pos, now); }
case 6 -> { var fs = now.getFluidState(); if (fs.isRandomlyTicking()) fs.randomTick(level, pos, level.getRandom()); }
case 7 -> { if (now.getBlock() instanceof LiquidBlockContainer c) c.placeLiquid(level, pos, now, stateOrThrow(b).getFluidState()); }
```
(`now` is the state at the drain; check vanilla's signatures of `FluidState.tick` and `randomTick` in 26.3's source and match them.) `HORIZON_JAVA_DEBUG` prints `Horizon: fluid tick <id> at <pos>` for kind 5, rate-limited to the first 20.

## Tests

- **horizon-world** (`level/tests.rs` or `fluids.rs` tests, with a test `FluidTable` installed: one mod pair whose `replaceable` says "replaceable from DOWN by anything, else not" and a container state accepting water):
  - a scheduled tick of the mod fluid reaches `observe_fluid_tick` once and changes no block (a recording `LevelAccess` stub; assert on the recorded calls and on the unchanged world, not on the stub alone);
  - water spreading sideways next to a mod source doesn't replace it; water above a mod flowing cell replaces it (expected from the fixture's table);
  - water flowing beside the container state calls `observe_place_liquid` with the container and `minecraft:water[level=n]`'s legacy block, and leaves the block unchanged natively;
  - `liquid_block_changed` on a mod liquid state schedules nothing.
- **Java testmod `ChecksFluids`:**
  - `goo_flows`: the probe places `horizon_java_test:goo` on a 9×9 stone floor (`run`), waits; in `later`, the cell two blocks east holds `flowing_goo` with amount 8 - 2·2 = 4 (`levelDecreasePerBlock(2)`, distance 2: vanilla `getNewLiquid` drops by `getDropOff` per block) and four blocks east holds nothing (8 - 2·4 = 0). Independent: the test mod's declared numbers and vanilla's rule.
  - `goo_recedes`: after removing the source (`/setblock air`), within `tickRate × 8` ticks the flowing cells are gone.
  - `water_stops_at_goo`: a water source placed beside a goo source leaves the goo source in place.
- **`tools/java_mods.py` `drive`**: place, wait 60, check; the scenario below.

## Scenario (`tools/scenarios/w7_fluids.json`)

Floor first (`forceload`, `fill ... stone`), `tp`, `near`; `/setblock 0 101 0 mekanism:brine`; `{"wait": 100}`; checks via `horizon-ctl block`: `1 101 0` is `mekanism:brine` with a `level` 1..7, `4 101 0` air (BaseFlowingFluid's default drop-off and slope distance are whatever SPEC-1's facts say for brine: compute the expected reach from those facts in the scenario's comment, not from Horizon's run); `/setblock 0 101 0 air`, `{"wait": 200}`, `1 101 0` air. `{"no_exceptions": true}`.

## Done bar

- `cargo test -p horizon-world fluid`, `-p horizon-client --features java-mods --lib java_`; clippy with and without the feature; `java_mods.py build`, `check`, `unit` (private jars).
- The scenario passes on `play/install-rs` (one run, through the gate).
- M3 against NeoForge (the lead, or the implementer if the oracle path is ready): the same placement on a real NeoForge server gives the same cells after 100 ticks.

## Live test (lead)

1. Creative, on a flat stone area: place a Mekanism brine source with `/setblock`; it spreads like water, slowly, as on Forge (use SPEC-7 to see it; before SPEC-7 use `horizon-ctl blocks` on the area).
2. Put water next to it: the brine source stays. Put a Mekanism block whose facts list `can_place_liquid` with `minecraft:water` (SPEC-1's report names them) at a flowing water's edge: on Forge water enters it (`fluid_logged=water`); the same here (`horizon-ctl block`).
3. Remove the brine source: the flow recedes.
