# SPEC-3 (W15): the dedicated server keeps its Java mods (`[engine] java_mods`)

Read first: `CLAUDE.md`, `/home/umceko/hz-poc/agents/RULES.md`, `docs/hosting.md`, `docs/server.md` §4 (`server.toml`),
`DESIGN.md` here (G3, D10), SPEC-2. Comments in code are one line at most, and only for a non-obvious why.

## Goal

A server folder says in `server.toml` which Java mods run folder it uses. `horizon-server` passes it to the game at every
start: by hand, as a service at login, after a crash, after a restart. No environment variable carries it. A folder that
names Java mods it can't run never starts without them.

Today: the launcher sets `HORIZON_JAVA_RUN`/`JAVA_HOME`/... on the `horizon-server` process it spawns
(`launcher/src/servers.rs` `command`), the service unit has no environment (`service.rs` `ExecStart=… run`), and
`[engine]` has no such key.

## `server.toml`

```toml
[engine]
java_mods = ""               # a Java mods run folder, relative to this folder (the launcher writes "java"); empty: none
```

## `crates/horizon-server/`

1. `config.rs`:
   - `Engine` gains `pub java_mods: String` (serde default empty; `deny_unknown_fields` stays).
   - The template line above, after `low_memory`.
   - `render`'s list of kept keys gains `("engine", "java_mods")`.
   - ```rust
     impl Engine {
         /// The Java mods run folder to start the game with: None when unset; an error when it can't run.
         pub fn java_mods_dir(&self, dir: &Path) -> Result<Option<PathBuf>, String>
     }
     ```
     Empty → `Ok(None)`. Else `dir.join(java_mods)` (absolute stays absolute); when `that/horizon-java.args` is not a file:
     `Err("server.toml [engine] java_mods names {path}, which has no horizon-java.args: set the server up for Java mods again in the launcher (or with horizon-launcher java-mods server on)")`.
2. `engine.rs` `command(...)` and `spawn(...)`: one more parameter `java_mods: Option<&Path>`; when `Some`, add
   `.arg("--java-mods").arg(abs)` **before** `--dedicated` (everything after it goes to vanilla's parser).
3. `supervisor.rs`, in the start loop beside `let transfers = config.transfer.engine_arg();`:
   `let java_mods = config.engine.java_mods_dir(dir)?;` and pass `java_mods.as_deref()` to `engine::spawn`. The `?` makes a
   broken setting stop this start with the sentence in the supervisor's log and status, as other start errors do.
   Log it: `note(&shared, "INFO", &format!("With the Java mods of {}", path.display()))` when set.
4. `admin.rs` status JSON: under `"engine"`, `"java_mods": <absolute path or null>`. Update the status test.
5. Nothing in `service.rs`: the service runs `horizon-server --dir D run`, which reads `server.toml`.
6. An engine built without the feature exits 2 with SPEC-2's usage error. `engine.rs`' start-failure text already relays
   the game's stderr; add a unit test beside the libasound ones that this message reaches the error text.

## Launcher (`launcher/src/java_mods.rs`, `launcher/src/servers.rs`, `launcher/src/cli.rs`)

- `server_enabled(dir)`: parse `dir/server.toml` with the `toml` crate (`[engine] java_mods` non-empty).
- `set_server(dir, on)`: run `horizon-server --dir D config set engine.java_mods=java` (or `engine.java_mods=` to turn off)
  through `crate::servers::program()` and `crate::process::command`, as the launcher runs other `horizon-server`
  commands; its stderr becomes the error.
- One-time conversion (DESIGN D10): in `servers::command` before anything else, if `dir/java-mods.enabled` exists: call
  `set_server(dir, true)`, then remove the file. Delete `server_mark` otherwise.
- Replace `server_env` with
  ```rust
  /// Makes the server's run folder `dir/java` ready for game version `tag`; refused in words when the install isn't.
  pub fn prepare_server(tag: &str, dir: &Path) -> Result<(), String>
  ```
  the same checks as `apply` (status ready, `support(tag)`), then `prepare_run(tag, &install, &dir.join("java"), true)`.
- `servers::command`: `if java_mods::server_enabled(dir) { java_mods::prepare_server(&tag, dir)?; }` and no `cmd.envs(...)`.
- `cli.rs` `java-mods server on|off` keeps calling `set_server`; `java-mods launch --dedicated --dir D`: unchanged
  besides SPEC-2's argument order.

A service started at login uses the run folder as last prepared (absolute paths into the launcher's install). That is
the same as today's install-time paths; a moved install needs the launcher once, as for clients.

## Tests

- `config.rs`: a `server.toml` with `java_mods = "java"` loads and renders back with the key and its comment; an unknown
  `[engine]` key is still refused. `java_mods_dir`: empty → None; `"java"` with `dir/java/horizon-java.args` present →
  `Some(dir/java)`; missing file → the error text, exactly.
- `engine.rs`: `command(..., Some(Path::new("/srv/mc/java")))`'s arguments are
  `--resources R --control-socket C --java-mods /srv/mc/java --dedicated --nogui` (expected list written by hand), and with
  `None` they are today's.
- `supervisor`/`admin` status test: `engine.java_mods` is the absolute path.
- Launcher `java_mods/tests.rs`: `server_enabled` on a fixture `server.toml` with and without the key; the mark conversion
  calls `set_server` once and removes the file (use a fake `horizon-server` script the tests already use for servers, or
  factor the toml write behind a function the test can call).

## Checks

`cargo test -p horizon-server`, clippy on it; launcher tests and clippy; `python3 tools/pr_ready.py --quick` touches
`horizon-server` docs: if `docs/server.md` lists `[engine]` keys (it does, §4), add `java_mods` there with one sentence.

## Done bar

- `horizon-server --dir D run` with `java_mods = "java"` starts the engine with `--java-mods D/java` (visible in
  `/proc/PID/cmdline` and the supervisor's log line).
- With the key set and `D/java/horizon-java.args` removed, the start fails with the sentence; no engine process.
- `rg "server_env|java-mods.enabled" launcher/src` finds only the conversion.

## Depends on

SPEC-2 (`--java-mods` in the game and its refusal without the feature).

## Live test for the lead (scratch homes; `HORIZON_LAUNCHER_HOME` etc. per CLAUDE.md)

1. `horizon-launcher java-mods pack /home/umceko/hz-poc/play/install-rs/mods`, `java-mods install`, create a server with the
   launcher CLI (or `horizon-server --dir D init`), `horizon-launcher java-mods server on --dir D` (whatever the CLI's form is).
   Expect: `D/server.toml` has `java_mods = "java"`, `D/java/horizon-java.args` exists.
2. `horizon-server --dir D run` from a plain shell with **no** Java variables: the log shows `With the Java mods of D/java`
   and the game's `Java mods ready`. Join with a client that has the same install: Mekanism's blocks place and work.
3. Kill the engine process (`kill -9 <engine_pid>`): the supervisor restarts it with the mods again (same log lines).
4. `horizon-server --dir D service install` then start it (user service), check the status: Java mods running. Remove the
   service afterwards.
5. Set `java_mods = "nowhere"`: the start fails with the sentence, no world is opened.
