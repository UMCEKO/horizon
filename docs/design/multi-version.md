# Older Minecraft versions beside 26.3

A design note, before any code: how Horizon plays an older release (26.2, 26.1.2) in the same build as the pinned
26.3, what stays shared, what it costs a player who only plays 26.3, and how 26.3 parity stays safe while it
happens. Agreed with Ludovic on 2026-11-05: it goes into main if it works, as this note and then small PRs. Issue
[#9881](https://github.com/horizon-rust/horizon/issues/9881).

Why: older versions have the modpacks and worlds players have. Not covered here: a 26.3 client talking to older
servers by translation (`crates/horizon-protocol/src/adapter/`, #8718), which is a different feature and stays as
it is; obfuscated versions (before 26.1); world downgrades.

## Rules

1. **Vanilla 26.3 stays the spec for 26.3.** Every 26.3 harness keeps passing, unchanged, on every PR.
2. **Unused, it costs nothing.** A player who only plays 26.3 downloads, stores, allocates, starts and reads
   nothing new.
3. **One engine.** Only behaviour that differs between versions is split, and data is preferred to code.
4. **Nothing rots unnoticed.** A ported method that differs in an older version can't land without a recorded
   verdict, checked in CI.

## How versions coexist

**The version is chosen once, at startup.** The launcher passes it as `HORIZON_MC_VERSION` (unset means the pin).
The game reads it into a `VersionProfile` in `horizon-core`, a `OnceLock` set before anything else runs: the
version's facts (protocol, data version, pack formats) and its order among the supported versions. There is no
other way to ask which version runs; changing version means restarting the game.

**Prepared data per version.** `horizon-prepare` already builds `.local/horizon-client/26.3` from the player's own
26.3 install. It builds `horizon-client/26.2` the same way from the player's own 26.2 install. Registries, tags,
recipes, loot tables, worldgen data, block states, models and lang all come from that version's files, so most
differences between versions never reach code at all.

**One function per version, cut clean.** Where an older version's logic differs, it gets its own copy of the
whole function; the 26.3 function keeps its body byte for byte and gains one attribute line:

```rust
/// `ServerExplosion.hurtEntities`: damage and knockback for every entity in range.
#[versioned {
    ..=V26_2 => v26_2::hurt_entities,
    V26_3 => self,
}]
pub fn hurt_entities(&mut self) {
    ... // the 26.3 body, unchanged
}
```

```rust
// crates/horizon-world/src/level/explosion/v26_2.rs (a child module: `use super::*`)
/// `ServerExplosion.hurtEntities@..26.2`
pub(super) fn hurt_entities(level: &mut Level) {
    ... // the 26.2 body, whole, no team check
}
```

- `#[versioned]` picks the function once at startup from `VersionProfile` (a dispatch table, not a branch per call),
  so the 26.3 body never contains another version's code.
- A copy is a child module of the file it replaces (`explosion.rs` declares `mod v26_2;`), so it reaches the parent's
  private helpers and nothing else can call it. Callers never change: they call the 26.3 fn, which routes.
- Deleting a version deletes its `v26_2.rs` files, their `mod` lines and their `..=V26_2 =>` arms.
- `versions.py fork Class.method --mc-version 26.2` writes the copy, the attribute and the citation; the porter edits
  only the lines that differ. Going back to 26.1.2 forks the 26.2 copy, so each step starts from the nearest body.
- Copies are whole, but nobody reads them whole: `versions.py diff Class.method` shows a copy as a diff against the
  newer body it was forked from, and a backport PR's review comment lists exactly that diff per copy. When a PR
  edits a 26.3 body that has copies, `pr_ready.py` prints the same diff for each copy, so a fix that applies to the
  older versions is carried over in the same PR.
- The ranged citation (`@..26.2`) tells the port ledger which versions a copy ports. Ranges follow the order in
  `research-lock.json` (the pin, then older versions newest first), never a parse of the version string.

**Protocol tables per version.** Packet ids, registry ids and entity-data layouts differ between versions. They come
from generated tables per version, selected through `VersionProfile` when the connection starts, not from `if`s in
codecs. Packets whose fields changed get a codec per version range.

**Data first.** A constant, threshold or table that differs becomes a per-version value, never an `if`:

```rust
pub const ZOMBIE_FIRE_CHANCE: Versioned<f32> = versions! {
    ..=V26_2 => 0.3,
    V26_3 => 0.4,
};
```

`versions!` and `#[versioned]` share one match-like syntax: arms are versions or inclusive ranges (`..=V26_2` is
26.2 and everything older), checked at compile time like a `match`: every version covered once, no `_`, and the
pinned version named in its own arm. Adding an older version needs no edit where it behaves like the newest arm
covering it; where it doesn't, the arm splits (`V26_2 => …, ..=V26_1_2 => …`).

**Editor support is a requirement.** Version names are real constants, `versions!` arms are real expressions and
`#[versioned]` passes bodies through with their spans, so completion, hover and go-to-definition work everywhere
code is typed; arm targets resolve to the copies. On unfinished input both macros emit the item unchanged plus a
`compile_error!`, so the editor keeps working mid-edit. A rust-analyzer fixture test guards this.

## What stays shared

Everything else: the renderer, chunk storage, meshing, light, streaming, the world generator's machinery, entity
physics, the server thread, LAN and the dedicated server, the launcher, the UI and mods. An older version is the same
engine running different data, plus per-version copies of the functions whose logic differs. There is one branch, one build and one
binary per platform.

## What it costs a 26.3-only player

Nothing:

- **No download, no extra data.** Older versions are never bundled. The first time a player picks 26.2 in the
  launcher, it prepares 26.2 from that version's own install in the player's Minecraft launcher, as it prepares
  26.3 today. Horizon still never downloads the game.
- **No allocation, thread or file read** when the version is the pin. Per-version tables for older versions are
  only loaded when that version is selected.
- **No change to the 26.3 code path.** Every 26.3 function keeps its body byte-identical to main (checked, below);
  the version is chosen once at startup, never branched on per call.
- **No new choice.** The picker only offers versions the player has installed; 26.3 stays the default.

## How 26.3 stays safe

- **26.3 harnesses unchanged and passing.** A backport PR never edits a 26.3 harness's expected output.
- **26.3 bodies unchanged.** `version_lint.py --pinned-unchanged` checks in CI that a backport PR adds only
  `#[versioned]` lines and `v<version>/` modules, and every `#[versioned]` fn's body matches `origin/main`.
- **Version-constant lint.** `tools/version_lint.py` flags protocol and data version numbers, pack formats, version
  strings, `.local/.../26.3` paths, tables in registry order and numeric id constants written outside the generated
  `version.rs`, the older-server adapter and the datafixer. It lands with a baseline of what main has today, so it
  starts green and the count only goes down.
- **A committed method delta.** `tools/versions/delta/<version>.tsv` lists every non-trivial vanilla method whose
  bytecode differs between that version and 26.3: method names and 12-hex body hashes only, no Mojang code.
  Renames are inferred from the bodies (from 26.1 the jars carry official names, and no mapping links one version's
  names to another's), with a short hand-written override file. Generated, never edited by hand, and checked
  against the jars in `parity.yml`.
- **Verdicts.** For each ported method in a delta, `tools/versions/verdicts/*.tsv` records one verdict per distinct
  older body: `same`, `ported-differently` (naming the Rust fn), `not-applicable`, `todo` (with an issue) or
  `unreviewed`. Heuristic ports count like cited ones. Each row carries both bodies' hashes, so it goes stale the
  moment vanilla changes under it. `tools/versions.py check` fails on a missing, stale or invalid verdict; it runs
  offline in `verify` (forks included, no jars needed) and with the full ledger in `parity.yml`.
- **What a 26.3 contributor sees.** If they newly port a method whose body differs in an older version,
  `tools/pr_ready.py` writes an `unreviewed` row for it and they commit that one generated line. Nothing else in
  their workflow changes.
- **The next pin bump is a finite list.** When the pin moves to 26.4, 26.3 becomes an older version. Verdicts whose
  hashes still match are kept; what needs review is exactly the ported methods that changed between 26.3 and 26.4,
  once per distinct older body. `version_port.py report` prints that number before porting starts.

Harnesses opt in per version (`PARITY_VERSIONS = "26.2.."`); `tools/parity.py --mc-version 26.2` runs only those,
with per-version oracle overlays in `mods/horizon/src/versions/<version>/java/`, so no harness script needs an edit.
Until a harness declares an older version, the extra CI step selects nothing and costs seconds.

## Backporting by agents

The work is large (5,455 vanilla methods changed from 26.1.2 to 26.3, 2,148 of them ported; 3,666 and 1,462 from
26.2) and mostly mechanical, so it is meant to be done by agents with no prior context. Every decision is a rule, a
tool output or a CI failure, never judgement.

**Batches.** `versions.py batches --mc-version V` groups `unreviewed` rows: rows ported by the same Rust fn, then
rows with the same old body, at most ~15 rows or ~400 lines. Order: protocol and registries, then `horizon-core`,
`horizon-world`, gameplay by package, then client-only; within a package, most-called first.
`versions.py batches --open N` files the next N as issues (`Backport V: <package> <fn>`). An agent takes work only by
`/claim` on one of them, under the usual claim rules, so two agents never take the same rows.

**The loop, per row** (a `backport` skill beside `vanilla-parity`):

1. `versions.py show KEY --mc-version V`: both bytecode bodies and their diff, both decompiled bodies from the
   player's own jars (printed, never written into the repository), the Rust fn that ports it and the harnesses that
   cover it.
2. Classify with the decision table below.
3. If code is needed: a per-version data row for a constant, or `versions.py fork` for logic, then edit the copy.
   The 26.3 body stays byte-identical.
4. Cover it: add the version to the fn's harness, or add a harness from the template.
5. Run `parity.py --mc-version V <harnesses>`, then `parity.py <harnesses>` (26.3, must stay green), then
   `version_lint.py` and `versions.py check`.
6. Record the verdict with both hashes (`versions.py verdict KEY <verdict>`).

After the batch: `pr_ready.py`, and a PR with `Fixes #N` for the batch issue.

**Decision table.**

| What the diff shows | Verdict | Code |
|---|---|---|
| Only names, descriptors or constant order differ | `same` | none (the tool writes it) |
| Behaviour differs, and Rust already matches both (the harness passes on both, unchanged) | `same` | none |
| A constant, threshold, table or registry entry differs | `ported-differently` | a per-version data row, never an `if` |
| Control flow or a call differs | `ported-differently` | a per-version copy (`versions.py fork`), 26.3 body untouched |
| The subject doesn't exist in Horizon (dev tool, telemetry, a removed online service) | `not-applicable` | none, a one-line reason |
| A stop condition | `todo #N` | none |

**Stop conditions.** The agent writes `todo #N` with the reason, comments on the batch issue, `/unclaim`s what it
can't finish and moves on when: the fix would touch a 26.3 body or a 26.3 harness's expected output; no harness
can observe the behaviour and none can be written from the template; it needs a subsystem Horizon doesn't have; the
batch passes ~500 lines or 3 crates; or the two versions' harness results disagree in a way the diff doesn't
explain. Those rows form a `needs-design` queue for a person.

**Review.** CI is the gate: per-version harnesses, 26.3 harnesses, `versions.py check`, the version lint and the
26.3-body check. A second agent, not the author, re-derives each verdict from `versions.py show` without seeing the
author's. People spot-check a random 10% of `same` rows and every `not-applicable` row. Before scaling up, a pilot
of ~20 batches measures how often the reviewer disagrees, and the table is tightened where it does.

## Delivery

- **Tier 0: tooling.** This note and the tooling PRs below. No older version is playable yet; nothing changes for
  players.
- **Tier 1: 26.2.** Versions go newest first: each step back starts from the smallest diff, and the next one
  reuses its work. It is *playable* when its protocol, registries and worldgen batches are done and the LAN smoke set passes on it, and
  *done* when it has no `unreviewed` or `todo` rows and every harness that declares it passes.
- **Tier 2: 26.1.2,** the same way. Rows whose 26.1.2 body is the same as 26.2's take 26.2's verdict and code.

Each tier lands as small batch PRs and is usable on its own. The game's `VersionProfile`, the protocol tables and
the launcher's picker are product PRs of Tier 1, each with its own issue.

### Tooling PRs

Each under ~500 lines with tests, `skip-release` (players see nothing). Generated data is counted apart.

| # | PR |
|---|---|
| 1 | Older versions in `research-lock.json` (`backports`); a selected version and local root in `lab.py` |
| 2 | Supported versions' facts (`tools/versions/facts.json`, checked in `parity.yml`) |
| 3 | Key map across versions and rename-normalised method hashes |
| 4 | `versions.py delta`: the reusable diff between two versions |
| 5 | The committed delta for 26.2 and 26.1.2 (generated) |
| 6 | Verdicts and `versions.py check`, `todo`, `show`, `batches` |
| 7 | Bootstrapped `unreviewed` verdicts; the check in CI and `pr_ready.py`; ranged citations in the ledger |
| 8 | Per-version port ledgers (`port_ledger.py --mc-version`) |
| 9 | The version-constant lint with its baseline, and the 26.3-body-unchanged check |
| 10 | `parity.py --mc-version` and oracle overlays |
| 11 | `horizon-prepare --mc-version` (hidden), per-version CI data and steps |
| 12 | Harness scripts read the selected version instead of literal `26.3` paths |
| 13 | The pin bump keeps verdicts (`versions.py rebase` in `version_port.py regen`) |
| 14 | Docs: `docs/versions.md`, `docs/backporting.md`, the `backport` skill, `CLAUDE.md` |

From PR 7 on, nothing can drift unnoticed, before any version gate exists. PR 11 needs 26.2 and 26.1.2 imported once
on the self-hosted runners from the maintainer's own install (`lab.py prepare --mc-version`); until then those CI
steps print a notice and run nothing.

## Questions for Ludovic

- **Committing the delta.** About 1.3 MB per version of Mojang's method names and body hashes, marked
  `linguist-generated`, no code: the same kind of data as `tools/port_map/`. It is what lets the check run on forks'
  PRs without jars. Proposed: yes.
- **Older versions on the runners.** Importing 26.2 and 26.1.2 once, after PR 1 lands.
