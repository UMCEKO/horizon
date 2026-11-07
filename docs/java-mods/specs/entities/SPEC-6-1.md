# SPEC-6-1: mod entity types as boot facts

Design: `DESIGN.md` §8 (boot facts: type ids, base vanilla class, dimensions, tracking, attributes, overrides, mod data items, complex spawn data), decision 1 (mod types are native as the nearest vanilla kind they subclass). Index: `SPEC-2-8.md`. Size: ~280 lines. Needs SPEC-0's `entity-types.bin` (built at the first server start from one instance per type).

## Java (`B/EntityFacts.java` `types(level)`, extended; `entity-types.bin` version 2)
For each **owned** entity type (`BootFacts.owns` over `BuiltInRegistries.ENTITY_TYPE`), from the `EntityType` and the one instance already built:
- `text id`, the class chain (as now), and **`text base`**: the first vanilla class (`net/minecraft/`) in the chain;
- `MobCategory`, `width`, `height`, `eyeHeight` (`type.getDimensions()`), `fixed`, the attachments (1c2c-1's exporter layout, from `type.getDimensions().attachments()`), and every `static final EntityDimensions` field of the mod class chain (babies, poses), as 1c2c-1 does for vanilla;
- tracking: `clientTrackingRange`, `updateInterval`, `trackDeltas`; `fireImmune`, `canSerialize`, `canSummon`, `canSpawnFarFromPlayer`, `spawnDimensionsScale`;
- for a `LivingEntity` base: default attributes (`DefaultAttributes.getSupplier(type)`, every attribute id and base value);
- **mod data items:** the instance's `SynchedEntityData` items whose accessor id is above the base vanilla class's last id (vanilla's ids are fixed per class; `javap -p` the accessor fields of the base to count them; or record the id range each class defined by reading `EntityDataAccessor` static fields of each class in the chain): `int id`, `text serializer` (`EntityDataSerializers.getSerializedId`, NeoForge's registry name for mod serializers);
- `bool complexSpawn` (`instanceof IEntityWithComplexSpawn`);
- the overrides of vanilla methods, from `entity-facts.json` `overrides` for each class of the chain (SPEC-0), as method keys.
The instance is discarded as now.

## Rust (`H/entity_facts.rs` `EntityTypes`, read at the first server start)
`ModEntityType { id, base: BaseKind, category, dims, attachments, babies/poses, tracking, flags, attributes, data_items, complex_spawn, overrides: MethodBits }`, `BaseKind` from the base class: `PrimedTnt` → `Tnt`, `ItemEntity` → `Item`, `ThrowableProjectile`/`AbstractArrow`/`AbstractHurtingProjectile`/`Projectile` → `Projectile` (with the projectile family), `Mob`/`PathfinderMob`/a vanilla mob class → `Mob` (with that species when the base is a vanilla species: a `Creeper` subclass is a creeper), `LivingEntity` → `Mob` with species `Living` (no AI: phase 7 adds goals), `AbstractMinecart`, `AbstractBoat`, `Entity` → `Other`, and so on for every `Kind` (phase 2 made every vanilla kind one). A base with no `Kind` is listed `unsupported base <class> (<types>)` in the boot log and isn't spawnable natively; name each in the report.

Native's type tables (`mobs.rs` `MobType` from `entity-types.json`; the per-kind tables) gain the mod types from this fact, so `/summon <mod type>`, tracking (`kind(&e.kind)` → network type id from the client's synced registry) and spawn eggs find them.

## Tests
- `cargo test -p horizon-java-host entity_facts` (version 2 round-trip; `BaseKind` for a `PrimedTnt` subclass and a `PathfinderMob` subclass).
- Test mod: register `horizon_java_test:test_tnt` (`TestTnt extends PrimedTnt`, overriding `tick` to call `super.tick()` and count) and `horizon_java_test:test_mob` (`TestMob extends PathfinderMob`, no goals yet; one mod data item `DATA_GLOW` boolean). Boot: the log lists both with bases `PrimedTnt`, `PathfinderMob`; `entity-types.bin` has `DATA_GLOW`'s id and serializer.
- `java_mods.py build`, `bridges`.
