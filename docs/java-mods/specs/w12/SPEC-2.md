# W12 SPEC-2: the routing boot fact and the route stubs

Design: `DESIGN.md` §2.2 (stub, body-routed bridges), §2.3 (admitted receivers), §2.7. Needs SPEC-1's `routing.json` and `routing.tsv`. Read first:
- `docs/java-mods-map.md` §2 ("Add a new boot fact") and §3 ("Bridged natives");
- `B/EntityFacts.java` (`confirm`, `mergedName`, `Live`);
- `H/entity_facts.rs` (reader style, `get()`);
- `J/plugin/.../BridgeProcessor.java`;
- `H/bridge.rs` (`Bridge`, `java_bridges!`).

Paths as in the map: `H/` = `crates/horizon-java-host/src/`, `B/` = `.../boot/dev/horizonlab/javamods/boot/`, `J/` = `mods/horizon/src/java-mods-host/`, `G/` = `crates/horizon-client/src/game/`.

## 1. Bridge entries can be route bodies (`H/bridge.rs`, `J/plugin/.../BridgeProcessor.java`)
- `Bridge` gets `pub route: bool`. `line()` appends an 8th column, `route` or empty.
- `java_bridges!` accepts an optional leading keyword `route` on an entry: `route "class".method "desc" [if "guard"] => fn name(game, args) -> ret { body }`. It sets `route: true`; nothing else changes.
- `BridgeProcessor` reads the 8th column (absent means false). It also reads the install's routing at construction:
  - the file is `routing.tsv` beside the path in system property `horizon.java.analysis`;
  - no property or no file means no routes;
  - lines are `owner \t name \t desc \t kind \t tail \t returns`.
- For each bridge entry whose `(owner, method, desc)` is a **body** route: don't bridge it. Print once: `Horizon: <owner>.<method> runs in Java: a mod patches inside its body`.
- For each entry with `route` set whose method is an **edge** route, or isn't routed at all, insert the **route stub** (below) in place of today's head.
- Plain entries keep today's form.

### The route stub (`static void routeStub(ClassNode, MethodNode, Entry, boolean earlyCheck)`)
`earlyCheck` is `tail && returns > 1` from `routing.tsv`; false when the method isn't routed. With `R` = a new local slot of the return type:
```text
HEAD:   <guard test as today: instanceof guard, else GETSTATIC Flat.ON>   IFEQ BODY
        <push receiver and args as today>  INVOKESTATIC holder.native  <Flat.to<T> as today>
        [non-void] xSTORE R
        [earlyCheck] INVOKESTATIC dev/horizonlab/javamods/boot/Routing.early()Z  IFEQ JOIN
        [earlyCheck] [non-void] xLOAD R   xRETURN
JOIN:   [non-void] xLOAD R
        GOTO TAIL
BODY:   <the original instructions, where the LAST return opcode (0xac–0xb1, in list order) becomes GOTO TAIL>
TAIL:   xRETURN                       ← the method's last instruction
```
- Mixin's `@At("TAIL")` targets the last return opcode in the list, so after this TAIL handlers run before `TAIL:` only. A native early return (the `[earlyCheck]` return) and the original's early returns are earlier returns: `@At("RETURN")` hits them, and TAIL doesn't. That's vanilla's semantics on both paths.
- Return `ComputeFlags.COMPUTE_FRAMES` for a class that got a route stub. Keep `COMPUTE_MAXS` otherwise. Don't hand-build frames for the stub.
- If the original body has no return opcode (it always throws), insert the head as today's plain form instead, and log it.

## 1b. Java-bodied stubs: GUI bridges (W5, `../w5/SPEC-7.md` §1)
This section was added while SPEC-2 was being implemented. If the code is already written, add it on top; nothing above changes except §2.
- **Source:** `gui_bridges.tsv`, `key \t seam`, where key is `owner#name` + descriptor. W5 writes it.
  - `BridgeProcessor` loads it with `BridgeProcessor.class.getClassLoader().getResourceAsStream("gui_bridges.tsv")`. `java_mods.py build` must put the file in the **plugin** jar: the class processor can't see the boot jar's resources. Tell W5 if its spec says otherwise.
  - Read it at construction, beside `routing.tsv`. A missing file means no rows.
- **Each row is an `Entry` with a Java body.** Generalise `bridge`/`routeStub` over two things:
  - **Backed test:** `ALOAD 0; INVOKESTATIC dev/horizonlab/javamods/boot/GuiRoutes.backed(Ljava/lang/Object;)Z; IFEQ BODY`, not the guard or `Flat.ON` test. GUI rows are never static; log and skip a static one.
  - **Body call:**
    1. push `seam` (LDC) and `this`;
    2. build an `Object[]` of the arguments, boxing primitives (`Integer.valueOf`, …);
    3. `INVOKESTATIC dev/horizonlab/javamods/boot/GuiRoutes.body(Ljava/lang/String;Ljava/lang/Object;[Ljava/lang/Object;)Ljava/lang/Object;`;
    4. unbox or `CHECKCAST` to the return type, or `POP` for `V`.

    No `Flat`, no native method added to the class, no `link`.
