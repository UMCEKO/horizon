//! The library jars an install is read against, the class index built from them (cached) and the transformation SPI names.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::{Value as Json, json};
use sha2::{Digest, Sha256};

use crate::class::{self, Parse};
use crate::pytext;

const OPAQUE_PACKAGES: [&str; 4] = [
    "net/neoforged/neoforgespi/transformation/",
    "net/neoforged/neoforgespi/coremod/",
    "cpw/mods/modlauncher/api/",
    "org/spongepowered/asm/service/",
];
const OPAQUE_SERVICES: [&str; 2] = [
    "net.neoforged.neoforgespi.transformation.ClassProcessorProvider",
    "net.neoforged.neoforgespi.coremod.ICoreMod",
];

/// One class of the libraries: its supers, access flags and (vanilla and extension classes only) its methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibClass {
    pub sup: Option<String>,
    pub ifaces: Vec<String>,
    pub access: u16,
    pub methods: Vec<(String, String, u16)>,
}

impl LibClass {
    /// The cache's row: `[super, interfaces, access, [[name, desc, access]]]`.
    pub fn to_json(&self) -> Json {
        let methods: Vec<Json> = self
            .methods
            .iter()
            .map(|(n, d, a)| json!([n, d, a]))
            .collect();
        json!([self.sup, self.ifaces, self.access, methods])
    }

    fn from_json(v: &Json) -> Option<LibClass> {
        let row = v.as_array()?;
        let methods = row.get(3)?.as_array()?;
        Some(LibClass {
            sup: row.first()?.as_str().map(str::to_owned),
            ifaces: row
                .get(1)?
                .as_array()?
                .iter()
                .map(|i| i.as_str().map(str::to_owned))
                .collect::<Option<_>>()?,
            access: u16::try_from(row.get(2)?.as_u64()?).ok()?,
            methods: methods
                .iter()
                .map(|m| {
                    let m = m.as_array()?;
                    Some((
                        m.first()?.as_str()?.to_owned(),
                        m.get(1)?.as_str()?.to_owned(),
                        u16::try_from(m.get(2)?.as_u64()?).ok()?,
                    ))
                })
                .collect::<Option<_>>()?,
        })
    }
}

/// Whether `name` fits a pattern with at most one `*`, as Python's glob does (a leading dot is never matched by `*`).
fn fits(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == name,
        Some((head, tail)) => {
            !name.starts_with('.')
                && name.len() >= head.len() + tail.len()
                && name.starts_with(head)
                && name.ends_with(tail)
        }
    }
}

/// The paths under `base` matching a `/`-separated pattern, sorted.
pub fn glob(base: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut level = vec![base.to_path_buf()];
    for part in pattern.split('/') {
        let mut next = Vec::new();
        for dir in &level {
            if !part.contains('*') {
                let path = dir.join(part);
                if path.exists() {
                    next.push(path);
                }
                continue;
            }
            let Ok(read) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in read.flatten() {
                if fits(part, &entry.file_name().to_string_lossy()) {
                    next.push(entry.path());
                }
            }
        }
        level = next;
    }
    level.sort();
    level
}

/// Port of mod_parity_explain.patched_jars.
pub fn patched_jars(libraries: &Path) -> Result<Vec<PathBuf>, String> {
    let libs = libraries.join("net").join("neoforged");
    let patterns = [
        "minecraft-client-patched/*/*.jar",
        "minecraft-server-patched/*/*.jar",
        "neoforge/*/neoforge-*-universal.jar",
        "fancymodloader/loader/*/loader-*.jar",
    ];
    let found: Vec<Vec<PathBuf>> = patterns.iter().map(|p| glob(&libs, p)).collect();
    let missing: Vec<&str> = patterns
        .iter()
        .zip(&found)
        .filter(|(_, f)| f.is_empty())
        .map(|(p, _)| *p)
        .collect();
    if missing.len() > 1 {
        return Err(format!("{} has no {}", libs.display(), missing.join(", ")));
    }
    Ok(found.into_iter().filter_map(|mut f| f.pop()).collect())
}

