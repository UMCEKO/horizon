# Game module discovery

This engine-layer build helper owns `.game-area` discovery. Both the game build script and source traversal tools call `declarations`. The generated `game_modules.rs` keeps each area root in the same logical module scope; physical area folders do not become Rust modules.

Each area contains a `.game-area` marker. Its immediate `.rs` files (except `mod.rs`) and child directories containing `mod.rs` are module roots. Names are emitted in deterministic filename order. Duplicate physical root names are rejected. The caller supplies any root files or directories that it declares separately; other unmarked directories or loose Rust roots are errors.

Optional source headers:

- `// game-module-visibility: pub` or `pub(crate)`; otherwise private.
- `// game-module-cfg: <cfg expression>`; the expression is parsed as Rust metadata. `always` disables the implicit `cfg(test)` for roots ending in `_tests`.
- `// game-module-name: <identifier>` gives a cfg-selected implementation another logical name.

There are no game feature names, platform dependencies, or unsafe operations in this crate. Native-hook, LAN action, host, hook-fire and goal-view generation remain in their existing generators.

The extraction was scripted before boundary changes. Verify the verbatim move with:

```sh
python3 tools/refactors/extract_game_discovery.py --verify --moved-commit f38483fc33
```

## Side census (`census` feature)

`cargo run -p xtask -- sides [--json] [--list methods|fields|files] [--root DIR]` reports which side's `Game` reaches each `impl Game` method, field and file: server, client, both or neither. It reads `crates/horizon-client/src/game.rs` and `game/` and writes nothing.

It is lexical, not type-checked: a call edge is a call on a `Game` receiver (`self`, `g`, `x.game`...) matched by method name, blocks under a mode guard (`self.remote.is_some()`, `let Some(..) = self.lan`, `!execution_role.is_client()` and kin) are pruned per side, and the walk starts at the client roots (`tick_remote`, `render`, `handle_packet`, the window handler, the client trait impls) and the server roots (`tick_server_body`, `run_dedicated_inner`, `tick_lan`, the server trait impls). Data-dependent deadness is invisible, so "both" is an upper bound: a dispatcher that branches on `remote` pulls both sides into everything it calls. `--json` prints the typed `Census` (every method, field and file with its side).
