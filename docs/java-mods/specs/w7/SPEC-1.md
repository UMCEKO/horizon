# SPEC-1 (W7): server facts for fluids, every state's fluid, and the test fluid

Read first: `DESIGN.md` §1.1, §2.1, §2.2; `mods/horizon/src/facts-shared/java/dev/horizonlab/facts/FactsWriter.java`; `BootFacts.java` (`run()` at the anonymous `FactsWriter`); `mods/horizon/src/neoforge-oracle-26/java/dev/horizonlab/neoforge/FactsExporter.java` (`Export.run()`); `crates/horizon-port-pack/src/oracle.rs` (`Facts`, `Block`, `State`); vanilla `FlowingFluid.java`, `LiquidBlock.java`, `FluidState.java` under `.local/minecraft/26.3/sources/server/net/minecraft/world/level/`.

Depends on: none. Comments in code: one line max, only a non-obvious why.

## Goal

The boot facts (and the NeoForge oracle's, same writer) describe every fluid and every block state's fluid, so later specs never derive a fluid from a name. Nothing native changes yet.

## Java

### `FactsWriter`

1. In `blocks()`, inside each state's object, after `collision`: when `!state.getFluidState().isEmpty()`:
   ```
   "fluid": {"id": <BuiltInRegistries.FLUID key of fs.getType()>, "amount": fs.getAmount(), "falling": <fs.getValue(FlowingFluid.FALLING) if fs.hasProperty(FALLING), else false>, "source": fs.isSource()}
   ```
   Written for vanilla and mod states alike: vanilla rows are SPEC-1's oracle.
   For a **mod** block whose class implements `LiquidBlockContainer` (Mekanism's fluid-loggable blocks), also write `"can_place_liquid": [fluid ids]`: every fluid of `BuiltInRegistries.FLUID` (except empty) for which `container.canPlaceLiquid(null, probe, PROBE_POS, state, fluid)` is true, `probe` answering `state` at `PROBE_POS` and air elsewhere (the same probe getter as `replaceable` below). Native can't ask Java inside its cascade (DESIGN D4), so this is how native water decides to flow into such a block.
2. New `public void fluids()`, called right after `blocks()` in both `run()`s (`BootFacts`, oracle `Export`). For every `Fluid` in `BuiltInRegistries.FLUID` whose id `scope.mod(...)` or `scope.vanilla(...)` admits (use `BuiltInRegistries.FLUID`, as `blocks()` does with `BLOCK`), skip `minecraft:empty`, write `"fluids": { id: {...} }`:
   - `class`: the class chain, most derived first (as blocks).
   - `source`, `flowing`: for a `FlowingFluid`, the ids of `getSource()` and `getFlowing()`; otherwise the fluid's own id twice.
   - `bucket`: the id of `getBucket()` (`minecraft:air` when none).
   - `states`: for each `FluidState` of `getStateDefinition().getPossibleStates()`, key `amount=<n>,falling=<b>` (from the properties present; `""` for a fluid without properties): `{"amount", "falling", "source", "random": fs.isRandomlyTicking(), "block": FactsWriter.stateKey-style "<block id>[<state key>]" of fs.createLegacyBlock()}`.
   - `code`: for each name in `CODE` below, `"vanilla"`, `"neoforge"` or `"mod"`: the package of the class that declares the method the fluid's class resolves to (walk `getClass()` up through `getDeclaredMethod(name, params)` with `setAccessible`, since several are protected). `net.minecraft.` → vanilla, `net.neoforged.` → neoforge, else mod.
     `CODE`: `tick(ServerLevel,BlockPos,BlockState,FluidState)`, `randomTick(ServerLevel,BlockPos,FluidState,RandomSource)`, `animateTick(Level,BlockPos,FluidState,RandomSource)`, `canBeReplacedWith(FluidState,BlockGetter,BlockPos,Fluid,Direction)`, `getTickDelay(LevelReader)`, `getDropOff(LevelReader)`, `getSlopeFindDistance(LevelReader)`, `canConvertToSource(ServerLevel)`, `beforeDestroyingBlock(LevelAccessor,BlockPos,BlockState)`, `getExplosionResistance()`, `getPickupSound()`. Check each signature against vanilla's `Fluid`/`FlowingFluid` source; a name a class doesn't have (a non-flowing fluid) is left out.
   - `replaceable` (mod fluids only; vanilla's rules are ported): a string of `'0'/'1'`, index `((state * F + incoming) * 6 + direction) * 2 + above`, where `state` is the index in `states` order, `incoming` the index of every fluid in `BuiltInRegistries.FLUID` order (empty included), `direction` `Direction.get3DDataValue()`, `above` 0 = nothing above, 1 = the fluid's own source above. Value: `fs.canBeReplacedWith(probe, PROBE_POS, incoming, direction)` where `probe` is a `BlockGetter` that answers `fs.createLegacyBlock()` at `PROBE_POS`, the source's legacy block at `PROBE_POS.above()` when `above == 1`, air (and its empty fluid) elsewhere, and no block entities. Write `"replaceable_fluids": F` beside it. `PROBE_POS` = `new BlockPos(0, 64, 0)`.
   - `fluid_type` (NeoForge's): see below.

   Before the `fluids` object, write `"fluid_order": [every id of BuiltInRegistries.FLUID in registry order, minecraft:empty first]`: the `incoming` index of `replaceable` counts in it. Never send Java's numeric fluid ids anywhere else: a fluid a namespace no jar names isn't carried, so native's ids can differ.
3. `fluid_type`: write it in `FactsWriter` if `facts-shared` compiles with NeoForge on the classpath (check the javac classpath in `tools/java_mods.py build` and `tools/neoforge_oracle.py`; both run on NeoForge 26.3.0.101). Otherwise add `protected void fluidExtras(Fluid fluid) throws IOException {}` in `FactsWriter`, called inside each fluid's object, and implement it once in a new `java-mods-host/boot/.../FluidTypeFacts.java` used by `BootFacts` and copied into the oracle's `Export` (the oracle has no access to the host's classes). Content, from `fluid.getFluidType()`:
   - `"id"`: its key in `NeoForgeRegistries.FLUID_TYPES`.
   - The private fields of `net.neoforged.neoforge.fluids.FluidType` by reflection (names from `javap -p`; reading NeoForge's source is banned): `motionScale`, `canPushEntity`, `canSwim`, `canDrown`, `fallDistanceModifier`, `canExtinguish`, `canConvertToSource`, `supportsBoating`, `canHydrate`, `lightLevel`, `density`, `temperature`, `viscosity`, `isWaterLike`; `sounds` as `{SoundAction name: sound id}`.
   - `"overrides"`: the public methods of `FluidType` (javap list) that the type's class re-declares outside `net.neoforged.` (e.g. `isVaporizedOnPlacement`, `getBlockPathType`), by name.
4. `owned()`: add `ownedOf("fluid", BuiltInRegistries.FLUID)` (the boot's `scope.mod` already covers `FLUID`, which `BootFacts.REGISTRIES` snapshots).
5. `schema` 3 → 4 in both `run()`s.

### The test fluid (`testmod`)

New `testmod/.../Goo.java`, registered from `TestMod` beside `Battery.register(bus, ITEMS)`; namespace `horizon_java_test`:
- `FluidType` `horizon_java_test:goo`: `Properties.create().motionScale(0.002).canSwim(true).canDrown(true).fallDistanceModifier(0.5F).canExtinguish(true).canConvertToSource(false).lightLevel(3).density(2000).viscosity(3000).sound(SoundActions.BUCKET_FILL, SoundEvents.BUCKET_FILL_LAVA).sound(SoundActions.BUCKET_EMPTY, SoundEvents.BUCKET_EMPTY_LAVA)`.
- `BaseFlowingFluid.Source` `horizon_java_test:goo`, `BaseFlowingFluid.Flowing` `horizon_java_test:flowing_goo` with `new BaseFlowingFluid.Properties(TYPE, SOURCE, FLOWING).tickRate(10).levelDecreasePerBlock(2).slopeFindDistance(2).explosionResistance(100F).block(BLOCK).bucket(BUCKET)`.
- `LiquidBlock` `horizon_java_test:goo` (`BlockBehaviour.Properties.ofFullCopy(Blocks.WATER).lightLevel(s -> 3)`), `BucketItem` `horizon_java_test:goo_bucket` (`craftRemainder(Items.BUCKET).stacksTo(1)`).
These numbers are the independent expected values of SPEC-1, 4, 5, 9.

## Rust (`crates/horizon-port-pack/src/oracle.rs`)

- `State`: `#[serde(default)] pub fluid: Option<StateFluid>`, `#[serde(default)] pub can_place_liquid: Option<Vec<String>>`; `pub struct StateFluid { pub id: String, pub amount: u8, pub falling: bool, pub source: bool }`.
- `Facts`: `#[serde(default)] pub fluid_order: Vec<String>`, `#[serde(default)] pub fluids: BTreeMap<String, Fluid>`; `Fluid { class, source, flowing, bucket, states: BTreeMap<String, FluidStateFacts>, code: BTreeMap<String, String>, replaceable: Option<String>, replaceable_fluids: Option<u32>, fluid_type: Option<FluidTypeFacts> }`, `FluidStateFacts { amount, falling, source, random, block }`, `FluidTypeFacts` with the fields above (`overrides: Vec<String>`, `sounds: BTreeMap<String, String>`). All new fields `#[serde(default)]` so schema-3 facts still read.
- `Owned`: `#[serde(default)] pub fluid: BTreeSet<String>`; `impl Facts { pub fn owns_fluid(&self, id: &str) -> bool }` beside `owns_block`.
- `impl Fluid { pub fn replaceable(&self, state: usize, incoming: usize, direction: usize, above: bool) -> Option<bool> }` decoding the bit string (None when absent or out of range).

## Tests

- **Rust, `oracle.rs` tests:** a fixture with one fluid and two states parses; `replaceable(...)` reads the documented index (fixture string built by hand from the formula with 2 states, 3 fluids).
- **Rust, `#[ignore]`d, `crates/horizon-port-pack/src/oracle.rs` or a new `tests/fluid_facts.rs`:** with `HORIZON_FACTS=<facts.json>`, every vanilla state's exported `fluid` equals `horizon_world::fluid::FluidState::from_block` on the same state of `.local`'s registry (kind name, amount, falling), and every vanilla state without `fluid` derives `Empty`. Expected values come from the real game; the native derivation is the code under test. Record any mismatch: it is a native bug to fix in SPEC-4, not here.
- **Java, `testmod` `Checks*` (new `ChecksFluids`, added to the server checks list):**
  - `goo_facts`: in the facts file the boot wrote, `fluids["horizon_java_test:flowing_goo"].states` has 16 entries; for `amount=n,falling=false` the `block` is `horizon_java_test:goo[level=<8-n>]` and for `falling=true` `level=8` (vanilla `FlowingFluid.createLegacyBlock` → `LiquidBlock` level: `getLegacyLevel`; read it in vanilla's source and assert those constants).
  - `goo_code`: `code.getTickDelay == "neoforge"`, `code.tick == "vanilla"`.
  - `goo_type`: `fluid_type.motionScale == 0.002`, `canConvertToSource == false`, `lightLevel == 3`, `sounds.bucket_fill == "minecraft:item.bucket.fill_lava"` (the `SoundAction` name as NeoForge's `SoundActions` constant spells it; check with javap).
  - `water_rows`: `blocks["minecraft:water"].states["level=0"].fluid == {id: minecraft:water, amount: 8, falling: false, source: true}`; `level=8` → `flowing_water`, amount 8, falling true.

## Done bar

- `java_mods.py build`, `check` (all CHECKs, both sessions), `unit` pass in a private `HORIZON_JAVA_MODS_DIR`.
- `cargo test -p horizon-port-pack oracle`, clippy `-D warnings` on `horizon-port-pack`.
- The boot facts of `play/install-rs` (`game/java_content.rs` `facts_file()`): `.fluids | keys` includes `mekanism:brine`, `mekanism:flowing_brine`, every `mekanismgenerators:` fluid, `horizon_java_test:goo`; `blocks["mekanism:brine"].states["level=0"].fluid.id == "mekanism:brine"`; a Mekanism block with `fluid_logged=water` has `fluid.id == "minecraft:water"`. Paste the `jq` lines in the report.
- The `#[ignore]`d vanilla comparison run once with those facts; its result (pass, or the list of mismatching states) is in the report.

## Live test (lead)

None in game: facts only. The lead runs `jq '.fluids | to_entries[] | {k: .key, code: .value.code, ov: .value.fluid_type.overrides}'` on the showcase boot's facts and checks: BOP's `blood` has `canBeReplacedWith: "mod"`; Mekanism's types list `isVaporizedOnPlacement`.
