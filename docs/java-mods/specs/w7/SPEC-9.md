# SPEC-9 (W7): entities in mod fluids (two parts: 9a the NeoForge trace harness, 9b the native port)

Read first: `DESIGN.md` §1.1 G7, D10, Q4; vanilla `Entity.updateInWaterStateAndDoFluidPushing`/`updateFluidHeightAndDoFluidPushing`, `Entity.updateFluidOnEyes`, `LivingEntity.travel`/`shouldTravelInFluid`/`travelInFluid`/`travelInWater`/`travelInLava`/`jumpInLiquid`, `ItemEntity.tick` (its fluid branch); javap (signatures only) of the patched `Entity`, `LivingEntity` and NeoForge's `IEntityExtension`/`ILivingEntityExtension`/`FluidType` (`isInFluidType`, `getFluidTypeHeight`, `getFluidMotionScale`, `isPushedByFluid`, `canSwimInFluidType`, `jumpInFluid`, `sinkInFluid`, `FluidType.move`, `setItemMovement`); native `crates/horizon-client/src/motion/fluids.rs` (`update_fluids`, `fluid_height`, `is_tagged`), `entities/fluids.rs`, the mob travel port (`mobs/living.rs`, search `travel_in_water`), `tools/neoforge_oracle.py` and an existing NeoForge-vs-Horizon harness to copy (`tools/test_*_parity.py` that uses `neoforge_oracle`; pick one with entity motion if any, else any).

Depends on: SPEC-5 (mod fluids exist and tick), SPEC-1 (`fluid_type` numbers). Comments in code: one line max, only a non-obvious why. NeoForge's source is never read: the expected values are its runtime behaviour.

## Goal

A mob, an item and a player in a mod fluid move as on NeoForge: slowed by the type's motion scale, pushed by its flow, swimming where `canSwim`, drowning where `canDrown`, fall damage scaled by `fallDistanceModifier`, extinguished where `canExtinguish`.

## 9a: the trace harness (lands alone)

- **Oracle** (`mods/horizon/src/neoforge-oracle-26/java/.../FluidMotionOracle.java`, run by `neoforge_oracle.py` with the test jar in the pack, on a real NeoForge 26.3.0.101 server): a void flat world; a 7×4×7 basin of stone at y 64; fill its inside with `horizon_java_test:goo` sources (case A), with one source in a corner and flow (case B); for each case spawn, 3 blocks above the basin centre: a zombie with `NoAI` and `PersistenceRequired` (gravity and fluid physics still apply), and an item entity (stone) with zero motion; record 80 ticks of `getX/Y/Z`, `getDeltaMovement`, `isInFluidType()`, `getFluidTypeHeight(type)`, `fallDistance`, `getAirSupply()` per entity. Also case C: the same zombie dropped from 20 blocks into a 3-deep goo pool (fall damage: record health). Output JSON keyed `case/entity/tick`.
- **Probe** (`crates/horizon-client/examples/fluid_motion_probe.rs`, built into `.local/parity-target/` as probes are): the same world through the game's server (`game/` fixture env var pattern, e.g. a `HORIZON_FLUID_FIXTURE` read where `sculk_fixture.rs` reads its own) with the SPEC-4 table of the test fluid installed from the boot facts; the same records.
- **Harness** `tools/test_fluid_motion_parity.py` (copy a nearby harness): runs both, compares per tick with tolerance 1e-6 on positions (vanilla's own doubles), exact on booleans and air. Water as a control case (`minecraft:water` instead of goo): it must already match before 9b.
- Done bar 9a: the harness runs; the water control passes; the goo cases fail (expected), and the report pastes the first diverging tick for each.

## 9b: the native port

- `FluidKind` → its fluid type's numbers (`ModFluid.fluid_type`, SPEC-4): `motion_scale`, `can_push_entity`, `can_swim`, `can_drown`, `fall_distance_modifier`, `can_extinguish`.
- **Heights and pushing** (`motion/fluids.rs` `update_fluids`, and the mob/item equivalents): vanilla computes a height and a flow push per fluid tag; NeoForge per fluid type. Keep water and lava exactly; add one entry per mod fluid type present in the entity's box, computed by the same loop as vanilla's water one with that type's `motion_scale` and `can_push_entity`. Expose `fluid_type_height(kind)` and `in_fluid_type()`.
- **Travel** (players' `motion.rs`, mobs' `mobs/living.rs`): where vanilla's `travel` asks `shouldTravelInFluid` and `travelInFluid` chooses water or lava, a mod fluid type with `can_swim` takes the branch the trace shows. Start from `travelInWater` with the type's numbers in place of water's constants, run the harness, and adjust until each goo case's diff is 0. Every constant that differs from vanilla's water path must be justified in the report by the trace tick that shows it.
- **Items**: `entities/fluids.rs` per the trace (NeoForge's `FluidType.setItemMovement`).
- **Air and drowning**: `can_drown` decides as water's tag does (`baseTick`'s air supply).
- **Fall distance**: on entering a mod fluid type, `fall_distance *= fall_distance_modifier` (check the trace: the zombie in case C).
- **Extinguish**: `can_extinguish` clears fire as water does.
- Not here: boats (`supportsBoating`), mob pathing (`getBlockPathType`): W9.

## Tests

- 9a: the harness itself, plus a unit test that the probe's record format parses.
- 9b: the harness (goo cases A, B, C and the water control: diff 0); unit tests of `update_fluids` with a mod fluid type of `motion_scale` 0.002: the push vector after one tick in a flowing goo cell equals the vanilla formula with 0.002 for 0.014 (worked out in the test comment).

## Done bar

- 9a: as above. 9b: `python3 tools/parity.py fluid_motion` (through `verify/parity.sh`) passes; `motion_parity`-type harnesses that cover water (run `parity.py --changed-files ... --list` on the touched files and run at most the first two) still pass; clippy; `pr_ready.py --quick`.

## Live test (user)

1. Walk into a Mekanism brine pool: movement slows as on Forge; jump swims up if Mekanism's type allows swimming (Forge side by side).
2. Stay under: the air bubbles drop if the type drowns.
3. Fall 20 blocks into a 3-deep brine pool: damage as on Forge.
4. Drop an item into flowing brine: it drifts with the flow.
