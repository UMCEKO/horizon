# SPEC-6 (W15): the mixin scan, without `javac` or a JVM

Read first: `DESIGN.md` here (§4 port rules), SPEC-4, SPEC-5, `tools/mod_parity.py` lines 40-166 (`lenient_json`,
`jar_mixin_classes`, `scan_install`, `parse_scan`, `mixins_markdown`, `ACCESS`), and
`mods/horizon/src/java-mods-host/parity/scan/dev/horizonlab/javamods/parity/MixinScanner.java`.
Comments in code are one line at most.

## Goal

The list of mixin handlers of an install (`mixins.json`, `mixins.md`) from Rust, equal to what `scan_install` produces
with the `javac`-compiled `MixinScanner`.

## Step 1 (Java, before porting): make the oracle deterministic

`MixinScanner.scan` writes `"CONSTANT:" + n.values`, `List.toString` of ASM's raw values. For an enum-array value
(`@Constant(expandZeroConditions = ...)`) that prints `[Ljava.lang.String;@1b6d3586`, an identity hash: two runs differ.
Replace it with a deterministic rendering, and port that:
```java
for (Object v : list(a, "constant")) if (v instanceof AnnotationNode n) at.add("CONSTANT:" + render(n.values));
```
`MixinScanner` is also compiled into the boot jar and called at runtime by `SoundOwners.java:45` and `Animate.java:231`;
check that neither reads the `CONSTANT:` text (they split the line by tabs and use the target fields), and keep it there:
SPEC-13 removes only the analysis's `javac` path, not this class.
`render(List<Object> values)`: `key=value` pairs joined by `,`, where a value is rendered by `one(Object)`:
`Type` → `getDescriptor()`, `String[]` (an enum: `{desc, name}`) → `desc + "." + name`, `List` → `[` + items by `one`
joined by `,` + `]`, `AnnotationNode` → `@` + desc, anything else → `String.valueOf`. Routing reads only the part of an
`at` before `:` (`EDGE_AT`), so routes don't change; re-run the Python analysis on both installs to refresh the oracle.

## Rust, `crates/horizon-java-analysis/src/mixins.rs`

```rust
pub struct MixinEntry { pub mod_id: String, pub config: String, pub side: String, pub mixin: String, pub handler: String, pub kind: String, pub target_class: Vec<String>, pub target_method: Vec<String>, pub at: Vec<String>, pub access: bool }
/// Port of mod_parity.jar_mixin_classes: (mod, config, side, class name, class bytes) of a jar and its nested jars.
pub fn jar_mixin_classes(zip, owner: Option<&str>, label: &str, warnings: &mut Vec<String>) -> Result<Vec<(String, String, String, String, Vec<u8>)>, String>
/// Port of MixinScanner.scan (with step 1's rendering) on SPEC-4's reader.
pub fn scan_class(mod_id: &str, config: &str, side: &str, bytes: &[u8]) -> Result<Vec<MixinEntry>, String>
/// Port of scan_install: every handler of every jar in `install/mods`, sorted by jar name as Python's glob is.
pub fn scan_install(install: &Path, warnings: &mut Vec<String>) -> Result<Vec<MixinEntry>, String>
/// Port of mixins_markdown.
pub fn mixins_markdown(entries: &[MixinEntry], classes: usize, warnings: &[String]) -> String
```
Port notes (each is a behaviour of the Java or Python the outputs depend on):
- `jar_mixin_classes` decodes with strict UTF-8 (`.decode()`), unlike `walk`: a non-UTF-8 config is an error naming the jar.
  Its nested label is the nested entry's full name (`META-INF/jarjar/x.jar`), not `walk`'s `label>stem`. The mod of a
  nested jar without a TOML is the owner's.
- `MixinScanner` reads **both** visible and invisible annotations (Mixin's are class-retention), class annotations first
  invisible then visible (`annotations(node.invisibleAnnotations, node.visibleAnnotations)`), and for methods the same order.
- `@Mixin`: each `value` (a `Class`) as `Type.getClassName()` (`a/b/C` → `a.b.C`, arrays get `[]`), then each `targets`
  string with `/` → `.`.
- A handler's kind is the annotation's simple name looked up in `KINDS` (copy the table); others are skipped.
  `methods`: each `method` string, the handler's own name for `overwrite`, then each `value` that is a string (`@Accessor("x")`).
  `at`: for each `at` annotation `value + (target == null ? "" : ":" + target)` (Java's `String.valueOf(null)` prints `null`
  when `value` is missing: keep that), then step 1's `CONSTANT:` entries.
- The handler line's class is `node.name` dotted. `access` is `kind in ("accessor", "invoker")`.
- `scan_install` keeps Python's order: jars sorted by name, each jar's configs in declared order (toml `mixins` then
  manifest `MixinConfigs`, de-duplicated keeping the first), sections `mixins`, `client`, `server`, entries in list order,
  nested jars after the jar's own, in sorted entry order.
- `mixins_markdown`: `Counter.most_common(25)` breaks ties by first appearance; sort with a stable sort on the count
  descending over the insertion-ordered list.

## Writing

`pub fn write_mixins(out: &Path, entries, classes, warnings)`: `out/mixins.json` (the entries as Python's dicts:
`mod, config, mixin, handler, kind, target_class, target_method, at, side, access`; JSON with indent 1) and
`out/mixins.md`.

## CLI and comparison

- `horizon-java-analysis mixins --install DIR --out OUT` writes both files (replaces `java_mods.py mixins`'s Python in SPEC-13).
- `java_analysis_compare.py mixins --install DIR`: runs `mod_parity.scan_install` into a temp folder and the Rust command,
  compares `mixins.json` as parsed JSON and `mixins.md` as text.

## Tests (`tests/mixins.rs`, fixtures compiled with `javac` when `JAVA_HOME` is set, else skipped)

Put stub annotation sources under `tests/java/org/spongepowered/asm/mixin/...` (`Mixin` with `value()`/`targets()`,
`injection/Inject` with `method()`/`at()`, `injection/At` with `value()`/`target()`, `injection/Constant` with
`intValue()`, `gen/Accessor` with `value()`, `Overwrite`), all `@Retention(CLASS)`. A mixin class
`@Mixin(targets = "net/minecraft/world/Base") class M { @Inject(method = "tick", at = @At(value = "INVOKE", target = "La;b()V")) void h() {}
@Overwrite void must() {} @Accessor("x") int getX(); @ModifyConstant(method = "tick", constant = @Constant(intValue = 5)) int c(int v) }`
(declare the needed stubs). Expected, by hand:
`[("h", "inject", ["tick"], ["INVOKE:La;b()V"]), ("must", "overwrite", ["must"], []), ("getX", "accessor", ["x"], []),
("c", "modify_constant", ["tick"], ["CONSTANT:intValue=5"])]`, `target_class == ["net.minecraft.world.Base"]`,
`access` true only for `getX`. A config naming a class the jar lacks adds the warning text
`"{mod}: {config} lists {name}, which is not in the jar"`.

## Checks

`cargo test -p horizon-java-analysis`, clippy, fmt; the Java change builds with `java_mods.py build` only if
`MixinScanner` is part of it (it is compiled by `scan_install` itself; run `java_mods.py mixins --install …` once).

## Done bar

`java_analysis_compare.py mixins`: **0 differences** on both finish installs, after step 1.

## Depends on

SPEC-5 (`walk`'s TOML reading and `lenient_json` are shared).

## Live test for the lead

None.
