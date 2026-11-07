# SPEC-0-analysis: entity facts from the jars, confirmed at boot

Design: `DESIGN.md` §0 decision 5, §1.4, §5. Implement this first: every later entity phase reads its output. Clean room: the tool reads class files (javap-level metadata and bytecode), never sources.

## Goal
One versioned boot fact answers, per mod set, with no runtime discovery:
1. which entity events fire at all, and for which subjects (side, entity classes, entity types), from each listener's provable guards;
2. which state groups views of each entity class must carry, and which vanilla entity writers need a bridge;
3. per entity class, which vanilla methods are changed: edge mixins (with handler names), body mixins, and own-type overrides;
4. entity capability registrations for vanilla types.

What can't be proven statically is confirmed once at boot, as listed under "Boot confirmation".

## Install time: `tools/mod_entity_facts.py` (new), called from `tools/mod_replaces.py`
### 1. Wiring
- At the end of `mod_replaces.analyse`, call `mod_entity_facts.analyse(classes, archives, hier, entries, result)`. `classes` and `archives` are the lists it already builds; keep the parsed `info` of every class the entity analysis needs by re-parsing with `code=True`.
- `mod_replaces.run` writes its return as `OUT/entity-facts.json` beside `replaces.json`.
- `tools/java_mods.py install` runs the same analysis into `INSTALL/analysis/entity-facts.json`, and writes an `analysis` line into `horizon-java.args` naming it.
- `G/java_content.rs` `fingerprint_of` adds the file's bytes, so a new mod set rebuilds the boot fact.

### 2. A small bytecode walker
`instructions(code) -> list[(offset, opcode, operands)]`, reusing `scan_code`'s length table (`LENGTHS`, `tableswitch`/`lookupswitch`/`wide` handling). Operands are:
- the constant-pool index for `getstatic`, `invoke*`, `instanceof`, `checkcast`, `ldc`;
- the local index for `aload`/`astore` (including `_0`..`_3`);
- the absolute branch target for `if*` and `goto`.

`is_exit(code, target)` is true when the instruction at `target` is `return` (0xb1), or a `goto` to one.

### 3. Listener guards
`class_listeners` gains the **implementation method** of each listener:
- `@SubscribeEvent`: the method itself.
- `addListener(lambda or method reference)`: the bootstrap's second argument, a method handle `(class, name, desc)`, which `lambda_event` already reads.

`guards(info_of_impl, method) -> {"side": "server"|"client"|"both", "require": [cls], "exclude": [cls], "types": [id] | null}`: a symbolic walk of the method's **prefix**. The event is the last parameter; its local is the last parameter slot (static vs instance aware).

