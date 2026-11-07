# SPEC-4 (W15): the `horizon-java-analysis` crate and its class-file reader

Read first: `CLAUDE.md`, `/home/umceko/hz-poc/agents/RULES.md`, `DESIGN.md` here (G5, D5, D6 and **the port rules in §4**).
The Python being ported is Horizon's own code (`tools/`): read it freely. Clean room still holds for mods and NeoForge:
the comparison reads their jars as data, nobody reads their sources. Comments in code are one line at most.

## Goal

A new crate in the game workspace that will hold the whole mod analysis (SPEC-4..11). This spec adds the crate, its
class-file reader (a port of `tools/mod_replaces.py` lines 81-286), a small CLI, and the dev comparison tool every later
spec extends.

## Crate

- `crates/horizon-java-analysis/Cargo.toml`: `edition = "2024"`, `publish = false`; deps `zip = { version = "9.1.3",
  default-features = false, features = ["deflate"] }`, `serde = { features = ["derive"] }`, `serde_json`, `flate2` (as the
  workspace pins them), `sha2`, `toml = "1.1.6"`. No `jni`, no `horizon-*` dependency.
- Root `Cargo.toml` `members`: add `"crates/horizon-java-analysis"`.
- `src/lib.rs`: `pub mod class;` (more modules come with later specs). Crate doc: one line, "Which vanilla behaviours the
  mods of a Java mods run folder change, read from their jars (docs/java-mods.md)."
- `src/main.rs`, bin `horizon-java-analysis`: subcommands are added per spec. This one: `dump-classes JAR...` prints one
  JSON object per class file of each jar (entries ending `.class`, not under `META-INF/`, not `module-info.class`, in the
  zip's entry order), in the shape below. A class that fails to parse prints `{"entry": ..., "error": true}` (Python
  skips on `ValueError, IndexError, struct.error`; record it so the comparison sees the same set).

## `src/class.rs`: port of `u2`, `u4`, `Pool`, `element`, `annotation`, `read_annotations`, `LENGTHS`, `op_length`, `scan_code`, `parse_class`, `params`, `is_extension`, `is_vanilla`

