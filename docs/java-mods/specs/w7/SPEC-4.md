# SPEC-4 (W7): native fluids become an open set

Read first: `DESIGN.md` §1.1 (G1, G2), §2.2; `crates/horizon-world/src/fluid.rs` (`FluidKind`, `FluidState::from_block`, `from_json`, `to_json`, `FluidStateTable`), `registry_fluid.rs` (`FluidRegistry::new`), `crates/horizon-items/src/blocks.rs:405-430` (`StateFacts.fluid`), `crates/horizon-client/src/motion/fluids.rs:117` (`is_tagged`), `game/java_bridges/level_api.rs:35` (`java_schedule_fluid`), `game/java_bridges/client_world.rs:101` (the `FLUID` flag), `save.rs` (fluid ticks in chunk NBT), `game.rs` `load_core`; SPEC-3's `horizon/fluids.json`.

Depends on: SPEC-3. Comments in code: one line max, only a non-obvious why. Upstream-bound prose must not name NeoForge.

## Goal

Native knows every fluid the root declares: a mod liquid block's states and Mekanism's logged states hold their fluid; a mod fluid's scheduled tick is kept, saved and loaded. Native gives mod fluids **no behaviour** here: their ticks do nothing yet (SPEC-5), they draw nothing yet (SPEC-7). Vanilla behaviour is byte-for-byte unchanged.

## `FluidKind` (horizon-world)

Replace the enum by a newtype, keeping every vanilla spelling compiling:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FluidKind(u16);
#[allow(non_upper_case_globals)]
impl FluidKind {
    pub const Empty: Self = Self(0);
    pub const FlowingWater: Self = Self(1);
    pub const Water: Self = Self(2);
    pub const FlowingLava: Self = Self(3);
    pub const Lava: Self = Self(4);
    pub const VANILLA: [Self; 5] = [...];
}
```
- Ids 0..4 are vanilla's `minecraft:fluid` protocol ids; mod fluids get 5.. in `fluids.json` order (still, then flowing, per declaration), equal to `registries.json`'s ids from SPEC-3.
- `id(self) -> u16`, `is_vanilla(self)`, `name(self) -> &'static str` (vanilla names as today; mod names from the installed table, leaked once at install), `from_name(&str) -> Option<Self>` (vanilla, then the table), `same_family(self, other)` (vanilla pairs as today; a mod pair when both map to the same declaration), `source(self)`, `flowing(self)`.
- `ALL` is renamed `VANILLA`; review each `ALL` caller: the ones that build per-kind tables for vanilla data keep `VANILLA`.
- `mask(self) -> u8`: vanilla only (`debug_assert!(self.is_vanilla())`); mod fluids in worldgen fluid predicates are W10's.
- Replace every `kind as u8`/`as u32` with `kind.id()`; add `_` arms where a `match` over the five was exhaustive. Let the compiler list them; change no behaviour in those arms (a mod kind takes the arm `Empty` takes, unless the arm is about naming).

## `FluidTable` (horizon-world `fluid.rs`)

