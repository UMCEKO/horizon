# SPEC-10 (W15): entity facts I: instructions, type ids, listener guards, state groups

Read first: `DESIGN.md` here (D8, §4 port rules), SPEC-8, `tools/mod_entity_facts.py` lines 1-300 (the format in its
docstring, the constants, `instructions` .. `classify`), and the entities design's use of `entity-facts.json`
(`/home/umceko/hz-poc/agents/entities/DESIGN.md`, `EntityFacts.java` `build`). Comments in code are one line at most.

## Goal

The first half of `mod_entity_facts.py`: the bytecode helpers, the entity type ids read from vanilla's static
initialisers, the guards a listener puts on its entity (side, required and excluded types), the event-level subjects, and
the state-group classification of entity methods. SPEC-11 assembles `entity-facts.json` on top.

## Data files (D8), `crates/horizon-java-analysis/data/`

- `entity_groups.tsv`: `group \t method` rows in `GROUP_NAMES` order, one row per name of each group's space-separated list
  (a `Class#method` name stays one token). Loading builds `GROUPS` as Python's dict comprehension does: **a later row wins**
  for a repeated name. The header comment line (`# group\tmethod; the state groups of entity methods mod code calls
  (docs/java-mods.md); a name in two groups takes the later`) is skipped by the reader. One line, as every comment.
- `entity_writers.tsv`: `writer \t name` rows for `EXTRA_WRITERS`, `not \t name` rows for `NOT_WRITERS`, `prefix \t p` rows for
  `WRITER_PREFIXES` (kept in order: `startswith` with a tuple is any-of, order doesn't matter, but keep it for diffs).
- `STATE_GROUPS` (the bit order of `entities.bin`) stays a Rust `const` array: the Java side's bit order depends on it and
  it must not be edited as data.

The error text for an unclassified method stays Python's, with the new place to edit:
`entity methods without a state group (add them to crates/horizon-java-analysis/data/entity_groups.tsv):`.

## Rust, `crates/horizon-java-analysis/src/entity_facts/`

`mod.rs` (declares the modules), `bytecode.rs`, `guards.rs`, `groups.rs`. Port, each named after the Python function:
- `bytecode.rs`: `instructions` (`(offset, opcode, operand)` triples, the operand as Python computes it per opcode),
  `is_exit`, `is_aload`, `is_astore`, `slot_of_last`, `read_library_class` (first library jar holding the class, in
  `hier.jars` order), `clinit`.
- `guards.rs`: `type_ids` (the two static initialisers; `minecraft:` prefixed when the name has no `:`), `walk_guards`,
  `listener_guards`, `subjects`. A guard is `Guard { side: String, require: Vec<String>, exclude: Vec<String>, types: Option<Vec<String>> }`
  with `ALL_GUARD` as a `const fn all()`. Equality for `subjects`' de-duplication is field equality (Python compares dicts).
- `groups.rs`: `carried`, `classify` (`Class#name` by the owner's simple name first, then the bare name).

## CLI and comparison

- `horizon-java-analysis dump-entity-guards --install DIR`: `type_ids`, and for every listener SPEC-8 finds whose event
  passes the event filter of `ef.analyse` (`EVENT_PREFIXES` or a subclass of `ENTITY_EVENT`) its `listener_guards` result
  (or `null`), keyed `mod|class|method|event`.
- `java_analysis_compare.py entity-guards --install DIR`: the same from Python (`ef.type_ids`, `mr.class_listeners`,
  `ef.listener_guards`), compared as parsed JSON.

## Tests (port of the guard and group parts of `tests/test_mod_entity_facts.py`, its Java fixtures, skipped without `JAVA_HOME`)

Port each assertion with its hand-written expected values: a listener guarded by `if (e.getEntity() instanceof Zombie)`
gets `require: [Zombie]`; by `if (level.isClientSide()) return;` gets `side: server`; an unprovable guard gives `null`
(then `all` upstream); `type_ids` on fixture `EntityTypes`/`EntityTypeIds` classes maps `ZOMBIE` to `minecraft:zombie`
and a namespaced string stays as is. Plus, without Java: `classify("net/minecraft/world/entity/item/ItemEntity", "getItem")`
→ `("item", false)` (the `ItemEntity#getItem` row beats the bare name), `classify(.., "setHealth")` → `("health", true)`,
`classify(.., "setChanged")` → not a writer (`NOT_WRITERS`), `classify(.., "nothingLikeThis")` → None;
`carried("all")` is the 16 `STATE_GROUPS`.

## Checks

`cargo test -p horizon-java-analysis`, clippy, fmt.

## Done bar

`java_analysis_compare.py entity-guards`: **0 differences** on both finish installs.

## Depends on

SPEC-8 (listeners and the hierarchy).

## Live test for the lead

None.
