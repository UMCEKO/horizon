# W15: Java mods for players: the game finds them, the server keeps them, a release carries them

Scope (from the lead): the game starting the JVM only when Java mods are present (at runtime, not a compile-time-only
path), the dedicated server path with Java mods, and a release-shaped Linux build of the `java-mods` feature, as far as
the finish bar needs (Mekanism, AE2, RS, Jade, BG2 and the top-50 set playable on single player and a dedicated server,
M1 events matching, M2 no native-owned mixin target unrouted). Windows and macOS are out.

Evidence was read from code at `java-mods` cc1b066b6, the built binaries on this machine, and the two finish installs
(`/home/umceko/hz-poc/play/install-rs`, `/home/umceko/hz-poc/agents/w12/install`). No live boot was run: every gap below is
shown by code and binaries, and none needed a running game to prove.

## 1. The gap, with evidence

### G1. Loading the JVM is already runtime; finding it is not safe (E1, E2)
- A feature build does not link libjvm. `jni 0.21` with `invocation` `dlopen`s it (`jni-0.21.1/src/wrapper/java_vm/vm.rs:194`,
  `libloading::Library::new`). `readelf -d` of the feature build `.claude/worktrees/slot-1/target/fast/horizon-client`
  (11-06 17:38, contains `HORIZON_JAVA_HOST_JAR` and `JNI_CreateJavaVM`) lists the same `NEEDED` set as the non-feature
  `target/fast/horizon-client`: `libudev, libasound, libgcc_s, libm, libc, ld-linux`. Size 105 MB vs 101 MB (fast profile).
  So "no Java needed when off" already holds for linking; the remaining compile-time switch is only the cargo feature.
- But which libjvm is loaded is left to `java-locator`: `JavaVM::new` → `locate_jvm_dyn_library` → `JAVA_HOME`, else
  `which java` (`java-locator-0.1.9/src/lib.rs:138,203`). On this machine `which java` is `/run/current-system/sw/bin/java`,
  **OpenJDK 21.0.12.1**, and `JAVA_HOME` is unset. Any start without the launcher's environment (a systemd service, a
  restart by `horizon-server`, a player's shortcut) loads Java 21, and the boot jars (class version 69) fail.
- `create_vm` (`crates/horizon-java-host/src/lib.rs:47-77`) needs three environment variables and panics without them:
  `HORIZON_JAVA_RUN`, `HORIZON_JAVA_HOST_JAR` (`.unwrap()` line 49), the args file (`.unwrap()` line 48), `JavaVM::new(...).expect`.
  The panic is caught (`RUNNING`, line 289) but the player reads "called `Option::unwrap()` on a `None` value".

### G2. The game does not decide that Java mods are present (E3, E7, E8)
- The only switch is the environment: `HORIZON_JAVA_RUN` is read in 7 places (`horizon-java-host/src/lib.rs:290,397`,
  `game/java_content.rs:53,64`, `game/java_components.rs:143`, `game/java_screens.rs:1360`, `horizon-java-host/src/main.rs`).
  `docs/java-mods.md` says it: "The game finds NeoForge through dev variables, not a `java-mods` folder."
- **Wire defect when off:** `java_components::install` (`game/java_components.rs:140`) sets `net::on_java_link`
  unconditionally, and `net.rs:1000` answers the server's `horizon:java` probe with 1 whenever that hook is set and the
  login is known. A feature build **without** Java mods tells a Java-mods server it has a Java client; the server's Java
  side then sends it configuration and play frames that `java_link` drops (`horizon_java_host::get()` is None).
- `run_with` (`game.rs:22477-22490`) starts the JVM before `dedicated::preflight`: a fresh server folder without an
  accepted EULA boots NeoForge, then exits.