/// Port of mod_replaces.library_jars.
pub fn library_jars(libraries: &Path) -> Result<Vec<PathBuf>, String> {
    let mut jars = patched_jars(libraries)?;
    for pattern in [
        "net/neoforged/bus/*/bus-*.jar",
        "net/fabricmc/sponge-mixin/*/*.jar",
    ] {
        let mut found = glob(libraries, pattern);
        found.retain(|p| !p.to_string_lossy().ends_with("-sources.jar"));
        jars.extend(found.pop());
    }
    Ok(jars)
}

/// The entry names of a zip in its directory order.
pub fn entry_names<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
) -> Result<Vec<String>, String> {
    (0..zip.len())
        .map(|i| {
            let entry = zip.by_index_raw(i).map_err(|e| e.to_string())?;
            Ok(entry.name().map_or_else(
                |_| String::from_utf8_lossy(entry.name_raw()).into_owned(),
                |n| n.into_owned(),
            ))
        })
        .collect()
}

fn open_zip(jar: &Path) -> Result<zip::ZipArchive<File>, String> {
    let file = File::open(jar).map_err(|e| format!("{}: {e}", jar.display()))?;
    zip::ZipArchive::new(file).map_err(|e| format!("{}: {e}", jar.display()))
}

/// Port of mod_replaces.build_library_index.
pub fn build_library_index(jars: &[PathBuf]) -> Result<BTreeMap<String, LibClass>, String> {
    let mut out: BTreeMap<String, LibClass> = BTreeMap::new();
    let what = Parse {
        members: true,
        annotations: false,
        code: false,
    };
    for jar in jars {
        let mut zip = open_zip(jar)?;
        for entry in entry_names(&mut zip).map_err(|e| format!("{}: {e}", jar.display()))? {
            if !entry.ends_with(".class")
                || entry.starts_with("META-INF/")
                || entry == "module-info.class"
            {
                continue;
            }
            let mut data = Vec::new();
            zip.by_name(&entry)
                .map_err(|e| e.to_string())
                .and_then(|mut f| f.read_to_end(&mut data).map_err(|e| e.to_string()))
                .map_err(|e| format!("{}!{entry}: {e}", jar.display()))?;
            let Ok(info) = class::parse_class(&data, what) else {
                continue;
            };
            let keep = class::is_vanilla(&info.name) || class::is_extension(&info.name);
            let methods: Vec<(String, String, u16)> = if keep {
                info.methods
                    .iter()
                    .map(|m| (m.name.clone(), m.desc.clone(), m.access))
                    .collect()
            } else {
                Vec::new()
            };
            if let Some(known) = out.get_mut(&info.name) {
                let seen: BTreeSet<(String, String)> = known
                    .methods
                    .iter()
                    .map(|m| (m.0.clone(), m.1.clone()))
                    .collect();
                known.methods.extend(
                    methods
                        .into_iter()
                        .filter(|m| !seen.contains(&(m.0.clone(), m.1.clone()))),
                );
            } else {
                out.insert(
                    info.name,
                    LibClass {
                        sup: info.sup,
                        ifaces: info.ifaces,
                        access: info.access,
                        methods,
                    },
                );
            }
        }
    }
    Ok(out)
}

/// Python's `json.dumps` of the cache stamp `[[str(jar), size, mtime_ns], ...]`.
fn stamp_json(jars: &[PathBuf]) -> Result<String, String> {
    let mut out = String::from("[");
    for (i, jar) in jars.iter().enumerate() {
        let meta = std::fs::metadata(jar).map_err(|e| format!("{}: {e}", jar.display()))?;
        let mtime = meta
            .modified()
            .map_err(|e| format!("{}: {e}", jar.display()))?
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        if i > 0 {
            out.push_str(", ");
        }
        out.push('[');
        pytext::dump_str(&jar.to_string_lossy(), &mut out);
        out.push_str(&format!(", {}, {mtime}]", meta.len()));
    }
    out.push(']');
    Ok(out)
}

