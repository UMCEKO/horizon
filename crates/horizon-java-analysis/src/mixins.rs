//! The mixin handlers of an install's mods, read from the class files: what `MixinScanner` prints, without a JVM.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Cursor;
use std::path::Path;

use serde_json::Value as Json;
use zip::ZipArchive;

use crate::class::{Annotation, Const, Parse, Value, parse_class};
use crate::jars::{lenient_json, mod_jars, read_entry};
use crate::libraries::entry_names;
use crate::pytext;

const MIXIN: &str = "Lorg/spongepowered/asm/mixin/Mixin;";
const MODS_TOML: &str = "META-INF/neoforge.mods.toml";
const MANIFEST: &str = "META-INF/MANIFEST.MF";
const JARJAR: &str = "META-INF/jarjar/";
const TOP_TARGETS: usize = 25;
const KINDS: [(&str, &str); 15] = [
    ("Inject", "inject"),
    ("Redirect", "redirect"),
    ("ModifyArg", "modify_arg"),
    ("ModifyArgs", "modify_args"),
    ("ModifyVariable", "modify_variable"),
    ("ModifyConstant", "modify_constant"),
    ("ModifyExpressionValue", "modify_expression_value"),
    ("ModifyReceiver", "modify_receiver"),
    ("ModifyReturnValue", "modify_return_value"),
    ("WrapOperation", "wrap_operation"),
    ("WrapWithCondition", "wrap_with_condition"),
    ("WrapMethod", "wrap_method"),
    ("Overwrite", "overwrite"),
    ("Accessor", "accessor"),
    ("Invoker", "invoker"),
];

/// One mixin handler method (an entry of mixins.json).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixinEntry {
    pub mod_id: String,
    pub config: String,
    pub side: String,
    pub mixin: String,
    pub handler: String,
    pub kind: String,
    pub target_class: Vec<String>,
    pub target_method: Vec<String>,
    pub at: Vec<String>,
    pub access: bool,
}

/// A mixin class a jar declares: its mod, config, side, dotted name and bytes.
pub type MixinClass = (String, String, String, String, Vec<u8>);

type Zip = ZipArchive<Cursor<Vec<u8>>>;

fn strict_text(zip: &mut Zip, name: &str, label: &str) -> Result<String, String> {
    String::from_utf8(read_entry(zip, name)?).map_err(|e| format!("{label}: {name}: {e}"))
}

/// Python's `bool(value)` of a JSON value.
fn truthy(v: &Json) -> bool {
    match v {
        Json::Null => false,
        Json::Bool(b) => *b,
        Json::Number(n) => n.as_f64() != Some(0.0),
        Json::String(s) => !s.is_empty(),
        Json::Array(a) => !a.is_empty(),
        Json::Object(o) => !o.is_empty(),
    }
}

/// The configs of a jar: its toml's `mixins` first, then the manifest's `MixinConfigs`, each once.
fn configs_of(
    zip: &mut Zip,
    toml: &toml::Table,
    has_manifest: bool,
    label: &str,
) -> Result<Vec<String>, String> {
    let mut configs: Vec<String> = toml
        .get("mixins")
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("config")?.as_str().map(str::to_owned))
        .collect();
    if has_manifest {
        let text = strict_text(zip, MANIFEST, label)?.replace("\r\n ", "");
        for line in pytext::splitlines(&text) {
            if let Some(rest) = line.strip_prefix("MixinConfigs:") {
                configs.extend(
                    rest.split(',')
                        .map(pytext::strip)
                        .filter(|c| !c.is_empty())
                        .map(str::to_owned),
                );
            }
        }
    }
    let mut seen = BTreeSet::new();
    configs.retain(|c| seen.insert(c.clone()));
    Ok(configs)
}

fn string_list(data: &Json, key: &str, config: &str) -> Result<Vec<String>, String> {
    let Some(list) = data.get(key) else {
        return Ok(Vec::new());
    };
    list.as_array()
        .ok_or_else(|| format!("{config}: {key} is not a list"))?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{config}: {key} holds a non-string"))
        })
        .collect()
}

