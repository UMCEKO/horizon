# SPEC-4: admission narrowed by provable handler guards; inert by proof

Design: `DESIGN.md` §2.3, decision C3. Needs W9 3-1 for the entity half (admitted entity types); the item and plain-class halves land without it. Size: ~400 changed lines (Python ~200, Java ~120, Rust ~80).

Paths as SPEC-2; `T/` = `tools/`. Read first: `T/mod_entity_facts.py` (`walk_guards`, `listener_guards`, entities `SPEC-0-analysis.md` §3), `T/mod_replaces.py` (the routing part: `changes`, `closure`, `overriders`, how `routing.json` is written), `B/Routing.java` (`admitted`, `reaches`, `confirm`), `H/routes.rs`, `G/java_routing.rs` (`Gate`).

Clean room: the walker reads bytecode of handler methods as a tool, like the entities analysis; nobody reads it by hand. Comments in code: one line at most.

## Why
A mixin on `Entity#isInWater` or `Entity#tick` admits every entity type by class chain; on `EnchantmentHelper#getItemEnchantmentLevel` (static) it admits every call. javap shows the handlers act on a subset: Travelers Backpack's `EntityMixin` and `ItemEntityMixin` both have `private boolean travelersbackpack$isBackpack()` beside the handlers; AE2's `hookGetItemEnchantmentLevel(Holder, ItemInstance, CallbackInfoReturnable)` is about its own tools. A provable guard admits only that subset, which is the difference between a route on every entity every tick and a route on a dropped backpack.

