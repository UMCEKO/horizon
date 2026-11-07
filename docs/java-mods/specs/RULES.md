# Rules for agents working on Horizon's Java-mods branch

Repo: /projects/horizonmc, branch `java-mods`. First read `CLAUDE.md` and `docs/java-mods.md`. The latter covers how mods run: an embedded JVM runs the NeoForge 26.3 server side of mods against views of Horizon's world, plus a Java client in the same JVM that gets the server's packets.

## Hard rules
- **Clean room.** Never read the source or decompiled classes of mods (Mekanism, AE2...) or of NeoForge. Allowed:
  - `javap` signatures.
  - Runtime traces, JFR (`jdk.MethodTrace`), packet logs.
  - Public docs.
  - Vanilla's decompiled sources in `.local/minecraft/26.3/sources/{client,server}/`, freely.
- **No per-mod code.** Nothing keyed on a mod id or a mod's class. Fix the shared layer so every mod works.
- **No polling, no fallbacks, no workarounds.** The Java client is a faithful Forge client: it learns only from server packets, in vanilla's order and timing.
  - Banned: timers, re-checking state, mirroring server objects into the client, client code reading server objects.
  - When something is missing, find what vanilla or Forge does (port it from vanilla's source) and do that.
  - Accepting anything "by namespace" in place of the real registry is a fallback and is banned.
- **Offload to native first (user, 2026-11-07).** Java runs only what can't be known before the call. Order of preference:
  1. Static analysis of the jars (bytecode: overrides, constants, mixin targets, which events a mod listens to) at install or boot, stored in native tables.
  2. Capture during init: what Java builds at boot or world load (registrations, data, models, shapes, tab contents, keymaps, command trees, particle/sound types) lands in Rust then.
  3. Runtime Java only for mod code that really runs per call (machine logic, a listener with live state), gated so nothing crosses when no mod listens.
  A spec that would call Java at runtime for something steps 1 or 2 could provide is done the native way instead; say so in the result notes.
- **All rendering is native Rust.** No per-frame Java render path. Mods' models reach Rust as data.
- **Comments are one line max**, and only for a non-obvious why. Never restate the code.
- Never commit Mojang files, `.local/`, mod jars or mod assets. **Do not commit at all**: the parent reviews and commits. Never `git stash`, `git checkout <file>`, `git reset` or `git clean`. Other agents edit this tree at the same time; touch only the files your task needs.
- If a build fails in a file you didn't touch, another agent is mid-edit. Wait a minute and retry. Don't "fix" their file.
- Run builds and games with `nice`; the user may be gaming. Only `/home/umceko/hz-poc/...` scratch homes. Never write to `~/.local/share/{applications,horizon,horizon-launcher}` or `~/.local/bin`, or to the user's Prism instances. Never open a window on the user's desktop (offscreen only).
- The disk was nearly full recently. Don't make new cargo target dirs or worktrees; clean up big scratch files you create.

## Build
- Java host: `cd /projects/horizonmc && TMPDIR=/tmp nice nix-shell -p jdk25 pkg-config --run 'python3 tools/java_mods.py build'`. Output: `.local/java-mods/horizon-java-host.jar`.
- Game: `cd /projects/horizonmc && TMPDIR=/tmp nice nix-shell -p pkg-config alsa-lib udev --run 'cargo build --profile fast -p horizon-client --features java-mods'`. Check `tools/java_mods.py` / `docs/java-mods.md` for the exact feature name if that one fails.
- On an sccache temp-dir error: `sccache --stop-server; TMPDIR=/tmp sccache --start-server`.
- The oracle (facts exporter): `HORIZON_NEOFORGE=26.3.0.101 nix-shell -p jdk25 --run 'python3 tools/neoforge_oracle.py export --jars DIR --out FACTS'`. Source: `mods/horizon/src/neoforge-oracle-26/`.
- The mods' work folders (port-pack output, carried mods): `/home/umceko/hz-poc/mek/work/mods/*/carry`. Mod jars: `/home/umceko/hz-poc/mek/jars`.

## Live game (offscreen, driven by horizon-ctl)
`/home/umceko/hz-poc/mek/live.sh` wraps `/home/umceko/hz-poc/mek/live.py`. To keep yours separate from other agents':

- Give it its own state file and its own copy of the host jar. A rebuild by another agent must not overwrite the jar under your JVM:
  ```
  export LIVE_STATE=/home/umceko/hz-poc/agents/<you>/live.json
  export LIVE_JAR=/home/umceko/hz-poc/agents/<you>/host.jar
  ```
  After each Java build, copy `.local/java-mods/horizon-java-host.jar` to `$LIVE_JAR`.
- Commands:
  - `/home/umceko/hz-poc/mek/live.sh start <you>` launches a fresh world with Mekanism, AE2 and the rest loaded.
  - `/home/umceko/hz-poc/mek/live.sh ctl <args>` runs horizon-ctl commands: `state`, `command "<cmd>"`, `place X Y Z --item id`, `interact X Y Z`, `select N`, `screenshot /abs/path.png`, `keys`, `turn`, `wait N`, `script /abs/path`... See `docs/horizon-ctl-commands.md`.
  - `/home/umceko/hz-poc/mek/live.sh stop` stops the game. Always stop it when you're done.
- The game's JVM needs JDK 25: run `live.sh` inside `nix-shell -p jdk25`, or the system's Java 21 fails with `UnsupportedClassVersionError`.
- Use absolute paths for screenshots and scripts.
- Examples: `/home/umceko/hz-poc/mek/infuser.sh` and `/home/umceko/hz-poc/mek/ui.sh`. Note that `data merge` on a block placed with `setblock` currently reports "not a block entity".
- The game log is under the scratch home `/home/umceko/hz-poc/p/live-<you>/`, and in `TMPDIR=/home/umceko/hz-poc/p`.
- Look at a screenshot once and decide. Don't keep re-reading the same image.
- For Java's view, `HORIZON_JAVA_DEBUG=1` prints extra logs. JFR method tracing: `OPTS="-XX:StartFlightRecording=jdk.MethodTrace#enabled=true,jdk.MethodTrace#filter=<Class::method>"` on start, then `jcmd PID JFR.dump`, then `jfr print`.

## Testing: who runs what
The parent (coordinator) is the tester. You build; it verifies.
- **You run:** a build, clippy `-D warnings` on the crates you touched, and only the tests relevant to your change: unit tests of the modules you touched, plus at most the one or two parity harnesses or Java checks that cover them.
- **You don't run:** the full suite, `tools/pr_ready.py`, `cargo test --workspace`, broad `tools/parity.py --changed` runs, or long live-game verification sessions. Those are heavy, and the parent runs them after you report.
- A live game is allowed only to investigate a cause you can't find otherwise: short, one at a time, stopped right after. **At most one live run per fix.** If it fails for environment reasons (the gate, binary races, launch scripts), don't retry: report it. The user flagged repeated failing test attempts as wasted tokens.
- When you report done, the parent tests the feature in the game. If it fails, you'll get back what was done, what was expected, what happened, and the evidence. Fix that and report again.

## Visual and real-input checks go to the user
Anything that needs real input (aiming, clicking, keys) or a visual judgement is tested by the user, not by you. Don't spend runs trying to drive or screenshot it. Build it, cover what you can with unit tests and traces, and in your report write the steps for a human: what to place, where to look, what to press, and what they should see.

## Report back
- The root cause, with evidence.
- What you changed: files plus a one-paragraph summary.
- Which tests you ran and their results.
- **How the parent should test it:** the exact steps in the live game (commands, placements, what to look at) and the expected result for each, so it can verify without guessing.
- Anything you couldn't check, and what needs a real window.

Keep it concise.

## Memory: at most 4 games on the whole machine
The user keeps running out of memory. **At most 4 game processes may run on this machine at a time, across all agents, and at least 16 GB must stay available.** That counts Horizon and real Forge, client or server.
- `live.sh start` runs the gate itself, `/home/umceko/hz-poc/agents/gate.py`. The gate refuses if 4 games are running or starting, or if less than 16 GB is available. If it refuses, wait (`sleep 120`, then retry) and do other work; never kill another agent's game.
- Any other game you start (a real Forge server or client, your own scripts) must run `python3 /home/umceko/hz-poc/agents/gate.py` first and only start on exit code 0. A pass claims the slot for 120 s for your `LIVE_STATE` (or `GATE_OWNER` if you set no `LIVE_STATE`), so set the same one for the gate and the start.
- `python3 /home/umceko/hz-poc/agents/gate.py list` shows what's running and the queue.
- **The gate is a first-come-first-served queue.** Your first refused call puts you in line. Keep retrying about every 30 s; each retry keeps your place, and 5 minutes without one drops you. Always set `LIVE_STATE` (or `GATE_OWNER=<your name>` for non-live.sh games) so you're not confused with another agent: the default owner, the cwd, is shared.
- Stop your game the moment you have your evidence. Never leave one running while you read code or build.
- **Every script that starts a game must kill that game on every exit path** (`trap`/`try...finally` on the pid), including failed or timed-out launches. Never retry a launch in a loop without the gate. A leaked loop once left 20 orphaned games holding 32 GB RAM and all 24 GB of VRAM.

## Shared jars
`LIVE_JAR` only isolates the host jar: `live.py` symlinks the boot and plugin jars from `.local/java-mods`, so another agent's rebuild reaches your JVM on its next start. If the Java client's join or boot suddenly fails in code you didn't touch, wait and retry; don't debug it.
**Never leave a broken jar in `.local/java-mods`:** every agent's next game uses it. While your Java change is unproven, build and run it from a private copy (`LIVE_JAR` plus your own boot jar path). Install it to `.local/java-mods` only once your own run boots and joins.
While your Java change is unproven, `export HORIZON_JAVA_MODS_DIR=/home/umceko/hz-poc/agents/<you>/jars`: `tools/java_mods.py` (build, run, instance) and `live.py` then build, read and run every jar from there instead of `.local/java-mods`; unset it and rebuild to install.

## Tools (use these, don't write your own launchers or tail logs)
- Set `export HORIZON_GAME_GATE="python3 /home/umceko/hz-poc/agents/gate.py" HORIZON_GFX_ENV=/home/umceko/hz-poc/agents/ship/gfx.env` before any tool that starts a game.
- Run and check the game: `python3 tools/hz_scenario.py run --install DIR --script S.json --out OUT` (examples in `tools/scenarios/`). It prints one JSON result.
- Read a log: `python3 tools/logsum.py LOG` (`--grep RE`, `--since PAT`). Never `cat`, `tail` or `grep` a raw game log.
- Code map: `docs/java-mods-map.md`. Read it before exploring.
- Coverage gaps: `python3 tools/mod_parity.py explain --run DIR` on a comparison run.
- Scenarios: build the floor (with forceload if the area may be unloaded) BEFORE any `tp`, and add a `near` check right after the `tp`. Three scenarios failed because the player fell before the floor existed.
- Scenarios that build anywhere must `/forceload add` the area (then wait) before `/fill` or `/setblock`. In a `--fresh` world the spawn can be far away, and an unloaded fill silently does nothing.
- Never set CARGO_TARGET_DIR to /projects/horizonmc/target from a worktree. Builds from different source trees share artifact names and corrupt each other: stale crates, missing functions. Each worktree uses its own target/ (the default).
- After the origin/main merge (7ee2f42f2) the game is a lib module: run java-mods tests as `cargo test -p horizon-client --features java-mods --lib java_` (`--bins java_` now runs 0 tests). Branches based before the merge keep `--bins`.

- 2026-11-05: sccache server restarted with TMPDIR=/tmp and a 200G limit; builds use it again. Only if "Failed to create temp dir" reappears, prefix `RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER=` and tell the lead. Delete a worktree's `target/` once its branch is merged.

## Worktree slots (2026-11-05, replaces "one worktree per agent")
- Agents work only in `/projects/horizonmc/.claude/worktrees/slot-1` or `slot-2` (one agent per slot). Never `git worktree add`, never `isolation: "worktree"`.
- Start a task: `git -C <slot> status` must be clean; `git -C <slot> switch -c <branch> <base>` (or `switch <branch>`). Commit before handing the slot back; never leave it dirty.
- The slot's `target/` stays warm across tasks: don't delete it. `.local` in a slot is a symlink to the shared one.
- `/home/umceko/hz-poc/verify/trim.sh` trims stale incremental data and deletes merged worktrees' targets.
- sccache is off (removed from the system env; after `nrs` it's gone). Until then, prefix builds with `RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER=`.
- slot-1 = the entities worktree (agent-aef92fbe041188edb) once its merge lands.

## Headless and test budget (user, 2026-11-05 — hard rules)
- Nothing may open a window on the user's desktop. Run harnesses ONLY via `/home/umceko/hz-poc/verify/parity.sh` and LAN only via `lan.sh` (both force a private Xvfb :99 and unset WAYLAND_DISPLAY). Never run `tools/parity.py`, `lan_e2e.py`, `pr_ready.py` without `--quick`, the game, or a vanilla client directly. For anything else that starts a game: `DISPLAY=:99 WAYLAND_DISPLAY= ` prefix.
- Test budget per task: cargo tests of the touched modules, clippy, fmt, `pr_ready.py --quick`, and at most ~5 named harnesses / 2 LAN checks that cover the change. No `--changed`, no full pr_ready, no suite-wide runs: the lead runs the wide suite once per merge. A feature must not spend more time in tests than in work.
