# SPEC-11 (W15): entity facts II: writes, capabilities, hooks, `entity-facts.json`

Read first: `DESIGN.md` here (D8, §4 port rules), SPEC-10, `tools/mod_entity_facts.py` lines 300-665 (`entity_receiver`
.. `analyse`, `HOOKS`, `NEW`, `FOLLOWED`, `NOT_FOLLOWED`). Comments in code are one line at most.

## Goal

The rest of `mod_entity_facts.py`, so the Rust analysis writes the `entity-facts.json` (format 2) the boot's
`EntityFacts.confirm` reads: events with listeners and subjects, the state groups read through each receiver class, the
vanilla entity writers mod code and NeoForge's hooks call, the entity routes, the overrides, capabilities, and the
`opaque`/`unclassified`/`unresolved_listeners`/`reflective_entity_access` lists.

## Data file (D8)

`crates/horizon-java-analysis/data/entity_hooks.tsv`: `hook \t line` rows for `HOOKS` in its order (the `line` is the
NeoForge method key the closure starts from). A `HOOKS` line the NeoForge jar doesn't have goes to `hooks_missing`, as
Python does.

## Rust, `crates/horizon-java-analysis/src/entity_facts/facts.rs`

Port, each named after the Python: `entity_receiver`, `entity_key`, `producers_of_call`, `capability_registrations`,
`library_target`, `handler_method`, `route_facts`, `hook_closure`, `loader_archive`, `event_classes`, `inherit`, `analyse`.
```rust
/// Port of mod_entity_facts.analyse: entity-facts.json for the classes and mixin entries of SPEC-8's result.
pub fn analyse(r: &Replaces) -> Result<serde_json::Value, String>
```
Port notes:
- `loader_archive` appends the NeoForge loader's archive after the mods' and its classes as listening-only (`ai >= mods_n`);
  keep that index rule.
- The pre-filter before parsing a class (`marked`, an entity class, the raw bytes holding `net/minecraft/world/entity/` or
  the `RegisterCapabilitiesEvent` name) is a byte search on the raw class, as Python's `in raw`.
- `writers` is keyed `(key, receiver, group)` and its mods are a set (written sorted); NeoForge's hooks add the mod name
  `neoforge`.
- `event_supers` lists, for each candidate event class with a candidate ancestor, its ancestors in `Hierarchy::ancestors`
  order filtered to candidates.
- `unresolved_listeners` adds SPEC-8's per-mod `unresolved_listeners` counts (`Counter.update` adds).
- The result is a JSON value with Python's key names and sorting (`dict(sorted(...))` for maps, `sorted` lists).
- `unclassified` is returned in the value; `analyse` does not fail on it. The callers fail (SPEC-12 for the game, the CLI
  with exit code 1 and the error text of SPEC-10), as `install_analysis` and `run` do.

## Writing and CLI

- `horizon-java-analysis replaces ...` now also writes `entity-facts.json` (indent 1), and exits 1 with the SPEC-10 message
  when `unclassified` is not empty, **after** writing the files (as Python's `run`).
- `pub fn write_boot_files(out: &Path, report: &Report, facts: &Value)`: `entity-facts.json`, `routing.json`, `routing.tsv`:
  what the boot reads. SPEC-12 uses it.
- `java_analysis_compare.py entity-facts --install DIR`: compares `entity-facts.json`.
- `java_analysis_compare.py all --install DIR`: runs every stage (classes, libraries, jars, mixins, knowledge, replaces,
  routing, entity-guards, entity-facts) and prints one line per stage.

## Tests (port of the rest of `tests/test_mod_entity_facts.py`, skipped without `JAVA_HOME`)

Port each remaining assertion with its hand-written expected values: a mod method calling `Entity#setHealth` on a
`LivingEntity` receiver gives a `writes` row `health` and `reads` of `health` for that receiver; `getData` is `java`
(not carried); a capability registered for `EntityTypes.ZOMBIE` lands in `caps["minecraft:zombie"]`; a registration whose
type is not a static field counts in `caps_unresolved`; a mixin `@Inject(at = @At("HEAD"))` into `Entity#tick` gives a
`routes[...]["edge"]` row; a jar with a coremod is listed in `opaque`; an uncategorised entity method appears in
`unclassified` with its mod.

## Checks

`cargo test -p horizon-java-analysis`, clippy, fmt.

## Done bar

`java_analysis_compare.py all`: **0 differences** in every stage on both finish installs. Paste the two outputs.

## Depends on

SPEC-10.

## Live test for the lead

None (SPEC-12).
