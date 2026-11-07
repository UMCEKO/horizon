# SPEC-6-2: a mod type spawns, ticks, tracks and saves as its native base kind

Design: `DESIGN.md` §8 ("Native: the base kind… Overrides are routes"), decision 1. Index: `SPEC-2-8.md`. Size: ~300 lines. Needs 6-1 and phase 2.

## Change
- **Native value:** every kind's struct gains `type_id: TypeId` (a small interned id; vanilla kinds already know their type per variant: make it a field where it isn't). A mod type's entity is its base kind's struct with the mod `type_id`, the mod type's dimensions and attachments (6-1), and its attributes for living bases. Every place that derives behaviour from the type (`MobType` lookups, box size, tracking range, `kind(&e.kind)`, NBT `id`) reads `type_id`.
- **Spawning:** `spawn_loaded_entities` (chunk load, `/summon`, spawn eggs, 1c's `EntityViews.spawn`) builds the base kind for a mod type id with the base's own loader (the vanilla keys of the base) and `type_id` set; the Java-owned keys (the mod's own `addAdditionalSaveData` keys) stay opaque in `persist.raw` (1c) and reach Java at join through `entity_view`'s tag, which builds the **mod class** instance (`EntityType.create`): the view is the mod's object.
- **Saving:** native saves the base kind's keys plus `persist.raw`; at each save, 1c's `entities_save` asks Java for the Java-owned keys — extend it to a mod type's own keys: Java runs the mod class's `addAdditionalSaveData` into a `TagValueOutput`, and returns the keys the base class doesn't write (diff of key sets against the base's `saveWithoutId`, computed by Java on the view). Native merges them into `persist.raw`.
- **Tracking:** the add-entity packet uses the mod type's network id; the base kind's data items are sent as now; mod data items are 6-4. A `complexSpawn` type is 6-4 too.
- **Ticking:** the turn of a mod-typed entity is its base kind's turn, until 6-3 routes the overridden methods.
- **Client (native):** a mod-typed entity renders as nothing until phase 8 (no vanilla model is guessed for it); its box shows with F3+B. List it in the user steps.

## Tests
- `game_tests`: `a_mod_tnt_spawns_as_tnt_with_its_type_id` (a fact for `horizon_java_test:test_tnt` with base `PrimedTnt`: `/summon` makes a `Kind::Tnt` whose `type_id` is the mod's and whose fuse counts down); `a_mod_type_saves_its_java_keys` (round trip through `persist.raw` with a fake answer).
- Test mod: `test_tnt_summons_natively` (`/summon horizon_java_test:test_tnt` → `level.getEntity` finds a `TestTnt` view); `test_tnt_saves_mod_keys` (`afterChange`: a mod field written in `later` is back).
- `python3 tools/parity.py tnt server_entities` unchanged.
