# SPEC-8 (W15): records and `replaces.json`

Read first: `DESIGN.md` here (§4 port rules), SPEC-5, SPEC-6, SPEC-7, `tools/mod_replaces.py` lines 1-36 (the format),
548-619 (`class_listeners`, `split_target`, `mixin_records`, `override_records`) and 664-799 (`analyse`).
Comments in code are one line at most.

## Goal

`analyse` up to (not including) its two last calls (`ef.analyse` and `mrt.routing`): every change the mods make to vanilla
methods and every event they listen to, routed, with the per-mod summary, the data counts and the totals; written as
`replaces.json`.

## Rust, `crates/horizon-java-analysis/src/replaces.rs`

```rust
pub struct Change { pub mod_id: String, pub mechanism: String, pub routing: String, pub note: Option<String>,
    pub kind: Option<String>, pub handler: Option<String>, pub desc: Option<String>, pub at: Vec<String>, pub side: Option<String>,
    pub conditional: Option<String>, pub folded: Option<String>,
    pub classes: Vec<String>, pub classes_n: usize, pub is_abstract: bool, pub extension: bool, pub scope: Option<String> }
pub struct MethodRow { pub changes: Vec<Change>, pub vanilla: bool, pub native: NativeInfo, pub routing: String, pub mods: Vec<String> }
pub struct Listener { pub mod_id: String, pub via: String, pub class: String, pub method: String, pub priority: String, pub receive_canceled: bool, pub bus: Option<String> }
pub struct EventRow { pub cancellable: Option<bool>, pub host_mentions: bool, pub changes: Vec<Listener>, pub routing: String, pub bus: Vec<String>, pub mods: Vec<String> }
pub struct ModClass { pub archive: usize, pub entry: String, pub name: String, pub marked: bool, pub listens: bool }
pub struct Replaces { pub mods: BTreeMap<String, ModSummary>, pub methods: BTreeMap<String, MethodRow>, pub events: BTreeMap<String, EventRow>,
    pub data: BTreeMap<String, BTreeMap<String, DataCount>>, pub totals: BTreeMap<String, usize>,
    pub archives: Vec<Archive>, pub classes: Vec<ModClass>, pub entries: Vec<MixinEntry>, pub hierarchy: Hierarchy }
/// Port of mod_replaces.analyse without its entity facts and routing: SPEC-9 and SPEC-11 run on the result.
pub fn analyse(run: &Path, knowledge: &Knowledge, cache: Option<&Path>, log: &mut dyn FnMut(&str)) -> Result<Replaces, String>
pub fn class_listeners(info: &ClassInfo, hier: &Hierarchy) -> Vec<FoundListener>
pub fn override_records(info: &ClassInfo, mod_id: &str, hier: &Hierarchy) -> BTreeMap<(String, String), (bool, bool)>
fn split_target(target: &str) -> (String, Option<String>)
fn mixin_records(entries: &[MixinEntry], plugins: &BTreeMap<(String, String), String>, hier: &Hierarchy) -> Vec<(String, bool, Change)>
```
`run` is a run folder or an install: the code reads `run/mods/*.jar` and `run/libraries` only. (In a run folder `mods/`
holds the boot and plugin jars too; the Python analysis of `java_mods.py` runs on `install/mods`, which doesn't. Skip the
files named `horizon-java-boot.jar` and `horizon-java-plugin.jar` so both see the same jars.)

Port notes:
- Mod classes: every `.class` entry of every archive in walk order (not `META-INF/`, not `module-info.class`), parsed with
  `members: false`; a failure skips the class. `marked` is a byte search for `addListener`, `addGenericListener` or
  `SubscribeEvent` in the raw bytes; `listens` the first two only.
- Mixin entries come from SPEC-6's `scan_install` (the Python passes `entries` from `scan_install` the same way).
- `mods` is a `defaultdict`: a mod gets a row the first time any code path touches it; `counts` is a `Counter` (missing
  keys are 0 and are not written unless incremented). `summary` keeps only routings with at least one key, in `ROUTING_ORDER`.
- `overrides` and `listeners` are dicts keyed by tuples, filled in class order; their final iteration order is insertion
  order. `methods[key]['changes']` order is: mixin records (after `fold_lambdas`), then overrides, in that insertion order.
- `events[event]['bus']` is `sorted({c['bus'] for c in changes if c['bus']})`; a listener's `bus` is the first of its sorted
  non-empty bus set, else null.
- `host_mentions`: the event's simple name before the first `$` is in `knowledge.host_words`.
- The `native lacks it` note rule and `top_routing` exactly as written.
- JSON field names are Python's (`mod`, not `mod_id`; `abstract`, not `is_abstract`): serde `rename`. A `Change` writes only
  the fields its mechanism has in Python (mixin/access: `mod, mechanism, kind, handler, desc, at, side, conditional`
  (+`folded`), override/implement: `mod, mechanism, desc, classes, classes_n, abstract, extension, scope`), then `routing`
  and `note` when set: use `skip_serializing_if` per field so the parsed JSON equals Python's.

## Writing

`pub fn write_replaces(out: &Path, r: &Replaces, install: &Path, seconds: f64)`: `out/replaces.json` with
`format, install, mods, methods, events, data, totals, seconds` (indent 1).

## CLI and comparison

- `horizon-java-analysis replaces --install DIR --boot-jar JAR --out OUT [--cache C]` (also writes `mixins/`).
- `java_analysis_compare.py replaces --install DIR`: the Python `mod_replaces.run` into one temp folder (ledger and
  verdicts as today), the Rust command into another with the boot jar built by SPEC-7's `java_mods.py build`; compare
  `replaces.json` without `seconds` and `install`.

## Tests (port of the record parts of `tests/test_mod_replaces.py`, same Java sources, skipped without `JAVA_HOME`)

Build the same fixture install as the Python test (libraries jar with the `net/minecraft`/NeoForge stubs, a mod jar with
`fixture/*`, a nested jar with `inner/Deep`), a boot jar whose `analysis/` resources encode the test's `LEDGER`, and assert
the Python test's own expected values, ported literally: e.g. `methods["net/minecraft/world/Base#tick"]` has an
`override` change from `fixture.One` routed `java-own-type` and one from `inner.Deep`; `Base#must` changes are `implement`
routed `java-already`; `events` has `PlainEvent` with listeners `subscribe` (priority HIGH, receive_canceled true) and
`addListener` (via the `Class` argument), `CancelEvent` cancellable, `ModThing` on the `mod` bus; `Sub`'s listener has
via `subscriber-class` and bus `mod`. Read the Python test and port each assertion that does not concern routing.json or
entity facts.

## Checks

`cargo test -p horizon-java-analysis`, clippy, fmt.

## Done bar

`java_analysis_compare.py replaces`: **0 differences** on both finish installs.

## Depends on

SPEC-6, SPEC-7.

## Live test for the lead

None.