- **Which stub:** the same rule as JNI rows.
  - The key is **body**-routed in `routing.tsv`: no stub (the body runs in Java).
  - It is **edge**-routed: the route stub of §1, with `earlyCheck` from `routing.tsv`.
  - Otherwise: the plain head (`if backed: return body(...)`).
- `GuiRoutes` is W5's class in `B/`. Until it exists, the unit check uses a stand-in class with those two static methods: point the generator's owner name at a constant you can override in the test.
- **Unit check:** extend `RouteStubCheck` with one Java-bodied method `int twoReturns(int)`:
  - with `backed` false, the original runs;
  - with `backed` true, the stand-in's `body` value comes back, unboxed;
  - with `earlyCheck` and the stand-in calling `Routing.markEarly()`, the method returns through the early return. `markEarly` sets a Java flag, so this runs without JNI.

## 3b. Client keys and the Java lookup (`B/Routing.java`)
- `routes.bin` holds every route, client keys included (`routing.json` `side: client`).
- `confirm` on the **server** dist (`HORIZON_JAVA_DIST=server`, or the boot's dist) must not load `net/minecraft/client/` classes. Write such routes as `on = false` and don't fail on them.
- Add the Java lookup W5 calls: `static int route(String key)` (-1 when unknown or off), `static boolean edge(int id)`, `static boolean on(int id)`.

## 2. `Routing.early()` (`B/Routing.java`, `H/lib.rs`)
- `B/Routing.java`, a new class, declares `static native boolean earlyNative();`. Register it in `H/lib.rs` `register_natives`, beside `shapeFlags`. It returns and clears the Rust thread-local `EARLY` in `H/bridge.rs`.
- `static boolean early()` is plain Java: it returns and clears a thread-local `javaEarly` flag, OR-ed with `earlyNative()`. Call `earlyNative()` only when the stub's body was a JNI native, so a Java body costs no JNI call. Two separate stub calls are fine too.
- `static void markEarly()` sets `javaEarly`. Java bodies (§1b) call it for an early return.
- `H/bridge.rs` adds:
  ```rust
  thread_local! { static EARLY: Cell<bool> = const { Cell::new(false) }; }
  /// Marks a body fn's return as one of vanilla's early returns, for the route stub.
  pub fn early<T>(value: T) -> T { EARLY.set(true); value }
  pub(crate) fn take_early() -> bool { EARLY.replace(false) }
  ```

## 3. Boot confirmation (`B/Routing.java` `confirm`, called in `Boot.main` right after `EntityFacts.confirm()`)
- Read `routing.json` (sibling of `horizon.java.analysis`). No file means an empty `routes.bin`.
- For each route, in key order:
  1. `Class.forName(owner, false, Boot.class.getClassLoader())`, which makes Mixin transform it.
  2. **Applied changes:** a change counts as applied when:
     - its mixin is in `org.spongepowered.asm.mixin.transformer.ClassInfo.forName(owner).getAppliedMixins()` (match `IMixinInfo.getClassName()` to the change's `mixin`);
     - and its merged handler exists: reuse `EntityFacts.mergedName`. Move it to a package-private static helper in `Routing`, and have `EntityFacts` call it.
  3. A non-conditional change that didn't apply fails the boot: throw `IllegalStateException("route <key>: <mixin>.<handler> didn't apply")`. A conditional one that didn't apply is dropped.
  4. `on` = at least one change applied.
  5. `MethodHandle`: `MethodHandles.privateLookupIn(ownerClass, MethodHandles.lookup())`, then `findVirtual` or `findStatic` by name and `MethodType.fromMethodDescriptorString(desc, loader)`. Keep it in `Routing.HANDLES[id]` for SPEC-3. Failing to find it fails the boot.
  6. **Admitted receivers:**
     - `block`: every `BuiltInRegistries.BLOCK` entry whose class reaches the route (below), by registry name.
     - `item`: the same over `BuiltInRegistries.ITEM`.
     - `entity` and `block_entity`: write `-2` ("pending", decided by the spec that adds those receivers).
     - every other family: `-1` (all).
- **Reaches** (`static boolean reaches(Class<?> type, String owner, String name, String desc, JsonArray overriders)`):
  - Walk `type`'s superclasses to the first that declares `name`+`desc` (`getDeclaredMethods`). If it is `owner`: true.
  - If it is a vanilla class (`net/minecraft/`) listed in `overriders` with `super_call: true`: true.
  - Otherwise false. A mod class overriding the method isn't admitted: its own override routes separately.
- Print one line per route: `Horizon: route <key> <edge|body> <on|off> (<mods>)`. SPEC-4 adds the gate status.

### `routes.bin` (big-endian, as `wire::Out`)
```text
int version (1)
int n
n x ( text key, byte kind (0 edge, 1 body), text family, bool on, bool hot, int admitted (-1 all, -2 pending, else count), admitted x text name )
```
- Written by `Routing.bin()` to the path the host asks for: op `routes_bin` in `GameBridge.register`, a `Crossing.game` `byte[] -> byte[]` returning the bytes, exactly as `entity_facts`.
- `H/lib.rs` `boot()` requests it and writes `facts.with_extension("routes.bin")`, beside `entities.bin`. `Host::routes_file()` returns that path.

## 4. Rust reader (`H/routes.rs`, new; `pub mod routes;` in `H/lib.rs`)
```rust
pub const VERSION: u32 = 1;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RouteId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Edge, Body }
#[derive(Debug)]
pub enum Admitted { All, Pending, Only(std::collections::HashSet<Box<str>>) }
#[derive(Debug)]
pub struct Route { pub key: Box<str>, pub kind: Kind, pub family: Box<str>, pub on: bool, pub hot: bool, pub admitted: Admitted }
pub struct Routes { routes: Vec<Route>, ids: std::collections::HashMap<Box<str>, RouteId> }
impl Routes {
    pub fn read(bytes: &[u8]) -> Result<Self, String>;
    /// The route of `key` if it is on in this boot.
    pub fn id(&self, key: &str) -> Option<RouteId>;
    pub fn get(&self, id: RouteId) -> &Route;
    pub fn iter(&self) -> impl Iterator<Item = (RouteId, &Route)>;
}
/// The boot's routes; `None` before the boot finished or without Java mods.
pub fn get() -> Option<&'static Routes>;
```
- Set once in `boot()` (a `OnceLock`), exactly like `entity_facts::get`. Copy `entity_facts.rs`'s `Reader`, or move it to `wire.rs` and share it; don't duplicate it.
- An off route isn't in `ids`.

## 5. Content cache (`G/java_content.rs`)
- Copy `routes.bin` into the cache folder and load it from there on a cache hit, as `entities.bin`. Find `entities.bin` in that file and do the same next to it.
- `fingerprint_of` also hashes `routing.json` beside the `analysis` path. Bump `GENERATION`.

## Tests
- **Java unit `J/unit/dev/horizonlab/javamods/plugin/RouteStubCheck.java`:** if `java_mods.py unit` doesn't put the plugin classes on its classpath, add them (one line).
  - Build a class with ASM with:
    - `int twoReturns(int x)`: `if (x < 0) return -1; return x * 2;`;
    - `void oneVoid()`.
  - Apply `routeStub` with `holder` = a test class holding a static Java method in place of the native, and `Routing.early` replaced by a static field the test sets. Point `Entry` at those names; no JNI.
  - Load it with a fresh `ClassLoader`, then check:
    - with the guard off: `twoReturns(-3) == -1`, `twoReturns(4) == 8`;
    - with the guard on and the fake native returning 7: the result is 7, early or not;
    - the last instruction is `IRETURN`;
    - the original's first return is still a return; the original's last return is now a `GOTO` to the last label;
    - with `earlyCheck` there is exactly one more return opcode than without.
  - `CheckClassAdapter.verify` passes.
- **Rust `H/routes.rs` tests:**
  - a hand-written `routes.bin` round-trips;
  - an off route has no id;
  - `Admitted::Only` holds the names;
  - a short file is an error;
  - no file means `get()` is `None`.
- `cargo test -p horizon-java-host routes`, `cargo clippy -p horizon-java-host -- -D warnings`, `python3 tools/java_mods.py build && python3 tools/java_mods.py unit`.
- `python3 tools/java_mods.py check` must still pass: the macro change touches every table.

## Acceptance
- A boot of the showcase pack prints the route lines, without failing.
  - The pack is `/home/umceko/hz-poc/play/pack3`. Its install doesn't exist yet: `python3 tools/java_mods.py install --pack /home/umceko/hz-poc/play/pack3 --out /home/umceko/hz-poc/agents/w12/install` (idempotent; check `df -h /home` first, the disk was nearly full).
  - Then `java_mods.py run --install /home/umceko/hz-poc/agents/w12/install`: one run, gate first, as RULES.md says.
- Report:
  - how many routes are on and off;
  - which conditional routes dropped;
  - the boot time added (`Routing.confirm`'s wall time, printed in its last line).
- Without an analysis file, nothing changes: no routes, no stubs.

## Not in this spec
Calling routes from native (SPEC-3), the lint (SPEC-4), entity and block-entity admitted lists.