/// The cache file name for a stamp, as load_library_index names it.
fn cache_name(stamp: &str) -> String {
    let digest = Sha256::digest(stamp.as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("library-{}.json.gz", &hex[..20])
}

/// Port of mod_replaces.load_library_index.
pub fn load_library_index(
    jars: &[PathBuf],
    cache: Option<&Path>,
) -> Result<BTreeMap<String, LibClass>, String> {
    let path = match cache {
        Some(dir) => Some(dir.join(cache_name(&stamp_json(jars)?))),
        None => None,
    };
    if let Some(path) = path.as_ref().filter(|p| p.is_file()) {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut text = String::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_string(&mut text)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let rows: BTreeMap<String, Json> =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        return rows
            .into_iter()
            .map(|(k, v)| {
                LibClass::from_json(&v)
                    .map(|c| (k.clone(), c))
                    .ok_or_else(|| format!("{}: bad row for {k}", path.display()))
            })
            .collect();
    }
    let index = build_library_index(jars)?;
    if let Some(path) = path {
        let rows: BTreeMap<&String, Json> = index.iter().map(|(k, c)| (k, c.to_json())).collect();
        let text = serde_json::to_string(&rows).map_err(|e| e.to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        let bytes = gz.finish().map_err(|e| e.to_string())?;
        std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(index)
}

/// Port of mod_replaces.spi_names.
pub fn spi_names(jars: &[PathBuf]) -> Result<BTreeSet<String>, String> {
    let mut found: BTreeSet<String> = OPAQUE_SERVICES.iter().map(|s| (*s).to_owned()).collect();
    for jar in jars {
        let mut zip = open_zip(jar)?;
        for entry in entry_names(&mut zip).map_err(|e| format!("{}: {e}", jar.display()))? {
            if entry.ends_with(".class")
                && OPAQUE_PACKAGES.iter().any(|p| entry.starts_with(p))
                && !entry.contains('$')
            {
                found.insert(entry[..entry.len() - 6].replace('/', "."));
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(root: &Path, rel: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    const ALL: [&str; 4] = [
        "net/neoforged/minecraft-client-patched/1/a.jar",
        "net/neoforged/minecraft-server-patched/1/b.jar",
        "net/neoforged/neoforge/1/neoforge-1-universal.jar",
        "net/neoforged/fancymodloader/loader/1/loader-1.jar",
    ];

    #[test]
    fn cache_key_is_pythons() {
        // From Python: hashlib.sha256(json.dumps(stamp).encode()).hexdigest()[:20]
        let stamp = r#"[["/l/a.jar", 12, 1700000000123456789], ["/l/\u00e9.jar", 3, 5]]"#;
        assert_eq!(cache_name(stamp), "library-441c897438087b5a54ae.json.gz");
    }

    #[test]
    fn stamp_is_written_like_python_dumps() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "\u{e9}.jar");
        let stamp = stamp_json(&[dir.path().join("\u{e9}.jar")]).unwrap();
        assert!(stamp.starts_with("[[\""), "{stamp}");
        assert!(stamp.contains("\\u00e9.jar\", 0, "), "{stamp}");
        assert!(stamp.ends_with("]]"), "{stamp}");
    }

    #[test]
    fn patched_jars_takes_the_last_sorted_match() {
        let dir = tempfile::tempdir().unwrap();
        for rel in ALL {
            touch(dir.path(), rel);
        }
        touch(
            dir.path(),
            "net/neoforged/minecraft-client-patched/1.9/a.jar",
        );
        touch(
            dir.path(),
            "net/neoforged/minecraft-client-patched/1.10/a.jar",
        );
        let jars = patched_jars(dir.path()).unwrap();
        assert_eq!(jars.len(), 4);
        assert!(
            jars[0].ends_with("minecraft-client-patched/1.9/a.jar"),
            "{jars:?}"
        );
    }

    #[test]
    fn patched_jars_errors_on_two_missing_but_not_one() {
        let dir = tempfile::tempdir().unwrap();
        for rel in &ALL[..2] {
            touch(dir.path(), rel);
        }
        let err = patched_jars(dir.path()).unwrap_err();
        assert!(err.contains("neoforge/*/neoforge-*-universal.jar"), "{err}");
        assert!(
            err.contains("fancymodloader/loader/*/loader-*.jar"),
            "{err}"
        );
        touch(dir.path(), ALL[2]);
        assert_eq!(patched_jars(dir.path()).unwrap().len(), 3);
    }

    #[test]
    fn class_rows_round_trip_through_the_cache_shape() {
        let row = json!(["a/B", ["a/I"], 33, [["m", "()V", 1]]]);
        assert_eq!(LibClass::from_json(&row).unwrap().to_json(), row);
    }
}
