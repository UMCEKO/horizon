# SPEC-1 (W15): the run folder names its Java and host jar; libjvm by path

Read first: `CLAUDE.md`, `/home/umceko/hz-poc/agents/RULES.md`, `docs/java-mods.md`, and `DESIGN.md` here (G1, D1-D3).
Comments in code are one line at most, and only for a non-obvious why.

## Goal

A run folder (`horizon-java.args` and its `mods/`) is enough to start the JVM: it names the JDK home and the host jar.
The host opens that JDK's `libjvm` by path after checking it is Java 25 or later. `JAVA_HOME`, `PATH` and
`LD_LIBRARY_PATH` no longer decide which JVM loads. Every failure is a `Result` with a sentence, never a panic.

Today (evidence): `create_vm` reads `HORIZON_JAVA_HOST_JAR` with `.unwrap()`, the args file with `.unwrap()`, and calls
`JavaVM::new(...).expect(...)`, which finds libjvm through `java-locator` (`JAVA_HOME`, else `which java`); `which java` on
this machine is Java 21.

## The args file: two new lines

```
java /abs/jdk/home
host /abs/install/host/horizon-java-host.jar
```
Written first, before `dist client` (order is not significant to the reader, but keep it fixed so tests compare whole files).

## Rust: `crates/horizon-java-host/src/lib.rs`

