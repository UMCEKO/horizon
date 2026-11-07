# SPEC-2 (W15): the game decides: `--java-mods DIR`, one run folder, off is inert

Read first: `CLAUDE.md`, `/home/umceko/hz-poc/agents/RULES.md`, `docs/java-mods.md`, `DESIGN.md` here (G2, D1, D4), SPEC-1.
Comments in code are one line at most, and only for a non-obvious why.

## Goal

- The game has Java mods when it is given a run folder, by `--java-mods DIR` (the launcher's and the server's way) or by
  `HORIZON_JAVA_RUN` (the developers' tools; the same meaning), **and** that folder's `mods/` holds at least one jar that
  is not Horizon's own (`horizon-java-boot.jar`, `horizon-java-plugin.jar`). Otherwise nothing of Java runs.
- Everything that read `HORIZON_JAVA_RUN` reads one function.
- With no Java mods, a feature build behaves as a build without the feature: no JVM thread, no hooks installed, the
  `horizon:java` probe answered 0, no `.horizon-cache/java-mods` folder written.
- A dedicated server checks its EULA before the JVM starts.
- A build without the feature given `--java-mods` stops with a usage error (exit code 2), never runs without the mods.

## Rust: `crates/horizon-java-host/src/lib.rs`

```rust
static RUN: OnceLock<Option<PathBuf>> = OnceLock::new();

/// The run folder this process's Java mods start from; set once, at startup, before `start`.
pub fn set_run(dir: Option<PathBuf>)
/// The Java mods' run folder, when this process has Java mods.
pub fn run_dir() -> Option<&'static Path>
/// Whether this process has Java mods.
pub fn configured() -> bool { run_dir().is_some() }
/// Whether `run`'s `mods/` holds a jar that isn't Horizon's own.
pub fn has_mods(run: &Path) -> bool
/// The run folder of a launch: `--java-mods` wins over `HORIZON_JAVA_RUN`; a relative path is taken from `cwd`.
pub fn resolve_run(flag: Option<&Path>, env: Option<&OsStr>, cwd: &Path) -> Option<PathBuf>
```
- `run_dir()` reads `RUN.get()`; when `set_run` was never called (dev binaries, examples), it is `None` except in
  `horizon-java-host/src/main.rs`, which calls `set_run(resolve_run(None, env, cwd))` itself.
- `RUNNING` (line 289): `let Some(run_dir) = run_dir() else { return Boot::Off };` instead of the env read.
- `has_mods`: `read_dir(run.join("mods"))`, any entry whose name ends in `.jar` and is not one of the two names above.
  A missing folder is false.
- A marker the launcher looks for in the program (replaces `FEATURE_MARK = b"HORIZON_JAVA_HOST_JAR"`, which SPEC-1 removed
  from the program's text):
  ```rust
  #[used]
  pub static SUPPORT_MARK: [u8; 25] = *b"horizon-java-mods-support";
  ```

## Rust: the game (`crates/horizon-client/src/`)

1. `game.rs` `Options`: field `java_mods: Option<PathBuf>`. In `parse`, beside `--dev-mod`:
   `"--java-mods" => o.java_mods = Some(std::path::absolute(&value)?)`. Add to `--help`:
   `  --java-mods DIR          Run the Java mods of this run folder (a build with Java mods support)`.
   Not in `game/launch_api.rs` (DESIGN open question 1).
2. Without the feature (`#[cfg(not(feature = "java-mods"))]`), in `Options::read` right after `parse`: if `o.java_mods`
   is set or `HORIZON_JAVA_RUN` is set, return
   `launch_api::UsageError("this Horizon was built without Java mods support; use a build with them".into())`.
3. `run_with` (line 22477), feature arm, replacing the block at 22481-22485:
   ```rust
   let run = horizon_java_host::resolve_run(options.java_mods.as_deref(), std::env::var_os("HORIZON_JAVA_RUN").as_deref(), &std::env::current_dir()?)
       .filter(|run| { let has = horizon_java_host::has_mods(run); if !has { log::info!(...) } has });
   horizon_java_host::set_run(run);
   ```
   The log line: `Horizon: no Java mods in {run}/mods: starting without Java`. Move the
   `java_content::start(...)` / `java_components::install()` block **after** the dedicated `preflight` check, and run it
   only `if horizon_java_host::configured()`.
4. Replace every environment read with the function:
   - `game/java_content.rs:53` `start`: `if let Some(run) = horizon_java_host::run_dir()`.
   - `game/java_content.rs:64` `carried`: `let Some(run) = horizon_java_host::run_dir() else { return Vec::new() };`.
   - `game/java_components.rs:143` `install`: the whole function body runs only when `configured()` (the two `net` hooks
     included). Since `run_with` already guards the call, make it an early `return` so a future caller can't install them.
   - `game/java_screens.rs:1360` `java_client_font`: `!horizon_java_host::configured()`.
   After this, `rg 'HORIZON_JAVA_RUN' crates/horizon-client` finds only `run_with`.
5. Nothing else changes in `net.rs`: with the hook unset, `net.rs:1000` already answers 0.

## Launcher (`launcher/`)

- `java_mods/args.rs` `launch_env`: drop `HORIZON_JAVA_RUN`.
- `java_mods.rs` `apply(cmd, tag, dedicated, run)`: after `prepare_run`, `cmd.arg("--java-mods").arg(&run)`. Every caller
  adds its own arguments after `apply`; check `main.rs:1223`, `cli.rs:1585` and the `java-mods launch` command in `cli.rs`
  (near line 1849): `--java-mods` must come before `--dedicated` (the game hands every argument after `--dedicated` to the
  server's own parser). Move `apply` before `--dedicated` is added where it isn't.
- `FEATURE_MARK = b"horizon-java-mods-support"`.

## Dev tool (`tools/java_mods.py`)

`launch`: pass `--java-mods RUN` before the game arguments instead of `HORIZON_JAVA_RUN` in `launch_env`. `live.py` and the
other dev scripts keep setting `HORIZON_JAVA_RUN`: same meaning, nothing to change.

## Docs

`docs/java-mods.md`: in "Playing with Java mods", `launch` passes `--java-mods`; delete the last "Not yet" bullet ("The game
finds NeoForge through dev variables…"); add one line under "What exists": the game runs Java mods given `--java-mods DIR`
(or `HORIZON_JAVA_RUN`) with a mod jar in `DIR/mods`, and starts no JVM otherwise.

## Tests

- `horizon-java-host` unit tests:
  - `has_mods` on temp folders: no `mods/` → false; only `horizon-java-boot.jar` and `horizon-java-plugin.jar` → false;
    plus `jei.jar` → true; a `notes.txt` beside the two → false.
  - `resolve_run`: flag `Some("/a")`, env `"/b"` → `/a`; flag none, env `"b"`, cwd `/w` → `/w/b`; both none → None.
- `crates/horizon-client` gameplay test (`cargo test -p horizon-client --features java-mods --lib java_components`):
  `install()` in a process where `set_run(None)` was called leaves `horizon_client::net::java_link_installed()` false. Add
  that one-line accessor to `net.rs` (`JAVA_LINK.get().is_some()`). If the test binary shares `RUN` with other tests, call
  `set_run(None)` at the test's start (first set wins; no other gameplay test sets it).
- `launcher` tests: `apply` puts `--java-mods <run>` in the command's args; `launch_env` has no `HORIZON_JAVA_RUN`;
  `has_feature` finds the new mark in a file holding it and not in one without it (update the existing test's bytes).
- Without the feature: `cargo test -p horizon-client --lib options` (or where option parsing is tested): `--java-mods x`
  gives the usage error text above.

## Checks

The unit tests above; clippy `-D warnings` on `horizon-java-host` and `horizon-client` with and without `--features java-mods`;
`cargo build --profile fast -p horizon-client --features java-mods`; launcher tests; one `hz_scenario.py` run with private jars
(the dev env path) to show the mods still boot.

## Done bar

- A feature build started with neither `--java-mods` nor `HORIZON_JAVA_RUN` writes no `Java mods` line in its log, starts no
  `Java mods` thread (`/proc/PID/task/*/comm`), and creates no `.horizon-cache/java-mods`.
- With `--java-mods` on install-rs's `client/`, the mods boot as before.
- `rg 'env::var(_os)?\("HORIZON_JAVA_RUN"\)' crates/` lists only `run_with` and `horizon-java-host/src/main.rs`.

## Depends on

SPEC-1 (the run folder alone starts the JVM, so `--java-mods` needs no other environment).

## Live test for the lead

1. Feature build, scratch homes, no Java variables: `horizon-client --resources … --offscreen --world T --world-frames 600`.
   Expect: a world, no `Java mods` in the log, no `.horizon-cache/java-mods` in the working folder.
2. The same with `--java-mods /home/umceko/hz-poc/play/install-rs/client` and no `JAVA_HOME`/`LD_LIBRARY_PATH`: the mods load
   ("Java mods ready"), the world opens with Mekanism's blocks in `/give` suggestions.
3. `lan.sh`: host with install-rs's mods (`--java-mods`), guest a feature build **without** `--java-mods`. Expect the guest
   to be treated exactly as a guest built without the feature (same log lines on the host for its configuration; no
   `client_connect_remote` for it, no Java frames sent to it). Before this spec the guest answered the probe with 1.
4. A dedicated server folder without `eula.txt`, started with `--java-mods …/server --dedicated --nogui`: the EULA message,
   exit, and no `Java mods` line.