/// Port of mod_parity.jar_mixin_classes: (mod, config, side, class name, class bytes) of a jar and its nested jars.
pub fn jar_mixin_classes(
    zip: &mut Zip,
    owner: Option<&str>,
    label: &str,
    warnings: &mut Vec<String>,
) -> Result<Vec<MixinClass>, String> {
    let names = entry_names(zip).map_err(|e| format!("{label}: {e}"))?;
    let present: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    let toml = if present.contains(MODS_TOML) {
        strict_text(zip, MODS_TOML, label)?
            .parse::<toml::Table>()
            .map_err(|e| format!("{label}: {MODS_TOML}: {e}"))?
    } else {
        toml::Table::new()
    };
    let mod_id = toml
        .get("mods")
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("modId")?.as_str())
        .find(|id| !id.is_empty())
        .or(owner.filter(|o| !o.is_empty()))
        .unwrap_or(label)
        .to_owned();
    let mut found = Vec::new();
    for config in configs_of(zip, &toml, present.contains(MANIFEST), label)? {
        if !present.contains(config.as_str()) {
            warnings.push(format!("{mod_id}: config {config} is not in the jar"));
            continue;
        }
        let text = strict_text(zip, &config, label)?;
        let data = lenient_json(&text).map_err(|e| format!("{label}: {config}: {e}"))?;
        if !data.is_object() {
            return Err(format!("{label}: {config}: not a JSON object"));
        }
        if let Some(plugin) = data.get("plugin").filter(|p| truthy(p)) {
            let shown = plugin
                .as_str()
                .map_or_else(|| plugin.to_string(), str::to_owned);
            warnings.push(format!(
                "{mod_id}: {config} has a config plugin ({shown}) that may drop or add mixins at runtime"
            ));
        }
        let package = data.get("package").and_then(Json::as_str).unwrap_or("");
        for (key, side) in [
            ("mixins", "common"),
            ("client", "client"),
            ("server", "server"),
        ] {
            for entry in string_list(&data, key, &config)? {
                let name = if package.is_empty() {
                    entry
                } else {
                    format!("{package}.{entry}")
                };
                let path = format!("{}.class", name.replace('.', "/"));
                if present.contains(path.as_str()) {
                    let bytes = read_entry(zip, &path)?;
                    found.push((mod_id.clone(), config.clone(), side.to_owned(), name, bytes));
                } else {
                    warnings.push(format!(
                        "{mod_id}: {config} lists {name}, which is not in the jar"
                    ));
                }
            }
        }
    }
    let nested: BTreeSet<&String> = names
        .iter()
        .filter(|n| n.starts_with(JARJAR) && n.ends_with(".jar"))
        .collect();
    for name in nested {
        let bytes = read_entry(zip, name)?;
        let mut inner = ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("{name}: {e}"))?;
        found.extend(jar_mixin_classes(
            &mut inner,
            Some(&mod_id),
            name,
            warnings,
        )?);
    }
    Ok(found)
}

/// ASM's `Type.getClassName` of a descriptor.
fn class_name(desc: &str) -> String {
    let dims = desc.chars().take_while(|&c| c == '[').count();
    let base = &desc[dims..];
    let name = match base {
        "Z" => "boolean".to_owned(),
        "C" => "char".to_owned(),
        "B" => "byte".to_owned(),
        "S" => "short".to_owned(),
        "I" => "int".to_owned(),
        "F" => "float".to_owned(),
        "J" => "long".to_owned(),
        "D" => "double".to_owned(),
        "V" => "void".to_owned(),
        other => other
            .strip_prefix('L')
            .and_then(|o| o.strip_suffix(';'))
            .unwrap_or(other)
            .replace('/', "."),
    };
    name + &"[]".repeat(dims)
}