1. A pure parser, so the rules are testable without a JVM:
   ```rust
   /// What `horizon-java.args` asks the JVM for.
   pub struct RunArgs {
       pub java: PathBuf,
       pub options: Vec<String>,
       pub prog: Vec<String>,
       pub client: bool,
   }
   impl RunArgs {
       /// `host_override` is `HORIZON_JAVA_HOST_JAR` when set: a developer's private host jar.
       pub fn parse(run_dir: &Path, text: &str, host_override: Option<&str>) -> Result<RunArgs, String>
   }
   ```
   - `options`: the file's own options in file order: each `jvm` line as is, `-Djava.class.path={cp}{CP_SEP}{host}` for the
     `cp` line, `-Dhorizon.java.analysis=…` for an `analysis` line (unchanged for now; SPEC-12 removes it).
     `create_vm` builds the JVM's list in today's order: `MEMORY`, then `profile::jfr_options`, then `parse`'s options, then
     `HORIZON_JAVA_OPTS` split on whitespace. Environment stays out of `parse`.
   - `prog`: `["--gameDir", run_dir]` then each `prog` line (today's).
   - `client`: a line equal to `dist client` (move `client_side`'s rule here; delete `client_side`, and read
     `RunArgs.client` where it was used).
   - Errors (exact texts, tests check them):
     - no `java` line: `"{run}/horizon-java.args names no Java (a \`java\` line): install the Java mods again"`
     - no `host` line and no override: `"{run}/horizon-java.args names no host jar (a \`host\` line): install the Java mods again"`
     - no `cp` line: `"{run}/horizon-java.args has no classpath (a \`cp\` line)"`
2. `fn java_major(home: &Path) -> Result<u32, String>`: reads `home/release`, the line `JAVA_VERSION="25.0.5"`, takes the digits
   before the first `.` (`"25"`), or the whole value for a bare major (`JAVA_VERSION="25"`). Errors: no file →
   `"{home} is not a Java home (no release file)"`; no line → `"{home}/release names no JAVA_VERSION"`.
3. `fn libjvm(home: &Path) -> PathBuf`: Linux `lib/server/libjvm.so`, macOS `lib/server/libjvm.dylib`, Windows
   `bin/server/jvm.dll` (`cfg!(target_os = ...)`).
4. `pub fn create_vm(run_dir: &str) -> Result<(JavaVM, RunArgs), String>`:
   read the file (error `"{run}/horizon-java.args: {io error}"`), `RunArgs::parse`, `java_major` < 25 →
   `"Java mods need Java 25 or later; {home} is Java {n}"`, `libjvm` missing → `"{path} is missing: {home} is not a full JDK"`,
   then `JavaVM::with_libjvm(args, || Ok(libjvm(&home)))`. Map `StartJvmError` with `to_string()`. No `unwrap`/`expect`.
5. `boot()` (line 309): `let (vm, run) = create_vm(run_dir)?;`, pass `&run.prog` where `prog` was used.
6. `crates/horizon-java-host/src/main.rs` (the dev host binary): use the `Result` (`.unwrap_or_else(|e| { eprintln!("{e}"); exit(1) })`).
7. `jni::InitArgsBuilder` is still used; `JavaVM::new` is no longer called anywhere (`rg "JavaVM::new"` = 0).

## Writers of the args file

### Launcher (`launcher/src/java_mods/args.rs`, `launcher/src/java_mods.rs`)
- `args_lines(os, dedicated, libs, neoforge, profiles, assets, java: &Path, host_jar: &Path)`: push `java …` and `host …` first.
- `prepare_run` passes `record.java_home(install)` and `install.join("host").join(HOST_JARS[0])`.
- `launch_env`: remove `JAVA_HOME`, `HORIZON_NEOFORGE`, `HORIZON_JAVA_HOST_JAR`, and the JDK's directories from the library
  path (`library_dirs` is then unused: delete it and its test). Keep the library path variable only for
  `/run/opengl-driver/lib` and the caller's value, as today. `HORIZON_JAVA_RUN` stays until SPEC-2.

### Dev tool (`tools/java_mods.py`)
- `args_lines(dist, libs, neoforge, client=None, server=None, assets=None, analysis=None, java=None, host=None)`: when given,
  `java` and `host` lines first.
- `write_args`: `java=java_home()`, `host=HOST_JAR` (the private-jar case is `HORIZON_JAVA_HOST_JAR`, which wins in the host).
- `install_pack`: `java=java`, `host=install / 'host' / 'horizon-java-host.jar'`.
- `launch_env`: drop `JAVA_HOME`, `HORIZON_JAVA_HOST_JAR` and the JDK lib dirs (keep the drivers dir and `HORIZON_NEOFORGE`,
  which the oracle tools read).

## Tests

- `horizon-java-host` unit tests (`lib.rs` `tests` module), expected values written by hand:
  - `parse` of a fixture text (`java /j`, `host /h.jar`, `dist client`, `cp /a.jar:/b.jar`, `jvm -Dx=1`, `prog --fml.x`, `prog 1`):
    `options == ["-Djava.class.path=/a.jar:/b.jar:/h.jar", "-Dx=1"]` when the `cp` line comes before the `jvm` line in the fixture,
    `prog == ["--gameDir", RUN, "--fml.x", "1"]`, `client == true`, `java == "/j"`.
  - With `host_override = Some("/p.jar")`: the classpath ends with `:/p.jar`.
  - Each of the three missing-line errors, by exact text.
  - `java_major` on temp dirs: `JAVA_VERSION="25.0.5"` → 25 (the line from Temurin 25.0.5+8's `release`), `JAVA_VERSION="21.0.12.1"` → 21,
    `JAVA_VERSION="26"` → 26, no file → the error text.
- `launcher/src/java_mods/tests.rs`: `args_lines` for the client and the server start with `java …` and `host …`;
  `launch_env` has no `JAVA_HOME`, `HORIZON_JAVA_HOST_JAR` or `HORIZON_NEOFORGE` key. Update the existing tests that list
  `launch_env`'s keys.
- `tests/test_java_mods_install.py`: the two `args_lines` tests gain the `java`/`host` arguments and check the first two lines.

## Checks

`cargo test -p horizon-java-host`, `cargo clippy -p horizon-java-host -- -D warnings`, `cargo test` and `clippy` in
`launcher/` for `java_mods`, `python3 -m unittest tests.test_java_mods_install`, `cargo build --profile fast -p horizon-client --features java-mods`.

## Done bar

- `rg "JavaVM::new|java_locator|HORIZON_JAVA_HOST_JAR\").unwrap" crates/` finds nothing.
- The unit tests above pass.
- One `hz_scenario.py` run (private jars, `HORIZON_JAVA_MODS_DIR`) boots: the log has `Java mods ready`.

## Depends on

Nothing. SPEC-2 builds on it.

## Live test for the lead

1. Re-run `java_mods.py install --pack … --out /home/umceko/hz-poc/agents/w15/inst` (scratch), then start the game from a
   shell with `env -u JAVA_HOME -u LD_LIBRARY_PATH` and `PATH` containing the system Java 21 (`/run/current-system/sw/bin`),
   with `HORIZON_JAVA_RUN=.../inst/client` and the usual scratch homes. Expect: `Horizon: Java mods ready …` in the log.
2. Edit `inst/client/horizon-java.args`, point `java` at a Java 21 home. Expect: no world opens and the title screen shows
   `Java mods need Java 25 or later; … is Java 21`.
3. Remove the `java` line. Expect the "names no Java" sentence, no panic text.
