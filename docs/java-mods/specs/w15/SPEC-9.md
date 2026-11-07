# SPEC-9 (W15): routing: `routing.json`, `routing.tsv`, `replaces.md`

Read first: `DESIGN.md` here (D8, §4 port rules), SPEC-7, SPEC-8, `tools/mod_routing.py` lines 1-41 (constants) and
138-407, `tools/mod_replaces.py` lines 801-848 (`table`, `short`, `markdown`, `write_routing`), and W12's
`/home/umceko/hz-poc/agents/w12/DESIGN.md` §2 for what the boot does with a route. Comments in code are one line at most.

## Goal

The routes the class processor (`routing.tsv`) and `Routing.confirm` (`routing.json`) read: for each vanilla method a
mod's mixin changes and native owns, its kind (edge or body), family, arguments, the changes, return count, `TAIL`,
overriders, the callee closure of a body route, hotness, side and verdict. Plus `replaces.md`, which needs them.

## Data files (D8), `crates/horizon-java-analysis/data/`, read with `include_str!`

- `routing_roots.tsv`: `hot \t key` rows for `HOT_ROOTS`, `view \t class` rows for `VIEW_ROOTS`.
- `families.tsv`: `family \t root class` rows in `FAMILIES` order (a family with two roots has two rows, kept in order).
Parse them once (`LazyLock`). `EDGE_AT`, `CLOSURE_CAP`, `HOT_DEPTH`, `LAMBDA_FACTORY`, the opcode numbers stay constants.

## Rust, `crates/horizon-java-analysis/src/routing.rs`

Port, one function each, named after the Python: `desc_parts`, `Graph` (`callees`, built on SPEC-7's `Vanilla.ops`, each
call resolved with `Hierarchy::declarer`), `key_of`, `split_key`, `hot_reach`, `subclass_index`, `overriders`, `family`,
`closure`, `handlers`, `callback_split`, `annotation` (the routing module's helper), `change_row`, `targets_of`,
`routing`, `tsv`, `routes_tsv`, `summary`.
```rust
pub type Bridges = BTreeSet<(String, String, String)>;
pub struct Report { pub routes: BTreeMap<String, Route>, pub undecided: BTreeMap<String, Undecided>, pub lambdas: BTreeMap<String, String> }
/// Port of mod_routing.routing.
pub fn routing(r: &Replaces, vanilla: &Vanilla, knowledge: &Knowledge, bridges: &Bridges) -> Report
```
Port notes:
- `bridges` is an argument (Python's `bridge_table()` regex over `game/java_bridges/*.rs`): the game passes its compiled
  table (SPEC-12); the CLI reads `--bridges FILE` (`owner \t name \t desc` lines).
- `analyse` passes `routing` its unsorted `methods`, but every output map is sorted at the end and one `full` key gathers
  changes from one method row only, so the order of rows doesn't reach the output; the order of changes inside a route
  is the row's order (mixin records, then overrides) and must be kept.
- `side`: `sides.pop()` only when exactly one side and it is `client` or `server`, else `common`.
- `closure` stops at `CLOSURE_CAP` walked callees and marks `truncated`; it walks breadth first with a `deque`; the sets
  are written sorted.
- `hot`: `k in reach or (kind == 'edge' and owner in reach_owners)`.
- `verdict` is written only for weak statuses (`WEAK_STATUSES`) with a verdict.
- `handlers` reads a mixin class's handler method from the mod archives (cache by `(archive, class)`), as Python does.

## Writing

- `pub fn write_routing(out: &Path, report: &Report)`: `routing.json` (`format: 1, routes, undecided, lambdas`, indent 1)
  and `routing.tsv` (`tsv`, the class processor's columns: owner, name, descriptor, kind, tail 0/1, returns; a trailing
  newline only when there are lines).
- `pub fn markdown(r: &Replaces, report: &Report) -> String`: port of `markdown` with `table` and `short`; written as
  `replaces.md` beside `replaces.json`.

## CLI and comparison

- `horizon-java-analysis replaces ... --bridges FILE` now also writes `routing.json`, `routing.tsv`, `replaces.md`
  (`--routes-out FILE` writes `routes_tsv` as Python's `--routes-out` does).
- `java_analysis_compare.py routing --install DIR`: writes `mrt.bridge_table()` to a temp TSV, runs both, compares
  `routing.json` as parsed JSON and `routing.tsv`, `replaces.md` as text.

## Tests (port of `tests/test_mod_routing.py`, same Java fixtures, skipped without `JAVA_HOME`)

Port every assertion of `tests/test_mod_routing.py` with its hand-written expected values (kinds edge/body by `@At`, a
`TAIL` route, a static route's `family`, `overriders` of a subclass override, a folded lambda in `lambdas`, the closure
stopping at a bridged and at a routed callee, `hot` for a route reached from a `HOT_ROOTS` key within `HOT_DEPTH`). Plus:
`tsv` of a two-route report equals the hand-written two lines.

## Checks

`cargo test -p horizon-java-analysis`, clippy, fmt.

## Done bar

`java_analysis_compare.py routing`: **0 differences** on both finish installs. The `routing.tsv` of the showcase install
from Rust is byte-identical to Python's.

## Depends on

SPEC-8.

## Live test for the lead

None yet (SPEC-12 switches the game to it).