/// Java's `Double.toString` (or `Float.toString`) from the shortest digits of Rust's `{:e}` text of the magnitude.
fn java_float(sci: &str, negative: bool) -> String {
    let (mantissa, exp) = sci.split_once('e').unwrap_or((sci, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let sign = if negative { "-" } else { "" };
    if digits.bytes().all(|b| b == b'0') {
        return format!("{sign}0.0");
    }
    let rest = |from: usize| {
        let r = digits.get(from..).unwrap_or("");
        if r.is_empty() {
            "0".to_owned()
        } else {
            r.to_owned()
        }
    };
    if (-3..7).contains(&exp) {
        if exp >= 0 {
            let int_len = exp as usize + 1;
            let int: String = digits
                .chars()
                .chain(std::iter::repeat('0'))
                .take(int_len)
                .collect();
            format!("{sign}{int}.{}", rest(int_len))
        } else {
            format!("{sign}0.{}{digits}", "0".repeat((-exp - 1) as usize))
        }
    } else {
        format!("{sign}{}.{}E{exp}", &digits[..1], rest(1))
    }
}

fn java_double(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_owned()
    } else if v.is_infinite() {
        (if v > 0.0 { "Infinity" } else { "-Infinity" }).to_owned()
    } else {
        java_float(&format!("{:e}", v.abs()), v.is_sign_negative())
    }
}

fn java_single(v: f32) -> String {
    if v.is_nan() {
        "NaN".to_owned()
    } else if v.is_infinite() {
        (if v > 0.0 { "Infinity" } else { "-Infinity" }).to_owned()
    } else {
        java_float(&format!("{:e}", v.abs()), v.is_sign_negative())
    }
}

/// Step 1's `one(Object)` of MixinScanner.
fn one(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Class(desc) => desc.clone(),
        Value::Enum { desc, name } => format!("{desc}.{name}"),
        Value::Anno(a) => format!("@{}", a.desc),
        Value::Array(items) => format!("[{}]", items.iter().map(one).collect::<Vec<_>>().join(",")),
        Value::Const { tag, value } => match (tag, value) {
            ('Z', Const::Int(i)) => (*i != 0).to_string(),
            ('C', Const::Int(i)) => u32::try_from(*i)
                .ok()
                .and_then(char::from_u32)
                .map_or_else(|| "?".to_owned(), String::from),
            ('B', Const::Int(i)) => (*i as i8).to_string(),
            ('S', Const::Int(i)) => (*i as i16).to_string(),
            (_, Const::Int(i)) => i.to_string(),
            (_, Const::Long(l)) => l.to_string(),
            (_, Const::Float(f)) => java_single(*f),
            (_, Const::Double(d)) => java_double(*d),
            (_, Const::Str(s)) => s.clone(),
        },
    }
}

/// Step 1's `render(List<Object>)`: `key=value` pairs joined by a comma.
fn render(values: &[(String, Value)]) -> String {
    values
        .iter()
        .map(|(k, v)| format!("{k}={}", one(v)))
        .collect::<Vec<_>>()
        .join(",")
}

/// Java's string concatenation of an annotation value (`null` when absent, `List.toString` for a list).
fn concat(v: Option<&Value>) -> String {
    match v {
        None => "null".to_owned(),
        Some(Value::Array(items)) => format!(
            "[{}]",
            items
                .iter()
                .map(|i| concat(Some(i)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Some(other) => one(other),
    }
}

fn get<'a>(a: &'a Annotation, key: &str) -> Option<&'a Value> {
    a.values.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// MixinScanner's `list`: the elements of an array value, the value itself when single, none when absent.
fn list<'a>(a: &'a Annotation, key: &str) -> Vec<&'a Value> {
    match get(a, key) {
        None => Vec::new(),
        Some(Value::Array(items)) => items.iter().collect(),
        Some(v) => vec![v],
    }
}

fn at_text(n: &Annotation) -> String {
    let target = get(n, "target");
    format!(
        "{}{}",
        concat(get(n, "value")),
        target.map_or_else(String::new, |t| format!(":{}", concat(Some(t))))
    )
}

fn strings(a: &Annotation, key: &str, what: &str) -> Result<Vec<String>, String> {
    list(a, key)
        .into_iter()
        .map(|v| match v {
            Value::Str(s) => Ok(s.clone()),
            _ => Err(format!("{what}: {key} holds a non-string")),
        })
        .collect()
}

