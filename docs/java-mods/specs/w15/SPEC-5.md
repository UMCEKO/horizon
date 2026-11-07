# SPEC-5 (W15): libraries, hierarchy, jar walk and jar facts

Read first: `DESIGN.md` here (§4 port rules), SPEC-4, `tools/mod_replaces.py` lines 288-392 and 459-516,
`tools/mod_parity.py` `lenient_json` (line 40), `tools/mod_parity_explain.py` `patched_jars` (line 131).
Comments in code are one line at most.

## Goal

Port the analysis's view of an install's jars: which library jars it reads, the class index built from them (cached), the
class hierarchy, the transformation SPI names, and per mod archive (nested jars included) its mod id, label, data
overrides, access-transformer lines, opaque transformers and mixin config plugins.

## Rust, `crates/horizon-java-analysis/src/`

### `libraries.rs`
- `pub fn patched_jars(libraries: &Path) -> Result<Vec<PathBuf>, String>`: port of `mod_parity_explain.patched_jars`,
  including its error rule (`missing[:3] and len(missing) > 1` → error naming the missing patterns) and "the last sorted
  match" per pattern. Glob by walking: `net/neoforged/minecraft-client-patched/*/*.jar`,
  `net/neoforged/minecraft-server-patched/*/*.jar`, `net/neoforged/neoforge/*/neoforge-*-universal.jar`,
  `net/neoforged/fancymodloader/loader/*/loader-*.jar`. Python sorts paths: compare as `PathBuf` (component-wise), which
  for these single-level globs equals Python's string sort; add a test with versions `1.9` and `1.10` to pin the order
  (string order: `1.10` < `1.9`).
- `pub fn library_jars(libraries: &Path) -> Result<Vec<PathBuf>, String>`: port of `library_jars` (adds the newest
  `net/neoforged/bus/*/bus-*.jar` and `net/fabricmc/sponge-mixin/*/*.jar`, skipping `-sources.jar`).
- `pub struct LibClass { pub sup: Option<String>, pub ifaces: Vec<String>, pub access: u16, pub methods: Vec<(String, String, u16)> }`
  and `pub fn build_library_index(jars: &[PathBuf]) -> BTreeMap<String, LibClass>`: port of `build_library_index` (methods
  kept for vanilla and extension classes only; a class seen again merges methods not yet present, as Python does).
- `pub fn load_library_index(jars: &[PathBuf], cache: Option<&Path>) -> Result<BTreeMap<String, LibClass>, String>`:
  port of `load_library_index` with the **same cache file**: name `library-{sha256(json.dumps(stamp))[:20]}.json.gz`
  where `stamp` is `[[str(jar), size, mtime_ns], ...]` serialised exactly as Python's `json.dumps` (`", "` and `": "`
  separators, ASCII escapes), and content `{class: [super, ifaces, access, [[name, desc, access]]]}` gzip'd JSON. Python
  and Rust then share caches; a test pins the key for a fixed stamp (compute the expected hex with Python once and write it
  into the test with the stamp).
- `pub fn spi_names(jars: &[PathBuf]) -> BTreeSet<String>`: port of `spi_names` with `OPAQUE_SERVICES` and `OPAQUE_PACKAGES`.

### `hierarchy.rs`
`pub struct Hierarchy` with `heads`, `libs`, the ancestors memo (`RefCell<HashMap<String, Vec<String>>>`), `jars`:
port of `Hierarchy.__init__`, `add`, `ancestors` (breadth first, `seen` order kept, the class itself left out),
`is_a`, `methods_of`, `vanilla_ancestors`, `declarer`, `event_facts` (returning `EventFacts { cancellable: Option<bool>, bus: Option<&'static str> }`).

### `jars.rs`
- `pub struct Archive { pub mod_id: String, pub label: String, pub zip: ZipArchive<Cursor<Vec<u8>>>, pub toml: toml::Table }`
  and `pub fn walk(path: &Path) -> Result<Vec<Archive>, String>`: port of `walk` (mod id = first `modId` of
  `META-INF/neoforge.mods.toml`, else the owner's, else the label; nested `META-INF/jarjar/*.jar` in sorted name order,
  label `{label}>{stem}`). The top label is the jar's file stem.
- `declared_configs`, `data_kind`, `lenient_json` (strip whole-line `//` comments, then parse), and
  `pub fn jar_facts(archive, spis) -> JarFacts { data: BTreeMap<String, Vec<String>>, at_lines: usize, opaque: Vec<String>, plugins: BTreeMap<String, String> }`:
  ports, with Python's text rules (`split('#')[0].strip()`, `replace('\r\n ', '')` on the manifest, `'utf-8', 'replace'`).
- A TOML that fails to parse: Python's `tomllib.loads` raises and the analysis stops. Same here: an error naming the jar.

## CLI and comparison

- `horizon-java-analysis dump-libraries --install DIR [--cache C]`: the library jar list, the index (as the cache's JSON),
  and `spi_names`.
- `horizon-java-analysis dump-jars --install DIR`: per archive `{mod, label, facts: {data, at_lines, opaque, plugins}}`
  in walk order.
- `java_analysis_compare.py libraries --install DIR` and `jars --install DIR`: the same from Python (`mr.library_jars`,
  `mr.build_library_index`, `mr.spi_names`, `mr.walk`, `mr.jar_facts`), compared as parsed JSON.

## Tests (hand-written expected values)

- `hierarchy.rs`: heads `C → B`, `B → A + I`, `I → J`: `ancestors(C) == [B, A, I, J]` (breadth first: B's super before its
  interfaces), `is_a(C, J)`, `declarer` finds the nearest declaring class with and without a descriptor.
- `jars.rs`: an in-memory jar with `META-INF/neoforge.mods.toml` (`modId = "one"`), a manifest line
  `MixinConfigs: a.mixins.json, b.mixins.json` folded over two lines (`\r\n `), `a.mixins.json` holding a `// comment` line
  and `"plugin": "x.Plugin"`, `data/minecraft/tags/block/foo/bar.json`, `assets/minecraft/lang/en_us.json`,
  `META-INF/accesstransformer.cfg` with 2 rules, a comment and a blank line, and a nested `META-INF/jarjar/inner.jar` with
  no TOML: `walk` gives `[("one", "outer"), ("one", "outer>inner")]`; facts `data == {"data:tags/block": [...],
  "assets:lang": [...]}`, `at_lines == 2`, `plugins == {"a.mixins.json": "x.Plugin"}`.
- `libraries.rs`: the cache key test above; `patched_jars` on a temp tree missing two patterns errors, missing only the
  loader returns three jars.

## Checks

`cargo test -p horizon-java-analysis`, clippy, fmt.

## Done bar

`java_analysis_compare.py libraries` and `jars`: **0 differences** on both finish installs.

## Depends on

SPEC-4.

## Live test for the lead

None (no player-visible change); re-run the two comparisons.