```rust
pub struct FluidTable { fluids: Vec<ModFluid>, by_name: HashMap<String, FluidKind>, states: HashMap<u32, FluidState>, overlay_states: HashSet<u32>, containers: HashMap<u32, Vec<FluidKind>> }
pub struct ModFluid { pub id: String, pub source: FluidKind, pub flowing: FluidKind, pub bucket: Option<String>, pub random: bool, pub water_tagged: bool, pub lava_tagged: bool, pub replaceable: Option<Replaceable>, pub model: Option<FluidModel>, pub overlay_texture: Option<String>, pub fog_code: bool, pub fluid_type: FluidType }
```
- `FluidTable::load(root: &Path, registry: &BlockRegistry) -> io::Result<Self>`: reads `horizon/fluids.json` (absent → empty table); state ids are checked against `registry.len()`; `water_tagged`/`lava_tagged` from the root's `data/minecraft/tags/fluid/water.json` and `lava.json` (resolve `#` entries one level, as other tag readers in horizon-world do; a mod's additions are merged into those files by `mod_content` already: check, and if not, read the bundles the same way blocks' tags are read).
- `install(table)` / `table() -> Arc<FluidTable>`: a process-wide `RwLock<Arc<FluidTable>>`, empty by default; installed in `game.rs` `load_core` right after the block registry is built, before anything builds a `FluidRegistry` or `BlockFacts`. Tests install their own and restore the empty one (a guard type).
- `FluidModel`, `Replaceable`, `FluidType` mirror SPEC-3's JSON (Rust names in `snake_case`); `Replaceable::get(state_index, incoming: FluidKind, direction, above) -> bool` maps `incoming` by name through `replaceable_fluids` (a fluid native has but Java didn't list answers false).
- `pub fn state_fluid(&self, state: BlockStateId) -> Option<FluidState>`, `pub fn overlay(&self, state: BlockStateId) -> bool`, `pub fn container(&self, state: BlockStateId) -> Option<&[FluidKind]>` (a mod `LiquidBlockContainer` state and the fluids it takes; a listed fluid native doesn't have is dropped at load).

## Where a state's fluid comes from

- `FluidState::from_block_id(state: &BlockState, id: BlockStateId) -> io::Result<Self>`: `table().state_fluid(id)` first, else today's name rules. `FluidRegistry::new` calls it. Keep `from_block` for callers without an id (check each; most have one).
- `FluidState::from_json`/`to_json`: mod names through `FluidKind::from_name`/`name`; a mod fluid's `level`/`falling` read and written as vanilla's are.
- `horizon-items` `StateFacts`: new `pub fluid_kind: FluidKind` (exact), and `fluid` (the tag class, used by 240 sites) = `Water` when `water_tagged`, `Lava` when `lava_tagged`, else `None` for a mod state from the table; vanilla states unchanged. `fluid_level`/`fluid_falling` from the table's state.
- `motion/fluids.rs` `is_tagged(kind, tag)`: vanilla as today; a mod kind by its table flags.
- `client_world.rs:101` and any native `MOTION_BLOCKING` heightmap predicate (search `motion_blocking` in horizon-world and horizon-client): vanilla's rule is `blocksMotion || !fluidState.isEmpty()`, so test `fluid_kind != FluidKind::Empty`, not the tag class.

## Ticks and saves

- `java_schedule_fluid` (`level_api.rs`): `FluidKind::from_name` now knows mod fluids; `TickTypeId(kind.id() as u32)`. An unknown name logs once per name: `Horizon: a Java mod scheduled a tick of fluid <name>, which Horizon doesn't have` (the drop stays, but it is no longer silent).
- `save.rs`: chunk `fluid_ticks` entries write `i` with `kind.name()` and read with `from_name`; an unknown name is kept as today's unknown-block rule does (read what it does for blocks and do the same).
- A mod fluid's due tick: `Sim::fluid_tick` returns at once for `!kind.is_vanilla()` (SPEC-5 fills it); random fluid ticks: unchanged (only lava).

## Tests

- `horizon-world` `fluid.rs`: with a test table of one mod pair (`gem:blood`, `gem:flowing_blood`) installed: `from_name`/`name` round trip; `same_family(blood, flowing_blood)` true, `same_family(blood, Water)` false; `FluidState::from_json(r#"{"id":"gem:flowing_blood","properties":{"level":"5","falling":"false"}}"#)` → kind flowing_blood, amount 5; `to_json` round trips.
- `horizon-world`: `FluidRegistry::new` on a two-block registry fixture (vanilla water plus `gem:blood` with a `level` property) and a table with `states` for `gem:blood[level=0]` and `[level=3]`: fluid states are (blood, 8, false) and (flowing_blood, 5, false) — expected from vanilla's legacy-level rule (`amount = 8 - level`), not from the code.
- `horizon-items`: a mod state with a `water_tagged` table fluid has `fluid == Fluid::Water` and `fluid_kind` the mod kind.
- `horizon-client` `save.rs`: a chunk with a scheduled `gem:flowing_blood` tick saves `i: "gem:flowing_blood"` and reads it back with the same trigger tick and priority.
- The SPEC-1 `#[ignore]`d vanilla comparison passes unchanged (proves vanilla derivation untouched; run it if facts are at hand).
- Vanilla regressions: `python3 tools/parity.py fluid_flow` if a harness of that name exists (`parity.py --changed-files paths.txt --list` on the touched files; run at most the two it lists first, through `verify/parity.sh`).

## Done bar

- `cargo test -p horizon-world fluid`, `-p horizon-items blocks`, `-p horizon-client --features java-mods --lib java_ save`; clippy `-D warnings` with and without `--features java-mods` on the touched crates; `pr_ready.py --quick`.
- One live run on `play/install-rs`: after `/setblock X Y Z mekanism:brine`, `horizon-ctl block X Y Z` reports the state, and `horizon-ctl state`'s `looking_at` (aimed at it with `look-at-block`) shows a fluid (whatever field the reply has for fluids); the boot log has no `which Horizon doesn't have` line.

## Live test (lead)

1. `/setblock 0 100 0 mekanism:brine`, `/setblock 2 100 0 mekanism:brine[level=4]`: nothing flows (expected until SPEC-5), nothing draws (until SPEC-7), no errors.
2. Save and quit, reopen: the blocks are still there (`horizon-ctl block`).
