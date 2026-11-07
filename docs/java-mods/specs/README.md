# Java mods (NeoForge 26.3 layer): the remaining work, as specs

These are the design and spec files behind the Forge-layer issues on UMCEKO/horizon; the tracker lists every piece.
Each `SPEC-*.md` is one claimable issue. Read the folder's `DESIGN.md` first, then the spec.

- **Base branch:** `java-mods`. Branch from it and open the pull request into it, with `Fixes #N`.
  The branch is being ported onto arismoko/horizon main (`port/java-mods`); the layer's files come over verbatim, so work on `java-mods` is carried.
- **How the layer works:** `docs/java-mods.md` on `java-mods`. One JVM runs the mods' server and client code against live views of Horizon's world; Horizon (Rust) is the game and draws everything.
- **Rules:** `RULES.md` here, plus the repo's `CLAUDE.md` and `CONTRIBUTING.md`. The ones reviews block on:
  - clean room: never read the source or decompiled classes of NeoForge or of any mod; `javap`, runtime traces and public docs only (vanilla's decompiled sources are fine);
  - native first: static analysis of the jars, then capture at boot or world load; Java at runtime only for mod code that really runs per call, gated on a listener;
  - no per-mod code, no polling, no mirroring, no fallbacks; all rendering native;
  - one-line comments, tests with signal, no hardcoded player-facing strings.
- **Paths in the specs:** `/home/umceko/...` and `live.sh` are the maintainer's machine; use your own scratch homes. Short-hands (`G/`, `B/`, `J/`...) are defined at the top of each `DESIGN.md`. Specs name java-mods hashes from when they were written; adapt to the current code.

| Folder | Lane |
|---|---|
| `w5` | W5: mods' additions to vanilla screens and the HUD (JEI, EMI) |
| `w1-outline` | W1: block-outline overrides |
| `w3` | W3: mods' particles |
| `w4` | W4: camera, FOV, fog, movement |
| `w6` | W6: commands, chat, toasts |
| `w7` | W7: mod fluids, sky, weather |
| `w8` | W8: remaining client details |
| `w10` | W10: mod worldgen |
| `w11` | W11: recipes, loot, crafting |
| `w12-coverage` | W12: mixin routing coverage (`w12` holds the routing mechanism's own design, already built) |
| `w13` | W13: server events |
| `w15` | W15: install, dedicated server, release |
| `entities` | W9 server entities and W2 client entities (`SPEC-5-*`, `SPEC-8-*`) |
