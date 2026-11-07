# SPEC-2-1: what NeoForge's hooks read and write, from the jar

Index: `SPEC-2-8.md` (E4). Size: ~250 lines. Depends on nothing in phase 2. Python, Java boot fact, Rust reader.

## Why
Phase 2 fires its events by calling NeoForge's own hook on a view (E4). The hook runs NeoForge code on that view: whatever it reads must be fresh, and whatever it writes must reach native. SPEC-0 knows this for mod code (`reads`, `writes`), but not for NeoForge's hooks, because `analyse` scans mod jars only. Nobody may read NeoForge's code, so a tool answers, as SPEC-0 does for mods. `onLivingBreathe`, for one, replaces vanilla's whole air block (javap of the patched `LivingEntity.baseTick`, offset 231), so its writes are vanilla's `setAirSupply`, `hurtServer` and so on, done from NeoForge's code.

## 1. `tools/mod_entity_facts.py`
**`HOOKS`**, a table in the tool, one line per hook key (descriptor from `javap -s` of the NeoForge universal jar, which `loader_archive(hier)` already finds):
```python
HOOKS = {
    'entity_tick_pre': 'net/neoforged/neoforge/event/EventHooks#fireEntityTickPre(Lnet/minecraft/world/entity/Entity;)Lnet/neoforged/neoforge/event/tick/EntityTickEvent$Pre;',
    'entity_tick_post': 'net/neoforged/neoforge/event/EventHooks#fireEntityTickPost(Lnet/minecraft/world/entity/Entity;)V',
    'mob_despawn': 'net/neoforged/neoforge/event/EventHooks#checkMobDespawn(Lnet/minecraft/world/entity/Mob;)Z',
    'living_breathe': 'net/neoforged/neoforge/common/CommonHooks#onLivingBreathe(Lnet/minecraft/world/entity/LivingEntity;Lnet/minecraft/server/level/ServerLevel;II)V',
    'start_tracking': 'net/neoforged/neoforge/event/EventHooks#onStartEntityTracking(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/entity/player/Player;)V',
    'stop_tracking': 'net/neoforged/neoforge/event/EventHooks#onStopEntityTracking(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/entity/player/Player;)V',
}
```
Later phases add lines (phase 4: damage, death, drops; phase 5: client hooks).

**`hook_closure(hier, key) -> {"reads": [groups], "writes": [{key, receiver, group}], "events": [event classes]}`:**
- Walk the method's code with `instructions`, then every method it calls whose owner is under `net/neoforged/` (not `net/neoforged/bus/`), depth first, each method once.
- A call whose owner is an entity class (`entity_receiver`) is classified with `classify` exactly as mod calls are: `entity_key`, its group, and whether it writes. An unmapped name goes to `unclassified` and fails the run, as for mods.
- A `new` of a class that `event_classes(hier)` lists is recorded under `events` (the hook posts it).
- `invokeinterface`/`invokevirtual` on `net/neoforged/bus/api/IEventBus#post` stops the walk there: listeners are mod code, already analysed.
- `invokedynamic` lambdas: follow the implementation handle (as `class_listeners` does).
- The output holds **only method keys and groups**: no code, no names of NeoForge internals beyond the call keys.

**Output** in `entity-facts.json`, format 2 (bump `"format"`):
```json
"hooks": {"living_breathe": {"reads": ["air", "flags", ...], "writes": [{"key", "receiver", "group"}], "events": ["net/neoforged/.../LivingBreatheEvent", ...]}}
```
- Each hook's `writes` also join the top-level `writes` with `"mods": ["neoforge"]`, so `java_mods.py check --analysis` requires their bridges like any mod writer.
- `events` tells which event classes a hook posts. `admits` for a hook is the OR over those events (`LivingBreatheEvent` **or** `LivingDrownEvent` if the tool finds both behind `onLivingBreathe`).

## 2. Boot fact (`B/EntityFacts.java`, `entities.bin` version 2)
After the events block, append: `int nHooks` × (`text hook`, `int readsMask`, `int nEvents` × `int eventIndex`). Event indices are positions in the events block; an event a hook posts that has no listener isn't in the events block and is skipped.

## 3. Rust (`H/entity_facts.rs`)
- `enum Hook { EntityTickPre, EntityTickPost, MobDespawn, LivingBreathe, StartTracking, StopTracking }` in `HOOKS` order, read by name (an unknown name is skipped, a missing one is "never admitted").
- `facts.hook_reads(Hook) -> u32` and `facts.hook_admits(Hook, type_id, client) -> bool`: the OR of `admits` over the hook's events, precomputed per type at load like `admits`.
- Version 1 files are refused with "re-run the analysis (entity facts format 2)": the content cache rebuilds them because the analysis file is in the fingerprint.

## Tests
- `tests/test_mod_entity_facts.py`, javac-built classes standing in for a hook (a class under `net/neoforged/...` in the temp jar):
  - `a_hook_calling_set_air_supply_lists_the_writer`;
  - `a_hook_reaching_a_helper_follows_it` (two levels of NeoForge static calls);
  - `the_walk_stops_at_post` (a call after `IEventBus.post` on a listener path isn't followed into mod code);
  - `the_events_a_hook_constructs_are_listed`;
  - `an_unmapped_entity_call_in_a_hook_fails`.
- `cargo test -p horizon-java-host entity_facts` (a version-2 file round-trips; `hook_admits` ORs two events; version 1 refused).
- The pack run (`python3 tools/mod_replaces.py --install /home/umceko/hz-poc/agents/parity-tools/run2/horizon/install --out OUT`): `hooks` has all six; `unclassified` is empty. Report each hook's `writes` list in your report: steps 2-16 to 2-18 bridge exactly that list.
- `python3 tools/java_mods.py check --analysis OUT/entity-facts.json`: the new writers are listed as missing bridges (expected until their steps); nothing else changes.

No user steps.
