# SPEC-13 (W15): retire the Python analysis; one implementation

Read first: `DESIGN.md` here (D6), SPEC-6..12. Comments in code are one line at most.

## Goal

After SPEC-12 the game runs the Rust analysis. The developers' tools switch to the same code (the CLI), and the Python
analysis, its `javac` scanner path and the comparison tool are deleted, so routes can't drift between what players get and
what reports show. What stays in Python reads the ledger for dev lints and builds the boot jar's knowledge.

## Keep, moved

New `tools/port_native.py` holding, unchanged, from `mod_replaces.py`: `ROOT`, `LEDGER_JSON`, `PORT_MAP`, `STATUS_ORDER`,
`OWNED`, `WEAK_STATUSES`, `ROUTING_ORDER`, `Native`, `read_verdicts`, `verdict_of`, `host_words` (with
`mod_parity_explain.host_lines`). Point every importer at it: `tools/java_routing.py` (line ~255), `tools/java_sound_sources.py`
(line ~20), `tools/java_knowledge.py` (SPEC-7).

## Switch to the CLI (`tools/java_mods.py`)

- `build`: after the jars, `cargo build --profile fast -q -p horizon-java-analysis` with the same `env` as the host's build;
  `ANALYSIS_BIN = TARGET / 'fast' / 'horizon-java-analysis'`.
- New helper `analysis(install, out, routes_out=None)`: writes the bridge table (`BRIDGE_ENTRY` over `BRIDGES_DIR`, the regex
  `check` already uses) to `out/bridges.tsv` and runs `ANALYSIS_BIN replaces --install INSTALL --boot-jar BOOT_JAR
  --bridges out/bridges.tsv --out OUT --cache JARS/mod-replaces [--routes-out F]`; returns its exit code (1 = unclassified
  entity methods, already printed).
- The `replaces` subcommand calls it (same arguments as today's `mod_replaces.add_arguments`: `--install`, `--out`,
  `--cache`, `--routes-out`; `--ledger` and `--mixins` go: the ledger is in the boot jar, the mixins are scanned by the CLI).
- The `mixins` subcommand runs `ANALYSIS_BIN mixins --install DIR --out OUT`.
- `mod_parity.scan_install(install, out)` becomes: run `ANALYSIS_BIN mixins`, return the parsed `mixins.json` (callers
  `mod_parity.run_command` and `port_triage.py` keep their call). Delete `asm_jars`, `compile_scanner`, `parse_scan` when
  nothing else uses them (`rg` each); keep `java_of` if other code uses it.
- `mods_analysis` and `mod_replaces.install_analysis` are deleted (unused since SPEC-12).

## Delete

- `tools/mod_replaces.py` (after the move), `tools/mod_routing.py`, `tools/mod_entity_facts.py`,
  `tools/java_analysis_compare.py`.
- `tests/test_mod_replaces.py`, `tests/test_mod_routing.py`, `tests/test_mod_entity_facts.py`: their assertions were ported
  in SPEC-8..11; before deleting, list each Python test method and the Rust test that holds its assertions, in the report.
  Any not yet ported is ported now.
- `tests/test_animate_routing.py` with `animate_classes`/`animate_routing`: port both functions into
  `crates/horizon-java-analysis/src/routing.rs` (`animate_closure.tsv` is read from the boot jar's resources, where the
  host already ships it under `res/`) with the test's four cases and hand-written expected values, then delete the Python.
- In `tests/test_mod_parity.py`, the case at line ~227 that compiles the scanner: keep it, now through the CLI (skip when the
  CLI isn't built, as it skips without `javac` today).
- **Not** `MixinScanner.java`: the boot calls it at runtime (`SoundOwners`, `Animate`). Only the analysis stops compiling it.

## Docs

`docs/java-mods-map.md`: the analysis rows point at `crates/horizon-java-analysis/` and its CLI; "add it to GROUPS"
becomes `crates/horizon-java-analysis/data/entity_groups.tsv`. `docs/java-mods.md`: `java_mods.py replaces` writes the
same report as before, made by the game's own analysis.

## Checks

`python3 -m unittest discover -s tests` for the touched tests (`test_java_mods_install`, `test_mod_parity`,
`test_java_routing*`), `cargo test -p horizon-java-analysis`, `python3 tools/java_routing.py report` and `lint` on both
installs (the same counts as before), `python3 tools/java_mods.py check --analysis …` (same counts).

## Done bar

- `rg -l "mod_replaces|mod_routing|mod_entity_facts|java_analysis_compare" tools tests docs` finds nothing.
- `java_mods.py replaces --install /home/umceko/hz-poc/agents/w12/install --out OUT`: the `routing.tsv` equals the one the
  game's cache holds for the same install, byte for byte.
- `java_routing.py report` on both installs prints the STATUS §0 numbers of the day (20 / 177 routes etc.).

## Depends on

SPEC-12.

## Live test for the lead

None beyond the checks: this changes dev tools only.
