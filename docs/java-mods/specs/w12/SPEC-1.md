# W12 SPEC-1: the routing analysis (Python only)

Design: `DESIGN.md` §1, §2.1, §2.3, §2.6, §2.7. No Rust or Java in this spec. Read `tools/mod_replaces.py` (docstring), `tools/mod_entity_facts.py` `route_facts`, `handler_method` and `library_target` first; reuse them, don't copy them.

## Why
- The `replaces` report says which vanilla methods route to Java, but not how.
- Each route needs:
  - **edge or body**;
  - the exact target descriptor;
  - which receivers it touches;
  - which native callees a body route reaches;
  - which subclass ports must also be gated.
- Two errors in today's report have to go first:
  - methods with weak ledger statuses routed `java-already`;
  - lambdas reported as their own methods.

## Deliverables
1. **New `tools/mod_routing.py`:** `routing(result, archives, hier, ledger, verdicts, vanilla_jar) -> dict`, plus the writers below. Called from `mod_replaces.analyse`, which adds `result['routing']`, and from `install_analysis`, which writes the two files beside `entity-facts.json`.
2. **`INSTALL/analysis/routing.json`** (format 1):
   ```json
   {"format": 1,
    "routes": {"net/minecraft/world/entity/player/Inventory#setSelectedSlot(I)V": {
        "kind": "edge",
        "static": false,
        "family": "player",
        "args": ["I"], "ret": "V",
        "changes": [{"mod": "carryon", "mixin": "tschipp.carryon.mixin.InventoryMixin", "handler": "onSwapPaint",
                     "kind": "inject", "at": ["HEAD"], "cancellable": true, "locals": false, "conditional": true}],
        "returns": 1, "tail": false,
        "overriders": [],
        "closure": {"bridged": [], "pure": [], "routed": [], "needs_bridge": []},
        "hot": false,
        "side": "common"}},
    "undecided": {"net/minecraft/world/entity/Entity#tick()V": {"status": "mentioned", "mods": ["waystones"]}},
    "lambdas": {"net/minecraft/world/level/levelgen/structure/templatesystem/StructureTemplate#lambda$placeEntities$0": "net/minecraft/world/level/levelgen/structure/templatesystem/StructureTemplate#placeEntities(...)V"}}
   ```
3. **`INSTALL/analysis/routing.tsv`:** one line per route, `owner \t name \t desc \t kind \t tail \t returns`, for the class processor (SPEC-2). It stubs edge routes and leaves body routes' bodies in Java.
4. **`mod_replaces.route` changes:**
   - A `mixin` on a method whose ledger status is `mentioned`, `inferred`, `absent`, `none` or `unknown` becomes routing `undecided`, unless the verdict map (`tools/port_map/*.tsv`, column 2) has a row for that `Class.method`:
     - `port` or `inline` → `java`;
     - `skip:java`, `skip:java-serves`, `skip:server`, `skip:debug` → `java-already`;
     - `missing` → `java-already`, with note `native lacks it`.
   - Add `undecided` to `ROUTING_ORDER` and to the docstring's "Routing of one change" list, one line: `undecided  a mixin on a method the ledger can't say native runs, and no verdict yet: decide it in tools/port_map`.
5. **`java_mods.py replaces --routes-out FILE`:** writes the reference-pack TSV (below). Commit the showcase one as `tools/java_routing/packs/showcase.tsv`, once the user agrees (DESIGN U3).

## Rules, exactly
### Key and descriptor
- A route key is `owner#name` + the target's descriptor, from `library_target` (already used by `route_facts`).
- When `library_target` returns more than one descriptor, the route is emitted for each.
- A key whose name starts with `lambda$` is replaced by its **enclosing method**: the method of the same class whose code holds an `invokedynamic` with a `LambdaMetafactory` bootstrap argument handle naming that lambda.
  - Read the class with `mod_replaces.Pool`/`scan_code` from the vanilla jar (`.local/minecraft/26.3/jars/server.jar`, or `client.jar` for `net/minecraft/client/`).
  - Record the mapping in `lambdas`. Nested lambdas resolve until a non-lambda.
  - A change folded from a lambda always makes its route `body`.

### Kind
`edge` when **every** change on the key is one of:
- `kind == "inject"`, every `at` in `HEAD`, `RETURN`, `TAIL`, and the handler's parameters equal the target's parameters plus one trailing `CallbackInfo`/`CallbackInfoReturnable`. More parameters means captured locals: set `"locals": true`, and the change is body;
- `kind == "modify_return_value"` (at RETURN);
- `kind == "wrap_method"`;
- `kind == "modify_variable"`, `at == ["HEAD"]`, and the annotation's `argsOnly` is true. Read it from the handler's annotation in its jar (`handler_method(...)['annos']`, as `route_facts` reads `cancellable`).

Anything else is `body`. One body change makes the whole key `body`.

### Family
From the target's owner class (vanilla hierarchy `hier.is_a`), first match:

