# SPEC-7 (W15): Horizon's knowledge in the boot jar; vanilla lambdas folded

Read first: `DESIGN.md` here (D7, §4 port rules), SPEC-4, SPEC-5, `tools/mod_replaces.py` lines 393-458 (`Native`,
`read_verdicts`, `verdict_of`, `route`), 517-547 (`lambda_event`, `lambda_impl`), 620 (`top_routing`), 660
(`host_words`); `tools/mod_routing.py` lines 42-137 (`Vanilla`, `lambda_target`, `enclosing`, `fold_lambdas`);
`tools/mod_parity_explain.py` `host_lines` (line 217). Comments in code are one line at most.

## Goal

The analysis needs three things only Horizon's tree knows. They ship inside `horizon-java-boot.jar`, which every run
folder links in `mods/`, so the game and the CLI read them from the run folder alone:

| Resource | Made from | Rows |
|---|---|---|
| `analysis/native.tsv` | `.local/port-ledger/ledger.json` through `mod_replaces.Native(ledger).info(key)` | `key \t status \t owned(0/1) \t fns(comma-joined, up to 3) \t note` for every key of every `STATUS_ORDER` table and of `methods` |
| `analysis/verdicts.tsv` | `tools/port_map/*.tsv` through `read_verdicts()` | `key \t verdict` |
| `analysis/host_words.txt` | `mod_replaces.host_words()` (the host's Java sources) | one word per line, sorted |

Text fields escape `\` as `\\`, tab as `\t`, newline as `\n`. A key absent from `native.tsv` is `unknown`
(`Native.info`'s last line), not an error.

Then the vanilla-side lambda folding the records need.

## Python (`tools/java_mods.py` `build`)

- New `tools/java_knowledge.py` (dev tool, kept after SPEC-13): `write(out_dir)` writes the three files with the rules above,
  using `mod_replaces.Native`, `read_verdicts` and `host_words` (moved to `tools/port_native.py` in SPEC-13).
- In `build`, for the `boot` jar: `knowledge = JARS / 'classes' / 'boot-knowledge'`, emptied, `java_knowledge.write(knowledge / 'analysis')`,
  then `jar_args += ['-C', str(knowledge), '.']`. When `mod_replaces.LEDGER_JSON` is missing, stop with
  `sys.exit('no port ledger: run python3 tools/port_ledger.py build first (the boot jar carries what it says)')`. No
  empty-ledger default anywhere: delete the `else {}` in `install_analysis` and `mods_analysis` too (they exit the same way).

## Rust, `crates/horizon-java-analysis/src/`

### `knowledge.rs`
```rust
pub struct NativeInfo { pub status: String, pub owned: bool, pub fns: Vec<String>, pub note: String }
pub struct Knowledge { native: HashMap<String, NativeInfo>, verdicts: HashMap<String, String>, pub host_words: HashSet<String> }
impl Knowledge {
    /// The three resources of the boot jar; an error names the one missing.
    pub fn from_boot_jar(jar: &Path) -> Result<Knowledge, String>
    /// Port of mod_replaces.Native.info.
    pub fn info(&self, key: &str) -> NativeInfo
    /// Port of mod_replaces.verdict_of.
    pub fn verdict_of(&self, key: &str) -> Option<&str>
}
/// Port of mod_replaces.route.
pub fn route(mechanism: &str, native: &NativeInfo, key: &str, k: &Knowledge) -> &'static str
/// Port of mod_replaces.top_routing.
pub fn top_routing(routes: &[&str]) -> &'static str
```
`Native.info` returns `'none'` for a key in `methods` without a status; `native.tsv` carries those rows with status `none`,
so `info` is a lookup with `unknown` as the miss. The error for a missing resource:
`"{jar} has no analysis/native.tsv: build the host jars again (java_mods.py build)"`.

### `lambdas.rs`
Port of `lambda_event`, `lambda_impl` (mod_replaces) and `Vanilla` (`info`, `method`, `ops`), `lambda_target`, `enclosing`,
`fold_lambdas` (mod_routing). `Vanilla` indexes vanilla class entries of the library jars (first jar wins, as
`index.setdefault`), parses lazily with `Parse { members, annotations, code: true }` and memoises (`RefCell` maps).
`fold_lambdas` takes `(key, is_vanilla, record)` triples where `record` is SPEC-8's change record; define here the minimal
`pub struct Change { pub desc: Option<String>, pub folded: Option<String>, .. }` fields it touches, and let SPEC-8 complete
the struct.

## CLI and comparison

- `horizon-java-analysis dump-knowledge --boot-jar JAR KEY...`: `info`, `verdict_of` and `route` for `mixin`, `override`,
  `implement` per key.
- `horizon-java-analysis dump-folds --install DIR KEY...`: for each `owner#lambda$…` key, `enclosing`'s answer.
- `java_analysis_compare.py knowledge --install DIR`: takes every `methods` key from the Python `replaces.json` of the install
  (run the Python analysis once into a temp folder) and compares Python's `Native.info`, `verdict_of`, `route` with
  the Rust dump; then every key of the Python report whose change has a `folded` field, compared with `dump-folds`.

## Tests (hand-written expected values)

- `knowledge.rs` from an in-memory jar with three small resources mirroring `tests/test_mod_replaces.py`'s `LEDGER`:
  `Base#tick` → `ported`, owned, `["crates/a.rs:7"]`; `Base#must` → `absent`, not owned, note `nothing`; `Base#other` →
  `none`; `Base#zzz` → `unknown`. `route("mixin", info(tick))` → `java`; `route("mixin", info(must))` with verdict
  `missing` → `java-already`, with `port` → `java`, without a verdict → `undecided`; `route("override", owned, key under
  net/minecraft/client/gui/)` → `java-already`; `route("access", ..)` → `inert`.
- Escapes: a note `a\tb\nc\\d` survives the round trip.
- `lambdas.rs` with a `javac` fixture (skipped without `JAVA_HOME`): `class V { void outer() { Runnable r = () -> { Runnable s = () -> inner(); }; } void inner() {} }`
  in package `net/minecraft/x`: `enclosing(V, "lambda$outer$1", None)` → `("outer", "()V")` through the nested lambda
  (javac names them `lambda$outer$0`/`lambda$outer$1`: read the names from the parsed class in the test, then assert the
  enclosing answer is `outer ()V` for both).

## Checks

`cargo test -p horizon-java-analysis`, clippy, fmt; `python3 tools/java_mods.py build` with a private
`HORIZON_JAVA_MODS_DIR`, then `unzip -l horizon-java-boot.jar | grep analysis/` shows the three files.

## Done bar

`java_analysis_compare.py knowledge`: **0 differences** on both finish installs.

## Depends on

SPEC-5 (library jars for `Vanilla`). Independent of SPEC-6.

## Live test for the lead

None. Check that a boot of install-rs with the new boot jar still says `Java mods ready` (the jar only gains resources).