## 1. The walker (`T/mod_replaces.py`, using `T/mod_entity_facts.walk_guards`)
For each mixin change of a routed key, walk the merged handler's prefix (the handler method in the mixin class, before merge; the receiver is `aload_0` for instance handlers, the mixin's `this`) with these values on the stack and in locals:
- `THIS`: `aload_0` in an instance handler.
- `ARG(i)`: the i-th parameter of the target method (the handler's leading parameters).
- `ITEMSTACK(v)`: `v.getItem()` where `v` is `THIS` of an `ItemEntity` mixin, or `ARG(i)` of type `ItemStack`/`ItemInstance` (`ItemStack.getItem()` and `ItemInstance.getItem()` both map).
- `ITEM(v)`: `ITEMSTACK(v).getItem()`; `Holder.value()` of `ItemInstance.typeHolder()` maps the same.
- `BLOCK(v)`: `v.getBlock()` where `v` is a `BlockState` argument (or `this` of a `BlockStateBase` mixin).
- `BLOCK_AT(l, p)`, `BE_AT(l, p)`: `l.getBlockState(p).getBlock()`, `l.getBlockEntity(p)`, where `l` is a level-typed argument (`Level`, `LevelReader`, `BlockGetter`…) and `p` a `BlockPos` argument.
- `INST(v, C)`: `instanceof C` on any of the above, or on `ARG(i)` itself (`checkcast` keeps the value).

Guard shapes (the taken side must be an exit, as in `walk_guards`; for a `CallbackInfo`/`CallbackInfoReturnable` handler an exit is a `return` that doesn't touch the callback):
- `INST(THIS, C) ifeq exit` → receiver class `C`;
- `INST(ITEM(THIS), C) ifeq exit` → receiver's item class `C`;
- `INST(ITEM(ARG(i)), C) ifeq exit` → argument i's item class `C`;
- `invoke{special,virtual} <mixin class>.<private no-arg boolean helper>()Z; ifeq exit`, where the helper's whole body is one of the shapes above ending in `ireturn` of the `instanceof` result (or a conjunction `&&` of two): inline it once, depth 1.

- `INST(ARG(i), C) ifeq exit` → argument i's class `C` (a native caller never passes a mod class: such a guard on a mod `C` makes the route inert for native, §2);
- `INST(BLOCK(ARG(i)), C)`, `INST(BLOCK_AT(l, p), C)`, `INST(BE_AT(l, p), C)` `ifeq exit` → argument block class, block-at-position class, block-entity-at-position class `C`.

The walk stops where `walk_guards` stops. Anything unrecognised: no guard (`all`). Result per change: `{"recv": [C…] | null, "recv_item": [C…] | null, "arg_item": {"i": [C…]} | null, "arg_class": {"i": [C…]} | null, "arg_block": {"i": [C…]} | null, "block_at": [C…] | null, "be_at": [C…] | null}`. A route's admission is the union over its changes; one change with no guard makes the route `all`. Write it into `routing.json` as `route["guards"]`; `routing.tsv` is unchanged.

## 2. Boot (`B/Routing.java`)
`admitted(...)` today returns a list for `block` and `item` families and null otherwise. Add:
- **Receiver class guard** `recv`: keep only receivers whose Java class `isAssignableFrom` one of the classes. For `entity` (W9 3-1's list of entity types by their real class, mod types included, since their views are their own class), `block`, `item` families: filter the list. For a family with no list (value, static, menu, scoreboard…): if no vanilla class of the family's native receivers can be an instance of a `C` (that is: every `C` is outside `net/minecraft/` and not a superclass of a vanilla class), the route admits nothing.
- **Item guards** `recv_item`, `arg_item`: the item ids whose `Item` class `isAssignableFrom` one of the classes, mod items included (their stacks are native-held). Written as a new list.
- **Block guards** `arg_block`, `block_at`, `be_at`: the Horizon block state ids (through `STATES`) whose block (or block entity type's class) matches. Written as a second new list, `int blocks` (-1 = any) and that many `int` Java state ids; Rust maps them to its ids at read.
- **Argument class guards** `arg_class`: a `C` outside `net/minecraft/` that is no superclass of a vanilla class admits nothing (native passes only vanilla objects); otherwise no narrowing.
- **Inert by proof:** a route that is `on` but whose admitted list (or item list) is empty is written with admitted `-3` (new value: none). The boot line reads `on inert (guard)` instead of `UNGATED`/`gated`; `summary(...)` leaves it out. `java_routing.py report` counts it under covered, as a verdict.

`routes.bin` goes to version 2: after each route's admitted list, `int items` (-1 = any) and that many `text` item ids.

## 3. Rust (`H/routes.rs`, `G/java_routing.rs`)
- `Admitted::None` for `-3`; `Route.items: Option<HashSet<Box<str>>>`. `Routes::read` accepts version 2 only (the content cache's fingerprint already covers `routing.json`, so a stale cache rebuilds).
- `Gate::admitted` returns `None` for `Admitted::None` (cached as off, state 1).
- The `route!` macro takes optional trailing `item = <expr>` (`&str`, the stack's item id) and `block = <expr>` (`BlockStateId`: the argument state, or the state at the position for `block_at`/`be_at`, which the caller reads natively), checked after the receiver: `if let Some(items) = &route.items && !items.contains(item) { not admitted }`, and a bitset test for blocks. Cost: one hash lookup or one bit test on routes that have such a list, nothing on others. A route with a block or item list reached from a call site that passes neither panics (`Horizon: route <key> admits by item/block but its gate at <file:line> passes none`): the gate can't be exact there, which is a seam bug, not a case to admit everything.
- `Recv::Entity` (from W9 3-1) is filtered by its admitted entity types as 3-1 specifies; the receiver class guard only shrinks that list at boot.

## 4. Tests
- Python (`tests/test_mod_routing.py`): compile small mixin-shaped classes with `javac` in the test (as the existing routing tests do) and assert the walker's result, values written out:
  - `isBackpack` helper shape: `private boolean isBackpack() { return ((Object) this) instanceof ItemEntity e && e.getItem().getItem() instanceof FakeBackpackItem; }` called by `if (!isBackpack()) return;` → `recv: ["net/minecraft/world/entity/item/ItemEntity"], recv_item: ["…/FakeBackpackItem"]`;
  - a static handler `(Holder h, ItemInstance s, CallbackInfoReturnable c)` with `if (!(s.getItem() instanceof FakeTool)) return;` → `arg_item: {"1": ["…/FakeTool"]}`;
  - a handler that logs before its `instanceof` → `all` (the walk stops at the unknown call);
  - two changes on one route, one guarded and one not → route `all`;
  - `(LevelReader l, BlockPos p, CallbackInfoReturnable c)` with `if (!(l.getBlockState(p).getBlock() instanceof FakeChestBlock)) return;` → `block_at: ["…/FakeChestBlock"]`;
  - a static `(VoxelShape a, VoxelShape b, …)` handler with `if (!(a instanceof FakeShape)) return;` → `arg_class: {"0": ["…/FakeShape"]}`, and the boot side makes it inert.
- Java (`U/RoutingGuardsCheck.java`, new): `admitted` with a `recv` guard of a mod-only class on a value family returns empty → `-3`; an item guard of class `BucketItem` over the real item registry admits exactly the items vanilla registers with `BucketItem` or a subclass of it (`MobBucketItem`…). Write that list out in the check by reading `.local/minecraft/26.3/sources/server/net/minecraft/world/item/Items.java` (the independent oracle), not by running the code under test.
- Rust: `an_item_list_admits_only_its_items` (hand-built `Routes` v2); `none_admission_never_crosses`.
- Run `python3 -m unittest tests.test_mod_routing tests.test_java_routing`, `cargo test -p horizon-java-host routes`, `cargo test -p horizon-client --features java-mods --lib java_routing`, `java_mods.py build`, `unit`.

## Done bar
- Showcase analysis (`java_mods.py replaces` on `agents/w12/install`): report, per route, its guards. Expected from javap alone: travelersbackpack's `isInWater`, `isInvulnerable`, `onBelowWorld`, `ItemEntity#tick`/`fireImmune`/`ignoreExplosion`/`setItem` get `recv_item`; if the walker gives `all` for them, report the instruction where it stopped (from the tool's own debug output, not by reading the class) instead of widening the shapes by guess.
- `SimpleContainer#setChanged`: report each mod's change's guard; it is inert by proof only if all five are guarded by mod classes. Otherwise it stays SPEC-17's (DESIGN Q1).
- A showcase boot prints `inert (guard)` for exactly the routes the analysis predicts.

## For the lead
No gameplay change by itself (no gate is added). One boot of the showcase install; compare its `inert (guard)` lines with the implementer's report. No user steps.
