# SPEC-12 (W15): the game analyses the mods before the JVM starts

Read first: `CLAUDE.md`, `/home/umceko/hz-poc/agents/RULES.md`, `DESIGN.md` here (G5, D5), SPEC-1, SPEC-2, SPEC-11,
`crates/horizon-java-host/src/lib.rs` (`boot`, `create_vm`, `start`), `crates/horizon-client/src/game/java_content.rs`
(the content cache it must not collide with). Comments in code are one line at most.

## Goal

Every Java mods start (window, LAN host, dedicated server; launcher, `java_mods.py` or `horizon-server`) has the mods'
analysis, made by the game itself before the JVM starts and cached, and hands it to the boot with
`-Dhorizon.java.analysis`. A launcher install then boots with the same routes and entity facts as a `java_mods.py` one.

Today: only `java_mods.py install` writes an `analysis` line; the launcher's installs boot with no routes
(`Routing.confirm`, `BridgeProcessor.readRouting` and `EntityFacts.confirm` return empty without the property).

## Rust, `crates/horizon-java-host/`

- `Cargo.toml`: depend on `horizon-java-analysis`.
- New `src/analysis.rs`:
  ```rust
  /// Bumped when the analysis writes something else from the same jars.
  pub const GENERATION: u32 = 1;
  /// The folder holding this run's analysis (`entity-facts.json`, `routing.json`, `routing.tsv`), made when not cached.
  pub fn prepare(run: &Path, bridges: &Bridges) -> Result<PathBuf, String>
  fn fingerprint(run: &Path, libraries: &[PathBuf], bridges: &Bridges) -> Result<String, String>
  fn cached_or(root: &Path, key: &str, make: impl FnOnce(&Path) -> Result<(), String>) -> Result<PathBuf, String>
  ```
  - `root` = `current_dir()/.horizon-cache/java-analysis` (**not** `.horizon-cache/java-mods`: `java_content::content`
    deletes every folder there but its own key, which would redo the analysis at every launch).
  - `fingerprint`: SHA-256 over `GENERATION`, then for each `run/mods/*.jar` sorted by file name: name, size, mtime in ns
    (the boot jar included: it carries the knowledge), then each library jar of `horizon_java_analysis::library_jars`
    with its size and mtime, then the bridges sorted. Hex, first 20 characters.
  - `cached_or(root, key, make)`: `root/key/done` exists → that folder. Else `make(root/key.partial)` (emptied first), write
    `done`, rename to `root/key` (remove a stale `root/key` first), then delete every other entry of `root` except
    `libraries/`. A failing `make` leaves no `done`, so the next start tries again.
  - `make`: `Knowledge::from_boot_jar(run/mods/horizon-java-boot.jar)`, `replaces::analyse(run, &knowledge,
    Some(root/libraries), log)`, `routing::routing`, `entity_facts::analyse`; if `unclassified` is not empty, `Err` with the
    SPEC-10 message and the list; else `write_boot_files`.
  - Log (stdout, as the boot's other lines): `Horizon: Java mods analysed in {s:.1}s: {routes} routes, {undecided} undecided`
    or `Horizon: Java mods' analysis cached ({key})`.
- `lib.rs` `boot()`: first step, before `create_vm`:
  `let analysis = analysis::prepare(Path::new(run_dir), &table_keys())?;` where `table_keys()` maps `bridge::TABLE`'s
  entries to `(class, method, desc)`. Pass `analysis.join("entity-facts.json")` to `create_vm`, which adds
  `-Dhorizon.java.analysis=<path>` after the args file's options and before `HORIZON_JAVA_OPTS`.
- `RunArgs::parse` (SPEC-1): drop the `analysis` line case; such a line in an old file is ignored like any unknown kind.
  `java_mods.py install` rewrites it at the next install.

## Dev tool (`tools/java_mods.py`)

- `args_lines`: remove the `analysis` parameter and line; `write_args` and `install_pack` stop computing it
  (`mods_analysis`, `install_analysis` are no longer called from them). `java_mods.py replaces` keeps working (Python until
  SPEC-13).
- `check --analysis FILE` is unchanged: point it at `HORIZON_HOME/.horizon-cache/java-analysis/<key>/entity-facts.json` or at
  a `replaces` output.
- Update `tests/test_java_mods_install.py` (the `analysis=` test goes; assert no `analysis` line).

## Docs

`docs/java-mods.md` "Blocks and items from the boot": one bullet: before the JVM starts, the game reads the mods' jars
(mixins, overrides, listeners, entity calls) and writes the routes and entity facts the boot reads, cached under
`.horizon-cache/java-analysis/` by the jars and Horizon's own tables; the analysis's own knowledge of what Horizon ports
comes from the boot jar.

## Tests

- `analysis.rs` unit tests with an injected `make` (no jars needed):
  - first `cached_or` runs `make` once and returns `root/key` holding `done`; a second call with the same key doesn't run it;
  - a new key runs `make` and deletes the old key's folder, keeping `libraries/`;
  - a `make` that fails leaves `root/key` absent and the next call runs `make` again.
- `fingerprint`: changing a mod jar's size, adding a jar, or adding a bridge gives another key; reordering the bridges does not.

## Checks

`cargo test -p horizon-java-host`, clippy on `horizon-java-host` and `horizon-client --features java-mods`, the build;
one `hz_scenario.py` run with private jars on install-rs: the log has `Java mods analysed` then `routes: N on`.

## Done bar

- install-rs started through `--java-mods` (no `analysis` line in its args file): the boot's `Horizon: routes: …` line
  shows the same number of routes as before this spec (20 for install-rs, 177 for the showcase install, STATUS §0), and
  `java_mods.py check --analysis <cache>/entity-facts.json` reports the same problem count as with the Python analysis
  (0 / 35 after STATUS step 1, else the counts of the day).
- A second start says `analysis cached`.

## Depends on

SPEC-1 (`RunArgs`), SPEC-2 (the run folder), SPEC-11 (the full analysis, compared equal).

## Live test for the lead (scratch homes)

1. A launcher install of the showcase pack (`horizon-launcher java-mods pack /home/umceko/hz-poc/agents/w12/install/mods`,
   `java-mods install`, `java-mods enable`), then Play (offscreen via the launcher CLI). Expect in the game log:
   `Java mods analysed in …: 177 routes` (or the day's count) and then `Horizon: routes: …`; before this spec a launcher
   install printed no routes line at all.
2. With Carry On in that pack: pick up a chest with shift + right click (W12 SPEC-5's gated route): it works as on a
   `java_mods.py` install. This needs real input: give the user the steps.
3. Start again: `analysis cached`, and the time to the title screen is not longer than before this spec.