- A run folder with no mod jar (only Horizon's boot and plugin jars) still boots a JVM.

### G3. The dedicated server loses its Java mods outside the launcher's own process (E4)
- `horizon-server` starts the engine with `--resources … --control-socket … --dedicated --nogui` and no Java argument
  (`crates/horizon-server/src/engine.rs:246-283`); `[engine]` in `server.toml` has no Java key (`config.rs:193`,
  `deny_unknown_fields`).
- Java mods reach the engine only through environment the launcher sets on the `horizon-server` process it spawns
  (`launcher/src/servers.rs:331-334`, `java_mods::server_env`). The service unit has no environment
  (`service.rs:86`: `ExecStart="{program}" --dir "{dir}" run`), so "Run in the background and at login" starts the
  server **without** its mods, and opens a world saved with their blocks without them.
- The server package builds `horizon-engine` with `--no-default-features` and no `java-mods` (`game-build.yml:312`); an
  engine without the feature ignores the variables silently.

### G4. No release carries Java mods support (E5)
- `game-build.yml:167` and `:312` build without `--features java-mods`; `xtask package` (`xtask/src/main.rs:224`) copies
  no `java-mods/` folder. The launcher's `host_jars_in` (`launcher/src/java_mods.rs:442`) therefore refuses every release:
  "carries no Java mods support (no java-mods/ folder with …)". Only a local build with `.local-build` works.
- Whether the server build compiles with `--no-default-features --features java-mods` was never checked.

### G5. A player install has no mod analysis, so W12 routes and W9 entity facts are empty (E6)
- The Java boot reads three files the analysis writes, located by `-Dhorizon.java.analysis` (the `analysis` line of
  `horizon-java.args`): `BridgeProcessor.readRouting` (`routing.tsv`), `EntityFacts.confirm` (`entity-facts.json`),
  `Routing.confirm` (`routing.json`). Each returns empty when the property is unset
  (`plugin/.../BridgeProcessor.java:79-81`, `boot/.../EntityFacts.java:60-61`, `boot/.../Routing.java:93-95`).
- `java_mods.py install` writes the line (`install-rs/client/horizon-java.args`: `analysis …/analysis/entity-facts.json`);
  the launcher's writer has no such line (`launcher/src/java_mods/args.rs:46-95`). A launcher install boots with **no
  routes** (M2 fails: every mixin into native-owned code is silent) and **no entity facts** (`EntityFacts.bin()` empty).
- The analysis is 2807 lines of Python (`mod_replaces.py` 915, `mod_routing.py` 407, `mod_entity_facts.py` 665, the scan in
  `mod_parity.py`) plus `MixinScanner.java` compiled with `javac` at analysis time, and reads the port ledger
  (`.local/port-ledger/ledger.json`, 72 MB, built from decompiled sources). It takes 10.0 s on install-rs and 11.8 s on the
  showcase (`finish/rep-*/replaces.json` "seconds"). Players have no Python, no `javac` step and no ledger.
- Silent fallback today: `install_analysis` uses `{}` when the ledger is missing (`mod_replaces.py:854`), which routes every
  mixin `java-already` or `undecided`.

## 2. The approach

1. **The run folder is the contract** (SPEC-1). `horizon-java.args` names its JDK (`java DIR`) and host jar (`host JAR`).
   The host opens `DIR/lib/server/libjvm.so` by path after checking `DIR/release` says Java 25 or later; no `JAVA_HOME`,
   no `PATH` search. Every failure is a sentence, never a panic.
2. **The game decides** (SPEC-2). `--java-mods DIR` (the launcher's way; `HORIZON_JAVA_RUN` stays as the developers'
   spelling of the same) sets one process-wide run folder. The JVM starts only when that folder's `mods/` holds a jar that
   is not Horizon's own, and only after the dedicated preflight. Everything that asked the environment asks
   `horizon_java_host::run_dir()`. Off means off: no thread, no hook, probe answer 0. A build without the feature refuses
   `--java-mods` in words.
3. **The server keeps its mods** (SPEC-3). `server.toml` `[engine] java_mods = "java"` names the server's run folder;
   `horizon-server` passes `--java-mods` to the engine at every start, so the service, restarts and crash restarts keep
   them. A named folder that isn't ready stops the start loudly.
4. **The analysis runs in the game** (SPEC-4 to SPEC-12). A new Rust crate `horizon-java-analysis` ports the Python
   analysis function by function, checked against the Python outputs on both finish installs. The host runs it before
   `create_vm`, cached by a fingerprint, and passes `-Dhorizon.java.analysis` itself, so the launcher, the server and the
   developers' installs all get the same routes. Horizon's own knowledge the analysis needs (the ledger's native status
   per vanilla method, the verdict map, the host's words) is generated by `java_mods.py build` into the boot jar, which
   every install already carries. Then the Python analysis is removed (SPEC-13): one implementation.
5. **A release-shaped build** (SPEC-14). `python3 tools/java_mods.py release --out DIR` builds the host jars, the client
   and `horizon-engine` with the feature, packages both with `java-mods/`, runs the release checks (system libraries only,
   baseline CPU, starts without any Java, an offscreen world without Java, an offscreen world with the RS pack from the
   packaged folder), and writes `SHA256SUMS`.

## 3. Decisions, with reasons

- **D1. `--java-mods DIR`, not a discovered folder.** The launcher already owns installs; the server owns its folder. An
  explicit argument survives services and restarts, and a game with no argument has nothing to guess (no fallback). The
  environment variable stays only because every dev tool and `live.py` sets it; it means the same thing.
- **D2. libjvm by path, Java version from `release`.** Proven wrong JVM on this machine (G1). `release` is the JDK's own
  file (`JAVA_VERSION="25.0.5"`), read before `dlopen`, so a Java 21 install fails with a sentence.
- **D3. `HORIZON_JAVA_HOST_JAR` stays as an override of the `host` line.** `live.py` isolates agents' JVMs with it
  (`LIVE_JAR`); it is an explicit developer choice, not a fallback.
- **D4. The cargo feature stays.** Upstream builds stay as they are; the fork's release-shaped build turns it on. SPEC-2
  makes "feature on, no mods" inert so that turning it on by default is a one-line decision for the upstream PR.
- **D5. The analysis is Rust, run by the game before the JVM.** Rejected: keeping Python (players have none); a Java
  analysis in the embedded JVM (the class processor reads `routing.tsv` while FML launches, so the analysis would have to
  run in the JVM before FML, a second boot phase; and CLAUDE.md: "Everything players run is Rust"). Rust reuses the zip
  and class-file reading the workspace already has (`horizon-port-pack/src/classes.rs` shows the pattern) and needs no JVM
  to produce routes, so the launcher and server status can show them later.
- **D6. Port first, compare, then delete Python.** Each port spec ends on a dev-only comparison (`tools/java_analysis_compare.py`)
  of the Rust and Python outputs on install-rs and the showcase install: the Python implementation is the independent
  oracle. SPEC-13 deletes the Python analysis and the comparison; the Python unit tests move to Rust with their
  hand-written expected values.
- **D7. Horizon's knowledge ships in the boot jar.** `native.tsv` (per vanilla method key: ledger status, owned, up to 3
  `file:line`, note), `verdicts.tsv` (the port map's rows) and `host_words.txt` are written by `java_mods.py build` from
  the ledger, `tools/port_map/*.tsv` and the host's sources. They are names and statuses, like `route_gates.tsv` already
  in that jar; they are never committed (generated). `build` refuses without a ledger: no empty-ledger fallback.
- **D8. Data tables become data files.** `GROUP_NAMES`, `EXTRA_WRITERS`, `NOT_WRITERS`, `HOOKS`, `FAMILIES`, `HOT_ROOTS`,
  `VIEW_ROOTS` move to `crates/horizon-java-analysis/data/*.tsv`, `include_str!`ed, so "add it to GROUPS" stays a data edit.
- **D9. The release-shaped build is a local script.** It runs where `.local` is (the ledger and the prepared resources), as
  the self-hosted runner does. A workflow job comes with the upstream PR; signing `SHA256SUMS` stays the release job's.
- **D10. One-time conversion of the launcher's `java-mods.enabled` mark** (SPEC-3). Losing it would open a server's world
  without its mods; four lines convert it to the `server.toml` key and delete it.

## 4. Specs

| Spec | Title | Lines (est.) | Depends on |
|---|---|---|---|
| [SPEC-1](SPEC-1.md) | The run folder names its Java and host jar; libjvm by path | ~260 | – |
| [SPEC-2](SPEC-2.md) | The game decides: `--java-mods`, one run folder, off is inert | ~380 | SPEC-1 |
| [SPEC-3](SPEC-3.md) | The dedicated server keeps its Java mods (`[engine] java_mods`) | ~330 | SPEC-2 |
| [SPEC-4](SPEC-4.md) | `horizon-java-analysis` crate and its class-file reader | ~480 | – |
| [SPEC-5](SPEC-5.md) | Libraries, hierarchy, jar walk and jar facts | ~420 | SPEC-4 |
| [SPEC-6](SPEC-6.md) | The mixin scan (replaces `MixinScanner.java`) | ~330 | SPEC-5 |
| [SPEC-7](SPEC-7.md) | Horizon's knowledge in the boot jar; vanilla lambdas folded | ~430 | SPEC-5 |
| [SPEC-8](SPEC-8.md) | Records and `replaces.json` | ~480 | SPEC-6, SPEC-7 |
| [SPEC-9](SPEC-9.md) | Routing: `routing.json` and `routing.tsv` | ~500 | SPEC-8 |
| [SPEC-10](SPEC-10.md) | Entity facts I: bytecode guards, type ids, groups | ~450 | SPEC-8 |
| [SPEC-11](SPEC-11.md) | Entity facts II: writes, caps, hooks, `entity-facts.json` | ~480 | SPEC-10 |
| [SPEC-12](SPEC-12.md) | The game analyses the mods before the JVM starts | ~350 | SPEC-1, SPEC-2, SPEC-9, SPEC-11 |
| [SPEC-13](SPEC-13.md) | Retire the Python analysis; its tests in Rust | ~450 (mostly deletions) | SPEC-12 |
| [SPEC-14](SPEC-14.md) | Release-shaped Linux build | ~360 | SPEC-1, SPEC-2, SPEC-3, SPEC-12 |

Order: SPEC-1, 2, 3 land first and independently of the analysis. SPEC-4 can start in parallel with SPEC-1. SPEC-5..11
are a chain (each ends on a passing comparison of its stage). SPEC-12 switches the game to it; SPEC-13 and SPEC-14 can go
in either order after it.

### Port rules for SPEC-4..11 (in every one of them)
- The Python function is the specification: same order of iteration, same caps (`CLOSURE_CAP`, `HOT_DEPTH`), same
  tie-breaking (`min(..., key=ROUTING_ORDER.index)`), same sorting (Python sorts strings by code point; Rust `String`'s
  `Ord` is the same), same string formats. Name each Rust function after the Python one it ports and give it a one-line
  doc comment naming that function (`/// Port of mod_routing.closure.`).
- Outputs are compared as parsed JSON: object key order ignored, array order kept, `seconds` and `install` dropped.
- Every comparison runs on both finish installs and must report 0 differences before the spec is done.
- The crate never reads `.local/`: everything Horizon-specific comes in as arguments or from the boot jar.

## 5. Out of scope

- Windows and macOS (`libjvm` names are written for both, untested).
- Signing `SHA256SUMS` (the release job's key); a GitHub workflow job (the upstream PR).
- A server-only installer: a headless server box installs with `horizon-launcher java-mods install` (the launcher's CLI).
- `horizon-server update` and the launcher's version updates picking a Java-mods build: they fetch upstream releases.
- Client and server with different mod sets, LAN guests without a JVM, server-pushed packs (W14).
- Detecting host jars built from another tree than the game (STATUS §0 stale jars): the release script builds both from
  one tree; a build stamp is a later nicety.
- The two writers of `horizon-java.args` (the launcher's Rust and `java_mods.py`'s Python) stay two: one is the player's
  installer, the other a dev tool; SPEC-1 changes both the same way and tests both.

## 6. Open questions for the user

1. Is `--java-mods DIR` a Launch API option (`game/launch_api.rs`, a promise to other launchers) now, or only `--help`
   until the upstream PR? The specs put it in `--help` only.
2. The boot jar of a release would carry `native.tsv`: vanilla method keys with Horizon's port status and `file:line`
   of the Rust port. Acceptable to ship (it is names, like `route_gates.tsv` and the mixins already in that jar)?
3. The analysis port is 8 specs (about 3500 lines of Rust). The alternative is a finish bar measured only on installs made
   by `java_mods.py`, leaving launcher installs without routes. The specs assume the port.
4. When `horizon-server update` would move a Java-mods server to a release without the feature, should it refuse (one
   check in `update.rs`), or is that left to SPEC-3's loud start failure?