```rust
pub struct Pool { /* entries by index: Utf8 (as String, decoded like Python's 'utf-8','replace'), Integer(i32), Float(f32), Long(i64), Double(f64), Class(u16), String(u16), MethodType(u16), Module(u16), Package(u16), Ref{tag,class,nat}, NameAndType{name,desc}, Dynamic{tag,bootstrap,nat}, MethodHandle{kind:u8, reference:u16}; Long and Double take two slots */ }
pub enum Value { Str(String), Enum { desc: String, name: String }, Class(String), Const(Const), Anno(Annotation), Array(Vec<Value>) }
pub enum Const { Int(i32), Long(i64), Float(f32), Double(f64), Str(String) }
pub struct Annotation { pub desc: String, pub values: Vec<(String, Value)> }
pub enum Event { Static { owner: String, name: String }, Class(String), Call { owner: String, name: String, desc: String }, Indy { bootstrap: u16, name: String, desc: String } }
pub struct Method { pub access: u16, pub name: String, pub desc: String, pub annos: Vec<Annotation>, pub invisible: Vec<Annotation>, pub events: Vec<Event>, pub code: Option<Vec<u8>> }
pub struct ClassInfo { pub name: String, pub sup: Option<String>, pub ifaces: Vec<String>, pub access: u16, pub methods: Vec<Method>, pub annos: Vec<Annotation>, pub invisible: Vec<Annotation>, pub bootstraps: Vec<(u16, Vec<u16>)>, pub pool: Pool }
pub struct Parse { pub members: bool, pub annotations: bool, pub code: bool }
pub fn parse_class(data: &[u8], what: Parse) -> Result<ClassInfo, ClassError>
pub fn params(desc: &str) -> Vec<String>
pub fn is_extension(name: &str) -> bool
pub fn is_vanilla(name: &str) -> bool
```
Differences from the Python, all on purpose and invisible to its consumers:
- `Value` keeps what Python drops (an enum's type, a non-`Z` constant, invisible annotations), because SPEC-6's mixin scan
  needs them. `fn python_value(&Value) -> serde_json::Value` renders what Python's `element` returns: `Str` and `Enum` as
  the string (enum: the constant's name only), `Class` as its descriptor, `Anno` as `[desc, {name: value}]`, `Array` as a
  list, `Const` as the int when the element tag was `Z` and the entry an Integer, else `null` (keep the tag in `Const`'s
  context: store `Value::Const` with the tag char; Python reads `entry[1] if tag == 'Z' and entry[0] == 3 else None`).
- Python's annotation dicts keep the **last** value of a repeated key and the **first** position: build `values` the
  same way when rendering (`python_value` of an `Annotation`), but keep the raw list in the struct.
- `annos` is only `RuntimeVisibleAnnotations` (as Python); `invisible` is `RuntimeInvisibleAnnotations` (new).
- Errors where Python raises: an unknown constant tag (`ValueError`), reading past the end (`IndexError`/`struct.error`):
  one `ClassError` with a message. Bounds-check every read.

The dump shape (Python's dict, `pool` and `code` left out):
`{"name","super","ifaces","access","methods":[{"name","desc","access","annos":{desc:{name:value}},"events":[["call",owner,name,desc]|["static",owner,name]|["class",name]|["indy",bootstrap,name,desc]]}],"annos":{...},"bootstraps":[[i,[args]]]}`.

## `tools/java_analysis_compare.py` (new, dev only; SPEC-13 deletes it)

`python3 tools/java_analysis_compare.py classes --install DIR [--bin target/fast/horizon-java-analysis]`: for every jar in
`DIR/mods` (and every jar nested under `META-INF/jarjar/`, extracted to a temp folder) and the patched client jar
(`mod_parity_explain.patched_jars(DIR/libraries)[0]`), run the Rust dump and Python's
`mod_replaces.parse_class(data, members=True, annotations=True, code=True)` rendered in the same shape (tuples as lists),
and compare parsed objects. Print `N classes, M differences` and the first 20 differences (`jar!entry: path: python vs rust`).
Exit non-zero on any difference. Later specs add stages to this script (`libraries`, `jars`, `mixins`, `replaces`,
`routing`, `entity-facts`).

## Tests (`crates/horizon-java-analysis/tests/class.rs`)

Fixture classes are compiled at test time from Java sources kept in `crates/horizon-java-analysis/tests/java/` (copy
`SOURCES` from `tests/test_mod_replaces.py` into files, plus one `fixture/Annos.java` below). A helper finds `javac` as
`$JAVA_HOME/bin/javac` and otherwise prints `skipped: no JAVA_HOME` and returns (the Python tests do the same). Expected
values are written by hand from the Java source (and match `javap -v`):
- `fixture/One`: `sup == "net/minecraft/world/Thing"`, methods in declaration order include `tick ()V`, `tick (J)V`,
  `must ()V`, `friction ()I`, `own ()V`, `<init> ()V`.
- `fixture/Listeners` constructor events contain `Indy { name: "accept", desc: "(Lfixture/Listeners;)Ljava/util/function/Consumer;" , .. }`
  then `Call { owner: "net/neoforged/bus/api/IEventBus", name: "addListener", .. }`, and a `Static` of
  `net/neoforged/bus/api/EventPriority.LOW`.
- `fixture/Annos.java`:
  `@Retention(CLASS) @interface Inv { String[] value(); }` and `@Retention(RUNTIME) @interface Vis { java.lang.annotation.ElementType e(); Class<?> c(); boolean z(); int i(); }`,
  a class annotated `@Inv({"a","b"}) @Vis(e = ElementType.FIELD, c = String.class, z = true, i = 7)`: `invisible[0].values ==
  [("value", Array([Str("a"), Str("b")]))]`; `annos[0]` has `Enum { desc: "Ljava/lang/annotation/ElementType;", name: "FIELD" }`,
  `Class("Ljava/lang/String;")`, `python_value(z) == 1`, `python_value(i) == null`.
- `params("(ILjava/lang/String;[[JLa/B;)V") == ["I", "java/lang/String", "[[J", "a/B"]`.
- `op_length` on a hand-assembled `tableswitch` at offsets 0 and 1 (padding differs): lengths computed by hand from the JVM
  spec (§6.5 tableswitch): with low 0, high 2 at offset 0: `1 + 3 + 12 + 12 = 28`.
- A truncated class (first 20 bytes of `One.class`) is an error, not a panic.

## Checks

`cargo test -p horizon-java-analysis`, clippy `-D warnings`, fmt; build the bin with `--profile fast`.

## Done bar

`java_analysis_compare.py classes` reports **0 differences** on `/home/umceko/hz-poc/play/install-rs` and on
`/home/umceko/hz-poc/agents/w12/install` (paste the two summary lines in the report).

## Depends on

Nothing (can land beside SPEC-1).

## Live test for the lead

None: nothing a player sees changes. The lead re-runs the comparison on both installs.