**Values on the stack and in locals:**
- `EV`: the event.
- `ENT`: `EV.getEntity()` (any no-argument `getEntity` on the event's class or a supertype).
- `LVL`: `ENT.level()` or `EV.getLevel()`.
- `CS`: `LVL.isClientSide()`.
- `TYPE`: `ENT.getType()`.
- `T(id)`: `getstatic net/minecraft/world/entity/EntityTypes.<FIELD>` (or the 26.3 holder class the vanilla jar uses), mapped to its registry id by parsing that class's `<clinit>` (`ldc "name"` … `putstatic FIELD`). Never by lowercasing.
- `INST(c)`: `instanceof c` on `ENT`. `checkcast` keeps the value.

**Guard shapes** (the branch's taken side must satisfy `is_exit`):
- `CS ifne exit` → server. `CS ifeq exit` → client.
- `INST(c) ifeq exit` → require `c`. `INST(c) ifne exit` → exclude `c`.
- `TYPE T(id) if_acmpne exit` → types `[id]`. Also `T(id) TYPE` order; `equals`/`is` calls are out of scope, so the walk stops there.

**The walk stops**, keeping the guards found so far (they dominate the rest), at:
- the first instruction outside `aload`/`astore`/`dup`/`pop`/`checkcast`/`instanceof`/`getstatic` of `EntityTypes`;
- a recognised invoke;
- a recognised branch;
- any branch whose taken side isn't an exit.

**Result:** none found means `{"side": "both", "require": [], "exclude": [], "types": null}` ("all"). A listener whose implementation can't be resolved is `all`, and is counted in `unresolved_listeners`, as today.

**Event-level filter** (`events[e].subjects`): the union of its listeners' admissions.
- If any listener is `all`, the filter is `all`.
- Sides union. `require` becomes a list of alternatives, one per listener: `(require, exclude, types)`.

**Expected on the pack:** `EntityTickEvent$Post` → `[{"side": "server", "require": ["net/minecraft/world/entity/LivingEntity"], "exclude": ["net/minecraft/world/entity/player/Player"], "types": null}]`.

### 4. State groups and writers
- For every mod method's `call` events whose owner `hier.is_a(owner, "net/minecraft/world/entity/Entity")`, excluding owners under `net/minecraft/world/entity/player/` and `net/minecraft/server/level/ServerPlayer` (players have their own views): `key = hier.declarer(owner, name, desc) + "#" + name + desc`; `receiver = owner`.
- `GROUPS` is a dict in the tool, method name (or `Class#name` when names collide) → `(group, write)`.
  - **Groups:** `identity` (id, UUID, type, level: always carried), `position` (position, block position, box, eye, rotation, head and body yaw), `motion`, `health` (health, max health, alive, dying, hurt, heal), `effects`, `equipment` (slots, hands, armour), `fire`, `air`, `flags` (crouching, sprinting, swimming, silent, glowing, invisible, no-gravity, on-ground, in-water, in-lava), `name` (custom name, tags), `item` (`ItemEntity` stack, pickup delay, owner, target), `riding` (vehicle, passengers), `removal` (removed, reason), `java` (`getData`/`setData`/`hasData`/`getPersistentData`/`getCapability`: Java-owned, never refreshed), `pure` (no entity state: `level`, `registryAccess`, `getType`'s descriptor-only helpers, `playSound`, which is an effect, not state).
  - **Writers:** name prefix `set`, `add`, `remove`, `hurt`, `kill`, `discard`, `teleport`, `push`, `knockback`, `heal`, `ignite`, `extinguish`, `startRiding`, `stopRiding`, `snapTo`, `moveTo`, `absSnapTo`, `lerpTo`, `resetFallDistance`, `clearFire`, plus explicit entries.
- **Unmapped methods:** a key whose name isn't in `GROUPS` goes to `unclassified`, and the tool exits non-zero listing them. Grow the table until the pack (Mekanism, AE2, Jade, BG2, guideme) has none; each entry is a reviewable line. Never default a group.
- **Output:**
  - `reads[class] = sorted groups`, per receiver class. A group read on `Entity` or `LivingEntity` applies to every subclass. Rust resolves by the type's class chain at boot.
  - `writes = [{key, receiver, group, mods}]`.

### 5. Routes per entity class
- From `result["methods"]`, keep the keys whose class is `Entity` or a subclass, or a renderer or model under `net/minecraft/client/renderer/entity/` or `net/minecraft/client/model/`.
- **Mixins:**
  - `edge` when `kind == "inject"`, every `at` is in `HEAD`, `RETURN`, `TAIL`, and the handler's descriptor (read from the mixin class in its jar: `e["mixin"]` and `e["handler"]`) equals the target's arguments plus one trailing `CallbackInfo` or `CallbackInfoReturnable`, with no captured locals.
  - Anything else is `body`.
  - Record `{mixin, handler, at, cancellable}`. `cancellable` is the `@Inject(cancellable=true)` annotation value.
- **Overrides:** `{class: [vanilla keys]}`, from `override` and `implement` records.
- **Output:** `routes[vanilla_class] = {"edge": {key: [handlers]}, "body": [keys]}` and `overrides[mod_class] = [keys]`.

### 6. Capabilities on vanilla types
- A `call` to `net/neoforged/neoforge/capabilities/RegisterCapabilitiesEvent#registerEntity` whose type argument is a `getstatic` of the vanilla types class (§3's map): `caps[type_id] += [capability field]`.
- Any other `registerEntity` call goes to `caps_unresolved` (count per mod) for boot.

### 7. `entity-facts.json` (format 1)
```json
{"format": 1, "events": {"pkg/Event": {"listeners": [{"mod","class","method","guards"}], "subjects": "all" | [filters]}},
 "reads": {"pkg/Class": ["position", ...]}, "writes": [{"key","receiver","group","mods"}],
 "routes": {"pkg/Class": {"edge": {"key": [{"mixin","handler","at","cancellable"}]}, "body": ["key"]}},
 "overrides": {"pkg/ModClass": ["key"]}, "caps": {"minecraft:cow": ["..."]}, "caps_unresolved": {"mod": n},
 "opaque": ["mod: why"], "unclassified": [], "unresolved_listeners": {"mod": n}}
```

### 8. Tests: `tests/test_mod_entity_facts.py`
Build tiny class files with a hand-assembler (bytes for the constant pool plus `Code`), or compile Java snippets with `javac` into a temp folder if a JDK is on `PATH` (`MixinScannerCheck` already compiles in `test_mod_parity`). Prefer `javac`.

**Cases:**
- Mekanism's exact shape (client return, `instanceof LivingEntity`, `instanceof Player`, then an unknown call) → server, require `LivingEntity`, exclude `Player`.
- Type comparison → types `[minecraft:zombie]`.
- A guard after an unknown call is not taken.
- A branch to non-exit code stops the walk.
- A lambda listener's guards come from its implementation.
- An unmapped entity method fails with its name.
- Edge vs body classification of injectors, including local capture → body.
- `registerEntity` with a vanilla static → `caps`.

**Pack run:**
```
python3 tools/mod_replaces.py --install /tmp/hz-poc/agents/parity-tools/run2/horizon/install --out OUT --mixins /tmp/hz-poc/agents/parity-tools/run2/mixins.json
```
It must report `EntityTickEvent$Post` as above, `unclassified: []`, the three edge mixins of DESIGN §1.4, and the writers there.

## Boot confirmation: `B/EntityFacts.java` (new), run in `Boot.main` after `BootFacts.seal`
- **Reads** the file named by the `analysis` line (`Host` passes it as a system property, as `HORIZON_JAVA_RUN` lines are passed).
- **Listeners:** for each event class in the file, plus every `EntityEvent` subclass loaded, take `EventBus.getListenerList(Class)` by reflection (private, found by name with `javap -p net.neoforged.bus.EventBus`), then `.getListeners().length`.
  - An event the file lacks but the bus has listeners for (registered dynamically) is added with subjects `all`.
  - An event the file lists with no bus listener (a conditional registration not taken) is dropped.
  - Each difference is logged once: `Horizon: entity facts: <event> listened at runtime only` or `... not listened at runtime`.
- **Conditional mixins:** a route whose mixin is `conditional` in `replaces.json` is kept only if a method whose name ends with `$` + handler exists in the target class (`getDeclaredMethods`). The same lookup resolves the merged name of every edge handler. A route whose handler isn't found fails the boot (`defaultRequire` style).
- **Writes:** every writer key must be in the bridge table (`Host.publish`'s list, which Java has), or in `tools/java_mods_skips/*.tsv` with reason `java-owned`. Otherwise boot logs `Horizon: entity facts: no bridge for <key> (<mods>)`. `java_mods.py check` fails on the same condition, statically.
- **Attachment types:** `NeoForgeRegistries.ATTACHMENT_TYPES`; those with a serializer are listed.
- **Output** `entities.bin` (`Host::entity_facts` file, beside `keys.tsv` in the content cache folder). `DataOutputStream`, big-endian:
  - `int version` (1);
  - `int nMethods` × `text key`;
  - `int nClasses` × (`text class`, `int reads mask`, `int nEdge` × (`int method`, `int nHandlers` × (`text merged name`, `byte at` 0 HEAD / 1 exit, `bool cancellable`)), `int nBody` × `int method`);
  - `int nOverrides` × (`text modClass`, `int n` × `int method`);
  - `int nEvents` × (`text event`, `bool all`, `int nFilters` × (`byte side`, `int nRequire` × `text`, `int nExclude` × `text`, `int nTypes` × `text`));
  - `int nCaps` × (`text type`, `int n` × `text cap`);
  - `int nSerializedAttachments` × `text`.

  Group bits, in this order: `identity`=1, `position`=2, `motion`=4, `health`=8, `effects`=16, `equipment`=32, `fire`=64, `air`=128, `flags`=256, `name`=512, `item`=1024, `riding`=2048, `removal`=4096.
- **Mod entity types' classes** need a level, so they are resolved at the first `server_start` (`EntityFacts.types(level)`). Each owned `EntityType` is built once with `EntityType.create(level, EntitySpawnReason.LOAD)`, its class chain is recorded, and it is discarded. The result is written once as `entity-types.bin` in the same folder (`int n` × (`text type`, `int depth` × `text class`)). Vanilla types' classes come from `BuiltInRegistries.ENTITY_TYPE` the same way, once.

## Rust: read once
- `G/java_content.rs`: after the boot, read `entities.bin` into `horizon_java_host::EntityFacts` (a new struct in `H/entity_facts.rs`, held in a `OnceLock`). Read `entity-types.bin` at the first server start.
- **API:**
  - `facts.route(type: &str, method: MethodId) -> Option<&Route>`, precomputed per type at load: the class chain's routes and overrides OR-ed into one bitset per type;
  - `facts.admits(event: EventId, type: &str, client: bool) -> bool`, precomputed per type and event as one bit;
  - `facts.reads(type) -> u32`.
  - `MethodId` and `EventId` are indices found once by key (`facts.method("net/minecraft/world/entity/item/ItemEntity#tick()V")`) and kept by the callers in statics.
- **Unit tests in `H/entity_facts.rs`:** a hand-written `entities.bin` round-trips; class-chain OR-ing; `admits` with require and exclude; a missing file is "no facts" (no mods: nothing routes, nothing fires).

## Checks for the parent
- `python3 -m unittest tests.test_mod_entity_facts`.
- The pack run (above) matches DESIGN §1.4. `unclassified` is empty.
- `cargo test -p horizon-java-host entity_facts`.
- Boot with the pack. The log has no `no bridge for` lines (until SPEC-1b they are expected for its writers, and `java_mods.py check` lists them; that is the point). The content cache folder has `entities.bin`, and `entity-types.bin` after a world opens.
- No user steps.

## What stays undecidable, and how boot settles it

| Unknown | Settled by |
|---|---|
| Listeners registered with computed classes, in config-gated code, or by reflection | Bus listener lists at boot (dynamic ones get `all`) |
| Guards behind calls the walker doesn't know | `all` for that listener (sound) |
| Conditional mixins (config plugins) | Merged-handler presence in the target class |
| Mod entity type → class | One instance per type at first server start, cached |
| Capabilities on mod types | `caps_unresolved` → read from NeoForge's entity capability providers after `RegisterCapabilitiesEvent`: field found with `javap -p net.neoforged.neoforge.capabilities.EntityCapability`, by reflection, at the first server start, cached with the types |
| Mod code reaching entity state by reflection or method handles | Not provable. It is listed per mod as `reflective_entity_access` (a `call` to `java/lang/reflect` or `MethodHandles` in a class that also references an entity class), and boot warns once per mod. Never guessed |
| Coremods and class processors | `opaque`: entity routing refused for that pack, loudly, at boot (DESIGN question 1) |
