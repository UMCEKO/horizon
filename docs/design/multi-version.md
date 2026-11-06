# Older Minecraft versions beside 26.3

A design note, before any code: how Horizon plays older releases from the same branch as the pinned 26.3, what stays
shared, what it costs a player who only plays 26.3, and how 26.3 parity stays safe while it happens. Agreed with
Ludovic on 2026-11-05: it goes into main if it works, as this note and then small PRs. Issue
[#9881](https://github.com/horizon-rust/horizon/issues/9881).

Why: older versions have the modpacks and worlds players have. Not covered here: a 26.3 client talking to older
servers by translation (`crates/horizon-protocol/src/adapter/`, #8718), which is a different feature and stays as it
is; world downgrades.

**Status: not yet proven.** An earlier prototype chose the version at run time: its 26.2 copy of
`LivingEntity.hurtServer` matched the 26.2 oracle on 442,462 lines, but it cost a load and a branch per call, which is
why this note chooses at compile time instead. The zero-cost claims below are the design's requirements, not
measurements. A proof of concept (below) has to show them before any tooling PR opens.

## Which versions

The highest release of each version line, newest first, each step reusing the one before it:

26.2, 26.1.2, 1.21.11, 1.20.6, 1.19.4, 1.18.2, 1.17.1, 1.16.5, 1.15.2, 1.14.4, 1.13.2, 1.12.2, 1.11.2, 1.10.2, 1.9.4,
1.8.9, 1.7.10.

Other releases where many modpacks sit may join after the 26.x tiers, based on a real count of packs per version.
Inserting one later needs no change to other versions' code. Nothing in the design assumes 1.7.10 is the floor.

## Rules

1. **Vanilla 26.3 stays the spec for 26.3.** Every 26.3 harness keeps passing, unchanged, on every PR.
2. **Unused, it costs nothing.** A player who only plays 26.3 downloads, stores, allocates, starts and reads nothing
   new, and a 26.3 build has no added or changed instructions.
3. **One engine.** Only behaviour that differs between versions is split, and data is preferred to code.
4. **Vanilla of that version is the spec for that version.** No future behaviour on past versions, unless it is a bug
   fix in shared code.

## How versions coexist

**The version is chosen when the game is compiled.** A build reads `HORIZON_MC_VERSION` (unset means the pin) into a
`const MC: McVersion` in `horizon-core`, with that version's facts (protocol, data version, pack formats). Each
supported version is its own binary, and nothing asks the version at run time.

**Prepared data per version.** `horizon-prepare` builds `.local/horizon-client/26.2` from the player's own 26.2 install,
as it does for 26.3. Registries, tags, recipes, loot tables, worldgen data, block states, models and lang come from
that version's files, so most differences never reach code. Versions before the data generator (1.13) need exporters
that read the same facts out of that version's game.

**One function per version, cut clean.** Where an older version's logic differs, it gets its own copy of the whole
function. The 26.3 function gains one attribute line:

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

- In a 26.3 build `#[versioned]` expands to the function exactly as written, and nothing else: no wrapper, table,
  branch or hidden helper. In a 26.2 build it expands to a direct, inlinable call to the copy.
- **A copy compiles only in its own version's build.** A 26.3 build never sees it. A per-version `cargo check` covers
  it in CI.
- A copy is a child module of the file it replaces, so it reaches the parent's private helpers and nothing else can
  call it. Callers never change: they call the 26.3 fn, which routes.
- `versions.py fork Class.method --mc-version 26.2` writes the copy, the attribute and the citation; the porter edits
  only the lines that differ. Going back a version forks the nearest newer copy.
- Copies are whole, but nobody reads them whole: `versions.py diff Class.method` shows a copy against the body it was
  forked from, and `versions.py log Class.method` shows a method's history from oldest to newest.
- Deleting a version deletes its `v…` files, their `mod` lines and their arms.

**Whole subsystems.** Where an era differs by more than functions (block storage before 1.13, rendering models before
1.8), `#[versioned]` swaps a whole module the same way. Older data maps onto the current internal form where that
reproduces the old behaviour exactly. There is still one engine and one branch.

**Plain values.** A constant, threshold or table that differs becomes a per-version value:

```rust
pub const ZOMBIE_FIRE_CHANCE: f32 = versions! {
    ..=V26_2 => 0.3,
    V26_3 => 0.4,
};
```

`versions!` expands to the selected arm only. Both macros share one match-like syntax: arms are versions or inclusive
ranges (`..=V26_2` is 26.2 and everything older), checked at compile time like a `match`: every version covered once,
no `_`, and the pin named in its own arm. A 26.3 line may be edited to read such a value as long as that adds no
instructions, which CI checks. Logic goes in a copy, not a value.

**State only an older version has.** Fields and enum variants that exist only in older versions use `#[cfg(...)]`, on
declarations and struct-literal fields only, so they don't exist in a 26.3 build. `#[cfg]` on control flow is rejected
by a lint: that goes in a copy.

**Protocol tables per version.** Packet ids, registry ids and entity-data layouts come from generated tables per
version, selected by the build's version, not from `if`s in codecs. Packets whose fields changed get a codec per
version range.

**Worlds and connections.** Each binary uses its own version's world format and protocol, so vanilla of that version
can open its worlds and join its servers. Older worlds upgrade through that version's datafixers, as in vanilla.

**Names.** One set of names everywhere: the official ones. From 26.1 the jars carry them. Mojang's mappings give them
from 1.14.4 to 1.21.11, and stay in `.local` because they may not be redistributed complete and unmodified. Below
that, [Feather](https://github.com/OrnitheMC/feather) (CC0) covers 1.7.10 to 1.14.4, linked to the official names
through 1.14.4's obfuscated names. A method with no official counterpart keeps its Feather name. Renames between
versions are inferred from bodies, with a short hand-written override file. `lab.py prepare` remaps the player's jars
locally to these names, so oracles read like 26.3's.

**Editor support is a requirement.** Version names are real constants and `#[versioned]` keeps bodies' spans, so
completion, hover and go-to-definition work. Copies are inactive code in the default 26.3 mode, so a porter runs
rust-analyzer with `HORIZON_MC_VERSION` set. The proof of concept has to show that this works, and a rust-analyzer
fixture test guards it.

## What stays shared

Everything else: the renderer, chunk storage, meshing, light, streaming, the world generator's machinery, entity
physics, the server thread, LAN and the dedicated server, the launcher and mods. Each supported version is one build,
one binary per platform.

**A fix in shared code reaches every version that uses it.** Only a copy or a `versions!` arm locks behaviour to a
version. A failing older harness then means vanilla really differs there, and the fix is a copy, not reverting the
26.3 change.

**Mods declare their versions.** Content mods and native hooks are built against 26.3's registries and hook catalog,
so a mod's manifest lists the versions it supports (`minecraft = ["26.3"]` by default). A world of another version
refuses to load a mod that doesn't list it, and the registry index shows the list.

**UI and visuals.** Gameplay, worlds and protocol are harness-exact. UI and visuals use that version's assets and
layouts through shared code, with a module swap per screen where the difference is visible. UI gaps are issues, not
part of a version being done.

## What it costs a 26.3-only player

Nothing:

- **No download, no extra data.** Older versions are never bundled. The first time a player picks one in the launcher,
  it prepares that version from its own install in the player's Minecraft launcher. Horizon still never downloads the
  game.
- **No allocation, thread, file read, branch or added instruction.** Older versions' arms, copies and fields aren't
  in a 26.3 build.
- **No new requests.** A launcher with only 26.3 doesn't look at older versions' artifacts.
- **No new choice.** The picker only offers versions the player has installed; 26.3 stays the default.
- **Same ownership gate.** The picker keeps the Microsoft sign-in and entitlement check for every version, and offers a
  version only when the official launcher installed it. Nothing on Horizon's side fetches an older jar, CI included:
  the runners import from the maintainer's own install.

Compile time isn't counted: a player building native mods compiles a 26.3 build, which has no copies in it anyway,
apart from the small macro crate.

## How 26.3 stays safe

- **26.3 harnesses unchanged and passing.** A backport PR never edits a 26.3 harness's expected output.
- **Same instructions.** CI compares a release build's instructions per function against the merge base, ignoring
  addresses, symbol hashes and function order. Any added, removed or changed instruction fails. It runs on PRs that
  change version markup (attributes, `versions!`, `mod v…` lines, the macro crate), and those PRs contain nothing else.
  A tick benchmark runs nightly as a backstop.
- **Version-constant lint.** `tools/version_lint.py` flags protocol and data version numbers, pack formats, version
  strings, `.local/.../26.3` paths, tables in registry order and numeric id constants written outside the generated
  `version.rs`, the older-server adapter and the datafixer. It lands with a baseline of what main has today, so it
  starts green and the count only goes down. It also rejects `#[cfg]` on control flow.
- **A method delta per version in progress.** `tools/versions/delta/<version>.tsv` lists every non-trivial vanilla
  method whose bytecode differs from the next newer supported version: names and 12-hex body hashes only, no Mojang
  code. Generated, never edited by hand, checked against the jars in `parity.yml`.
- **Verdicts.** For each ported method in a delta, `tools/versions/verdicts/*.tsv` records one verdict per distinct
  older body: `same`, `ported-differently` (naming the Rust fn), `not-applicable`, `todo` (with an issue) or
  `unreviewed`. Each row carries both bodies' hashes, so it goes stale the moment vanilla changes under it.
  `tools/versions.py check` fails on a missing, stale or invalid verdict; it runs offline in `verify` and with the full
  ledger in `parity.yml`.
- **The next pin bump is a finite list.** When the pin moves, only the step from 26.3 to the new pin needs review.
  `version_port.py report` prints that number before porting starts.

### What a 26.3 contributor sees

- A version joins this system only when porting on it starts, so rows appear only for versions in progress.
- If they newly port a method whose body differs in such a version, `tools/pr_ready.py` writes an `unreviewed` row and
  they commit that generated line.
- If they rename a helper or change a signature that copies use, it must compile on those versions. `pr_ready.py` makes
  the mechanical edits, and CI runs that `cargo check` only for such PRs and only for versions with copies. This is the
  one case where an older version can block a 26.3 PR.
- If they fix a bug in a body that has copies, they do nothing. A tool files a backport issue listing each copy.

No other 26.3 PR is blocked by an older version.

### CI budget

On a PR, older-version harnesses run only when the PR touches version code for that version, and only the harnesses
selected for those files; the 26.3 parity job is unchanged. Versions in progress run nightly. Released versions rotate,
a few per night, and each also runs before its release job. Harnesses opt in per version (`PARITY_VERSIONS = "26.2.."`);
`tools/parity.py --mc-version 26.2` runs only those, with per-version oracle overlays in
`mods/horizon/src/versions/<version>/java/`. Each version uses its own Java (`java_major` in `research-lock.json`).

## Backporting by agents

The work is large (3,666 vanilla methods changed from 26.2 to 26.3, 1,462 of them ported; 5,455 and 2,148 from 26.1.2)
and mostly mechanical, so it is meant to be done by agents with no prior context. Every decision is a rule, a tool
output or a CI failure, never judgement.

**Batches.** `versions.py batches --mc-version V` groups `unreviewed` rows: rows ported by the same Rust fn, then rows
with the same old body, at most ~15 rows or ~400 lines. Order: protocol and registries, then `horizon-core`,
`horizon-world`, gameplay by package, then client-only; within a package, most-called first. `versions.py batches --open
N` files the next N as issues (`Backport V: <package> <fn>`). An agent takes work only by `/claim` on one of them, under
the usual claim rules.

**The loop, per row** (a `backport` skill beside `vanilla-parity`):

1. `versions.py show KEY --mc-version V`: both bytecode bodies and their diff, both decompiled bodies from the player's
   own jars (printed, never written into the repository), the Rust fn that ports it and the harnesses that cover it.
2. Classify with the decision table below.
3. If code is needed: a per-version value for a constant, or `versions.py fork` for logic, then edit the copy.
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
| A constant, threshold, table or registry entry differs | `ported-differently` | a per-version value, never an `if` |
| Control flow or a call differs | `ported-differently` | a per-version copy (`versions.py fork`) |
| The subject doesn't exist in Horizon (dev tool, telemetry, a removed online service) | `not-applicable` | none, a one-line reason |
| A stop condition | `todo #N` | none |

**Stop conditions.** The agent writes `todo #N` with the reason, comments on the batch issue, `/unclaim`s what it can't
finish and moves on when: the fix would change a 26.3 build's instructions or a 26.3 harness's expected output; no
harness can observe the behaviour and none can be written from the template; it needs a subsystem Horizon doesn't have;
the batch passes ~500 lines or 3 crates; or the two versions' harness results disagree in a way the diff doesn't
explain. Those rows form a `needs-design` queue for a person.

**Review.** CI is the gate: per-version harnesses, 26.3 harnesses, `versions.py check`, the version lint and the
instruction comparison. A second agent, not the author, re-derives each verdict from `versions.py show` without seeing
the author's. People spot-check a random 10% of `same` rows and every `not-applicable` row. Before scaling up, a pilot
of ~20 batches measures how often the reviewer disagrees, and the table is tightened where it does.

## Delivery

- **Proof of concept, before any tooling PR.** A partial backport: a real 26.2 binary from `HORIZON_MC_VERSION`, with
  one fork (`LivingEntity.hurtServer`), one `versions!` value and one 26.2-only field, matching the 26.2 oracle. The
  26.3 harness stays green, the instruction comparison is clean and rust-analyzer works in 26.2 mode. One time-boxed
  attempt to prepare 26.2 data and open a world offscreen lists what breaks. A failure here changes this design, and
  the result gets posted on this PR.
- **Sizing study, with the proof of concept.** The size of each step down to 1.7.10, and how far name linking holds,
  with no porting.
- **Tier 0: tooling.** The PRs below. Nothing changes for players.
- **Tier 1: 26.2, partial by partial.** Small batch PRs on main. *Done* means no `unreviewed` or `todo` rows and every
  harness that declares it passes.
- **Each older version after it, in the same way.** Rows whose body matches the newer version's take its verdict and
  code.
- **The launcher picker last.** It gains a version when that version is done, 26.2 first. Before that, no older
  release builds exist and developers build them locally. After that, a separate job builds them once 26.3 has
  published, and it can't hold 26.3 up: a failure leaves that version on its last good build.

### Tooling PRs

Each under ~500 lines with tests, `skip-release` (players see nothing). Generated data is counted apart.

| # | PR |
|---|---|
| 1 | Older versions in `research-lock.json` (`backports`); a selected version and local root in `lab.py` |
| 2 | Supported versions' facts (`tools/versions/facts.json`, checked in `parity.yml`) |
| 3 | Key map across versions and rename-normalised method hashes |
| 4 | `versions.py delta`: the reusable diff between two versions |
| 5 | The committed delta for 26.2 (generated) |
| 6 | Verdicts and `versions.py check`, `todo`, `show`, `log`, `batches` |
| 7 | Bootstrapped `unreviewed` verdicts; the check in CI and `pr_ready.py`; ranged citations in the ledger |
| 8 | Per-version port ledgers (`port_ledger.py --mc-version`) |
| 9 | The version-constant lint with its baseline, and the instruction comparison |
| 10 | `parity.py --mc-version` and oracle overlays |
| 11 | `horizon-prepare --mc-version` (hidden), per-version CI data and steps |
| 12 | Harness scripts read the selected version instead of literal `26.3` paths |
| 13 | The pin bump keeps verdicts (`versions.py rebase` in `version_port.py regen`) |
| 14 | Docs: `docs/versions.md`, `docs/backporting.md`, the `backport` skill, `CLAUDE.md` |

PR 11 needs 26.2 imported once on the self-hosted runners from the maintainer's own install (`lab.py prepare
--mc-version`), and each older version when its porting starts. Until then those CI steps print a notice and run
nothing. Jar remapping and Feather's data come with the first obfuscated version, not in this list.

## Questions for Ludovic

- **Committing the delta.** About 1.3 MB per version of Mojang's method names and body hashes, marked
  `linguist-generated`, no code: the same kind of data as `tools/port_map/`. It is what lets the check run on forks' PRs
  without jars. Proposed: yes.
- **Older versions on the runners.** Importing 26.2 once, after PR 1 lands, and each older version when it starts.
- **A version nobody fixes.** No policy yet. A version whose release job keeps failing stays on its last good build.
- **Merging.** Merging this note is its own decision, and each tooling PR after it is reviewed on its own.