/// What `parse_scan` makes of a joined field: empty means no items.
fn split_field(items: &[String], sep: char) -> Vec<String> {
    let joined = items.join(&sep.to_string());
    if joined.is_empty() {
        Vec::new()
    } else {
        joined.split(sep).map(str::to_owned).collect()
    }
}

/// Port of MixinScanner.scan (with step 1's rendering) on SPEC-4's reader.
pub fn scan_class(
    mod_id: &str,
    config: &str,
    side: &str,
    bytes: &[u8],
) -> Result<Vec<MixinEntry>, String> {
    let what = Parse {
        members: true,
        annotations: true,
        code: false,
    };
    let node = parse_class(bytes, what).map_err(|e| e.0)?;
    let class = node.name.replace('/', ".");
    let mut targets = Vec::new();
    for a in node
        .invisible
        .iter()
        .chain(&node.annos)
        .filter(|a| a.desc == MIXIN)
    {
        for v in list(a, "value") {
            let Value::Class(desc) = v else {
                return Err(format!("{class}: Mixin value holds a non-class"));
            };
            targets.push(class_name(desc));
        }
        targets.extend(
            strings(a, "targets", &class)?
                .iter()
                .map(|t| t.replace('/', ".")),
        );
    }
    let mut out = Vec::new();
    for m in &node.methods {
        for a in m.invisible.iter().chain(&m.annos) {
            let simple = a.desc.rsplit('/').next().unwrap_or(&a.desc);
            let simple = simple.get(..simple.len().saturating_sub(1)).unwrap_or("");
            let Some((_, kind)) = KINDS.iter().find(|(k, _)| *k == simple) else {
                continue;
            };
            let mut methods = strings(a, "method", &class)?;
            if *kind == "overwrite" {
                methods.push(m.name.clone());
            }
            methods.extend(list(a, "value").into_iter().filter_map(|v| match v {
                Value::Str(s) => Some(s.clone()),
                _ => None,
            }));
            let mut at = Vec::new();
            for v in list(a, "at") {
                if let Value::Anno(n) = v {
                    at.push(at_text(n));
                }
            }
            for v in list(a, "constant") {
                if let Value::Anno(n) = v {
                    at.push(format!("CONSTANT:{}", render(&n.values)));
                }
            }
            out.push(MixinEntry {
                mod_id: mod_id.to_owned(),
                config: config.to_owned(),
                side: side.to_owned(),
                mixin: class.clone(),
                handler: m.name.clone(),
                kind: (*kind).to_owned(),
                target_class: split_field(&targets, ','),
                target_method: split_field(&methods, ','),
                at: split_field(&at, '|'),
                access: matches!(*kind, "accessor" | "invoker"),
            });
        }
    }
    Ok(out)
}

/// Port of mod_parity.scan_install: every handler of every jar in `install/mods`, and the mixin classes read.
pub fn scan_install(
    install: &Path,
    warnings: &mut Vec<String>,
) -> Result<(Vec<MixinEntry>, usize), String> {
    let mut found = Vec::new();
    for jar in mod_jars(install) {
        let label = jar
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        let bytes = std::fs::read(&jar).map_err(|e| format!("{}: {e}", jar.display()))?;
        let mut zip =
            ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("{}: {e}", jar.display()))?;
        found.extend(jar_mixin_classes(&mut zip, None, &label, warnings)?);
    }
    let mut entries = Vec::new();
    for (mod_id, config, side, name, bytes) in &found {
        entries
            .extend(scan_class(mod_id, config, side, bytes).map_err(|e| format!("{name}: {e}"))?);
    }
    Ok((entries, found.len()))
}

