# SPEC-14 (W15): a release-shaped Linux build with Java mods

Read first: `CLAUDE.md`, `/home/umceko/hz-poc/agents/RULES.md` (gate, headless, memory), `DESIGN.md` here (G4, D4, D9),
`.github/workflows/game-build.yml` (the `linux` entry and the exporters job), `xtask/src/main.rs` (`package`,
`package-server`), `tools/check_imports.py`, `tools/check_cpu_baseline.py`. Comments in code are one line at most.

## Goal

One command makes, from a clean commit of `java-mods`, the two Linux release folders and archives a player or a server
operator would get, with Java mods support compiled in and Horizon's host jars inside, and proves them the way the release
job proves its builds, plus two Java checks: the game runs with no Java on the machine, and it runs the RS pack from the
packaged folder.

```sh
python3 tools/java_mods.py release --out /home/umceko/hz-poc/agents/w15/release [--keep-target] [--no-smoke]
```

Output in `OUT/`: `horizon-linux-x86_64.tar.gz`, `horizon-server-linux-x86_64.tar.gz`, `SHA256SUMS` (both archives, the
format `sha256sum` writes), `release.json` (`commit`, `neoforge`, `features: ["java-mods"]`, the checks and their results).
Signing is the release job's (out of scope).

## Why a container

The game must be built against the release's glibc (`game-build.yml` uses `ubuntu-22.04`); a build on this NixOS machine
has a `/nix/store` loader and run path, which `check_imports.py` rejects (it does today for `target/fast/horizon-client`), and
the machine's `/lib64/ld-linux-x86-64.so.2` is a stub, so an Ubuntu-built binary can't run on it either. Docker 29 is
installed. Builds and smoke runs happen in `ubuntu:22.04`; the Java side and the exporters (portable class files) are
built on the host, where `.local` is.

## `xtask/src/main.rs`

- `package BINARY OUT [--java-mods DIR]` and `package-server ENGINE OUT [--java-mods DIR]`: with the option, copy the three
  host jars (`horizon-java-host.jar`, `horizon-java-boot.jar`, `horizon-java-plugin.jar`) into `OUT/java-mods/`; a missing
  jar is an error naming it. Without it, unchanged (the upstream release job doesn't pass it).
- Test (xtask has tests for packaging, add beside them): with `--java-mods` the folder holds the three files; without it, no
  `java-mods/` folder.

## `tools/java_mods.py release`

Steps, each printed with its time; any failure stops, and a `trap`-like `finally` stops every container and game started:
1. The checkout is clean (`git status --porcelain` empty) and on a commit: record `git rev-parse HEAD`.
2. Host: `python3 tools/port_ledger.py build` (the knowledge must be this commit's), then `build()` (host jars with their
   knowledge, SPEC-7; the analysis CLI, SPEC-13), into `HORIZON_JAVA_MODS_DIR` when set (private jars), else `.local/java-mods`.
3. Host: the exporters as the workflow's job does (`cargo build --profile fast -p horizon-prepare`,
   `compile-exporters --root ROOT`, copy `server/`, `client/`, `manifest.json` to `ROOT/exporters/`, the stray-file check,
   `tools/check_prepare_exporters.py --binary …`).