| Owner is a | Family |
|---|---|
| `net/minecraft/world/level/levelgen/` (package prefix) or `net/minecraft/server/level/ChunkMap` | `worldgen` |
| `net/minecraft/client/` (package prefix) | `client` |
| the target method is static (any owner) | `static` |
| `net/minecraft/world/entity/player/Player` or `Inventory` | `player` |
| `net/minecraft/world/entity/Entity` | `entity` |
| `net/minecraft/world/level/block/entity/BlockEntity`, or the interface `net/minecraft/world/RandomizableContainer` | `block_entity` |
| `net/minecraft/world/level/block/Block` or `.../BlockBehaviour$BlockStateBase` | `block` |
| `net/minecraft/world/item/Item` | `item` |
| `net/minecraft/world/level/Level` | `level` |
| `net/minecraft/world/scores/Scoreboard` | `scoreboard` |
| `net/minecraft/world/inventory/AbstractContainerMenu` | `menu` |
| otherwise | `value` (a receiver that is plain data: `LootTable`, `ItemStack`, `MobEffectInstance`, `Shapes`) |

### Returns and tail
- `returns`: the number of return opcodes (`0xac`–`0xb1`) in the vanilla method's code.
- `tail`: true if any change has `at` `TAIL`.

### Overriders
- For each vanilla subclass S (any class in the vanilla jar with `hier.is_a(S, owner)`) that declares the same name and descriptor, add `{"class": S, "super_call": bool}`.
- `super_call` is true when S's code holds `invokespecial owner.name desc`, or an `invokespecial` to an intermediate superclass's override that itself super-calls (follow the chain).

### Closure (body routes only)
- Walk the vanilla call graph from the key, breadth-first, through the vanilla jar's `invokevirtual`/`invokespecial`/`invokestatic`/`invokeinterface` targets.
- Resolve a virtual target to its declaring class with `hier`.
- Classify each reached method, and stop walking at it unless it is `pure`:
  - `routed`: a key of this report's `routes`; Java runs it, and its own stub bridges when edge.
  - `bridged`: a method in the bridge table. Read the `java_bridges!` entries from `crates/horizon-client/src/game/java_bridges/*.rs` with the regex `java_mods.py check` already uses (import it, don't copy it).
  - `needs_bridge`: the declaring class is a view-family class (Entity and below, `Level` and below, `BlockEntity` and below, `Inventory`, `Scoreboard`, `MinecraftServer`, `PlayerList`, `ServerPlayer`), the ledger says native owns it (`OWNED`), and it isn't bridged.
  - `pure`: anything else. Keep walking; cap the walk at 2,000 methods per route and record `"truncated": true`.
- A `needs_bridge` entry doesn't fail the analysis. It shows in `replaces.md` and in the boot report, so the route's target spec adds the bridge.

### Hot
`hot` is true when the key, or an `edge` key's owner, is reached from one of these within 3 calls in the vanilla call graph:
- `Entity#tick()V`
- `Entity#baseTick()V`
- `LivingEntity#aiStep()V`
- `Entity#move(Lnet/minecraft/world/entity/MoverType;Lnet/minecraft/world/phys/Vec3;)V`
- `ServerLevel#tick(Ljava/util/function/BooleanSupplier;)V`

### Side
`common` unless every change's `side` is `client` (then `client`) or `server`.

### Reference-pack TSV (`--routes-out`)
- One line per route: `key \t kind \t family \t tail \t returns \t mods (comma) \t conditional (0/1)`.
- Sorted, with a header line `# routes of <install dir name>, mod_routing format 1`.
- No handler names, no mod class names.

## Tests (`tests/test_mod_routing.py`)
Build tiny jars with `javac` the way `tests/test_mod_entity_facts.py` does: a fake vanilla class tree and fake mixin classes with `@Mixin`/`@Inject` annotations stubbed. Cases:
1. HEAD inject with `CallbackInfo` → `edge`; with one captured local → `body`, `locals: true`.
2. TAIL inject → `edge`, `tail: true`; `returns` counts two returns.
3. `wrap_operation` at INVOKE → `body`; a HEAD inject plus a `wrap_operation` on the same key → `body`.
4. `modify_variable` HEAD `argsOnly` → `edge`; without `argsOnly` → `body`.
5. A mixin into `lambda$run$0` → the route is `Class#run(...)` and `body`, listed in `lambdas`.
6. Overriders: S overrides with `super` → `super_call: true`; T overrides without → `false`.
7. Closure: a body route calling a bridged method (fake bridge table), a view-family owned method (`needs_bridge`), and a value method (`pure`).
8. `undecided`: status `mentioned` with no verdict row → `undecided`; with a `port` row → `java`.

Also run `python3 -m unittest tests.test_mod_replaces tests.test_mod_entity_facts` (they must still pass).

## Acceptance on the showcase pack
- Run `python3 tools/java_mods.py replaces --install /home/umceko/hz-poc/showcase-pack --out /home/umceko/hz-poc/agents/w12/replaces`. Report in your summary:
  - routes by kind and family;
  - `undecided` count (expect about 85 changes over about 70 keys);
  - folded lambdas;
  - hot routes;
  - the `needs_bridge` list of the T1 keys.
- Spot checks:
  - `Inventory#setSelectedSlot(I)V` is `edge`, family `player`;
  - `Inventory#getFreeSlot()I` is `body`;
  - `LootTable#fill(...)V` is `body`, family `value`;
  - `HopperBlockEntity#getBlockContainer` is `edge` (wrap_method), `static`;
  - `Entity#isInWater()Z` is `hot`.

## Not in this spec
Boot, Rust, Java, the lint. Don't edit `tools/port_map/*.tsv` verdicts: listing `undecided` is the deliverable, and the user decides them (DESIGN U7).

## Report
The numbers above, the files changed, the tests run.