/// Port of mod_parity.mixins_markdown.
pub fn mixins_markdown(entries: &[MixinEntry], classes: usize, warnings: &[String]) -> String {
    let mut by_mod: BTreeMap<&str, BTreeMap<&str, usize>> = BTreeMap::new();
    for e in entries {
        let counts = by_mod.entry(&e.mod_id).or_default();
        *counts
            .entry(if e.access { "access" } else { "behaviour" })
            .or_default() += 1;
        *counts.entry(&e.kind).or_default() += 1;
    }
    let mut kinds: Vec<&str> = entries.iter().map(|e| e.kind.as_str()).collect();
    kinds.sort_unstable();
    kinds.dedup();
    let mut lines = vec![
        format!(
            "# Mixin handlers: {} in {classes} mixin classes",
            entries.len()
        ),
        String::new(),
        format!("| mod | behaviour | access | {} |", kinds.join(" | ")),
        format!("|---|---|---|{}", "---|".repeat(kinds.len())),
    ];
    for (m, c) in &by_mod {
        let n = |k: &str| c.get(k).copied().unwrap_or(0);
        let cells: Vec<String> = kinds.iter().map(|k| n(k).to_string()).collect();
        lines.push(format!(
            "| {m} | {} | {} | {} |",
            n("behaviour"),
            n("access"),
            cells.join(" | ")
        ));
    }
    let mut order: Vec<&str> = Vec::new();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for t in entries
        .iter()
        .filter(|e| !e.access)
        .flat_map(|e| &e.target_class)
    {
        let n = counts.entry(t).or_default();
        if *n == 0 {
            order.push(t);
        }
        *n += 1;
    }
    order.sort_by_key(|t| std::cmp::Reverse(counts[t]));
    lines.extend(
        [
            "",
            "## Top target classes (behaviour handlers)",
            "",
            "| class | handlers |",
            "|---|---|",
        ]
        .map(str::to_owned),
    );
    lines.extend(
        order
            .iter()
            .take(TOP_TARGETS)
            .map(|t| format!("| {t} | {} |", counts[t])),
    );
    if !warnings.is_empty() {
        lines.extend([String::new(), "## Notes".to_owned(), String::new()]);
        lines.extend(warnings.iter().map(|w| format!("- {w}")));
    }
    lines.join("\n") + "\n"
}

fn json_list(items: &[String], out: &mut String) {
    if items.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push_str("[\n");
    for (i, s) in items.iter().enumerate() {
        out.push_str("   ");
        pytext::dump_str(s, out);
        out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
    }
    out.push_str("  ]");
}

/// The entries as Python's `json.dumps(entries, indent=1)` writes them.
pub fn mixins_json(entries: &[MixinEntry]) -> String {
    if entries.is_empty() {
        return "[]\n".to_owned();
    }
    let mut out = String::from("[\n");
    for (i, e) in entries.iter().enumerate() {
        out.push_str(" {\n");
        for (key, value) in [
            ("mod", &e.mod_id),
            ("config", &e.config),
            ("mixin", &e.mixin),
            ("handler", &e.handler),
            ("kind", &e.kind),
        ] {
            let _ = write!(out, "  \"{key}\": ");
            pytext::dump_str(value, &mut out);
            out.push_str(",\n");
        }
        for (key, list) in [
            ("target_class", &e.target_class),
            ("target_method", &e.target_method),
            ("at", &e.at),
        ] {
            let _ = write!(out, "  \"{key}\": ");
            json_list(list, &mut out);
            out.push_str(",\n");
        }
        out.push_str("  \"side\": ");
        pytext::dump_str(&e.side, &mut out);
        let _ = write!(out, ",\n  \"access\": {}\n }}", e.access);
        out.push_str(if i + 1 < entries.len() { ",\n" } else { "\n" });
    }
    out.push_str("]\n");
    out
}

/// Writes `mixins.json` and `mixins.md` into `out`.
pub fn write_mixins(
    out: &Path,
    entries: &[MixinEntry],
    classes: usize,
    warnings: &[String],
) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    for (name, text) in [
        ("mixins.json", mixins_json(entries)),
        ("mixins.md", mixins_markdown(entries, classes, warnings)),
    ] {
        std::fs::write(out.join(name), text).map_err(|e| format!("{name}: {e}"))?;
    }
    Ok(())
}