4. Container build, `docker run --rm -v ROOT:/src -v OUT/target:/target -v ~/.cargo/registry:/usr/local/cargo/registry
   -e CARGO_TARGET_DIR=/target ubuntu:22.04 bash -c …`: `apt-get install -y --no-install-recommends curl ca-certificates
   build-essential pkg-config git python3 libasound2-dev libudev-dev`, rustup stable minimal, `git config --global
   safe.directory /src`, then
   `cargo build --release --locked -p horizon-client -p horizon-client-lib --features horizon-client/java-mods --bin horizon-client --bin horizon-ctl -p horizon-prepare --bin horizon-prepare -p horizon-server --bin horizon-server`,
   `cargo run --locked -p xtask -- package /target/release/horizon-client /out/horizon-linux-x86_64 --java-mods /jars`
   (mount the host jars folder at `/jars` and `OUT` at `/out`), then the server build as the workflow's step:
   `cargo build --release --locked -p horizon-client --no-default-features --features java-mods --bin horizon-client`,
   `xtask package-server … --java-mods /jars`. Archives: `tar czf` as the workflow.
   Nobody has built `--no-default-features --features java-mods` yet (DESIGN G4). If it fails, the fix is part of this spec:
   `cfg` the Java code that reaches audio or controllers (`java_screens.rs`' sound events, `audio.rs` uses) on those
   features as the rest of the game does; run `cargo clippy -p horizon-client --no-default-features --features java-mods
   -- -D warnings` on the host first, before any container work.
   The container's user maps to the caller (`--user $(id -u):$(id -g)` after the root `apt` step, or a second `docker run`),
   so nothing in `ROOT` or `OUT` ends up owned by root.
5. Container checks on the packaged folders (the workflow's, same commands): `check_imports.py` on the four client programs
   and `--server` on `horizon-engine`, `horizon-server`, `horizon-prepare`; `check_cpu_baseline.py` (needs `llvm-tools`:
   `rustup component add llvm-tools`); `--help`/`--version` checks; and two new ones:
   - **No libjvm link:** `readelf -d horizon-client horizon-engine | grep -c libjvm` is 0.
   - **The support mark:** `grep -c horizon-java-mods-support` on both programs is at least 1 (SPEC-2's mark).
6. Smoke without Java (the gate first: `python3 /home/umceko/hz-poc/agents/gate.py` with `GATE_OWNER=w15-release`):
   in a container with `mesa-vulkan-drivers` (lavapipe) and no JDK, `HORIZON_HOME` etc. in a scratch folder, mount
   `.local/horizon-client/26.3` read-only: `horizon-client --resources … --offscreen --world smoke --seed 1 --render-distance 4
   --world-frames 300 --timeout-seconds 600 --smoke-report smoke.json`; the report's checks as the workflow's
   (`frames`, `chunks`, `meshed_sections`, `player`, no `error`). Then `horizon-engine --resources … --dedicated --nogui
   --port 25599` in a fresh folder with `eula.txt` (`eula=true`), wait for `Done (`, send `stop` on stdin, exit code 0.
7. Smoke with Java mods (gate again): build a run folder `OUT/smoke/server-run` from install-rs: `mods/` = links to
   `install-rs/mods/*` plus the **packaged** `java-mods/horizon-java-boot.jar` and `horizon-java-plugin.jar`, `libraries` →
   `install-rs/libraries`, and `horizon-java.args` = install-rs's `server/horizon-java.args` with its `java` line set to
   install-rs's Temurin (`install-rs/jdk/jdk-25.0.5+8`, portable, not the Nix JDK) and its `host` line to the packaged
   `horizon-java-host.jar`. Mount install-rs read-only at the same path. Run `horizon-engine --resources …
   --java-mods OUT/smoke/server-run --dedicated --nogui --port 25599` in a fresh folder with `eula.txt`; expect the log lines
   `Java mods analysed` (or `cached`), `Horizon: routes:`, `Java mods ready`, then `Done (`; `stop`; exit 0. Same with the
   client: `horizon-client --java-mods <a client run folder built the same way from install-rs/client> --offscreen
   --world smoke --world-frames 300 …`, expect `Java mods ready` and a smoke report with a world.
8. `SHA256SUMS`, `release.json`; delete `OUT/target` unless `--keep-target` (about 10 GB; it lives on `/home`, which has the
   space, never under `/projects`).

## Docs

`docs/java-mods.md`, a short "A release-shaped build" section: the command, what it checks, that signing and a CI job are
the upstream PR's.

## Tests

- xtask's packaging test above.
- `tests/test_java_mods_release.py` (Python unit tests, no Docker): the run-folder rewrite of step 7 (`java` and `host` lines
  replaced, every other line kept, in order) on a fixture args text, with the expected text written by hand; the
  `SHA256SUMS` line format for two fixture files (`<64 hex>  <name>`), checked against `hashlib` on the fixture bytes.

## Checks

The tests above; `cargo test -p xtask`; one full `java_mods.py release` run (it is the done bar).

## Done bar

`java_mods.py release --out …` exits 0 on a clean `java-mods` commit with SPEC-1..13 merged, and `release.json` lists
every check of steps 5-7 as passed. Paste `release.json`.

## Depends on

SPEC-1, SPEC-2, SPEC-3 (the programs it packages), SPEC-12 (the smoke expects the analysis lines), SPEC-13 (the CLI
`build` makes). Before SPEC-12/13 land, step 7's `Java mods analysed` check is the only one that can't pass.

## Live test for the lead

1. Unpack `horizon-linux-x86_64.tar.gz` into a scratch launcher's versions folder as a local version (the launcher's
   `.local-build` path is not needed: the folder carries `java-mods/`). `horizon-launcher java-mods install` with the RS
   pack accepts it (before this spec: "carries no Java mods support").
2. On a machine or VM with Ubuntu 22.04 or later and no Java installed (or the container from step 6 with a GPU device),
   run the packaged game without Java mods: the title screen and a world. Then enable the RS pack through the launcher's
   CLI and Play: Mekanism, AE2 and RS items in creative tabs. Visual: the user.
