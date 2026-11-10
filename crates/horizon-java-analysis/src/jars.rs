//! A mod archive and the jars nested in it: its mod id, data overrides, access transformers, opaque transformers and mixin plugins.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::Path;

use serde::Serialize;
use serde_json::Value as Json;
use zip::ZipArchive;

use crate::libraries::entry_names;
use crate::pytext;

const ACTIVE_DATA: [&str; 2] = ["data/minecraft/", "assets/minecraft/"];
const OPAQUE_FILES: [&str; 1] = ["META-INF/coremods.json"];
const MODS_TOML: &str = "META-INF/neoforge.mods.toml";

/// One archive: a mod jar or a jar nested in it.
pub struct Archive {
    pub mod_id: String,
    pub label: String,
    pub zip: ZipArchive<Cursor<Vec<u8>>>,
    pub toml: toml::Table,
}

/// What one archive changes outside its classes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct JarFacts {
    pub data: BTreeMap<String, Vec<String>>,
    pub at_lines: usize,
    pub opaque: Vec<String>,
    pub plugins: BTreeMap<String, String>,
}

/// An entry's bytes.
pub fn read_entry(zip: &mut ZipArchive<Cursor<Vec<u8>>>, name: &str) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    zip.by_name(name)
        .map_err(|e| format!("{name}: {e}"))?
        .read_to_end(&mut data)
        .map_err(|e| format!("{name}: {e}"))?;
    Ok(data)
}

fn read_text(zip: &mut ZipArchive<Cursor<Vec<u8>>>, name: &str) -> Result<String, String> {
    Ok(String::from_utf8_lossy(&read_entry(zip, name)?).into_owned())
}

/// The `mods` of a `neoforge.mods.toml` that carry a modId, first one first.
fn mod_ids(toml: &toml::Table) -> Vec<&str> {
    toml.get("mods")
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("modId")?.as_str())
        .filter(|id| !id.is_empty())
        .collect()
}

/// Port of mod_replaces.walk.
pub fn walk(path: &Path) -> Result<Vec<Archive>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let label = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let mut out = Vec::new();
    walk_into(bytes, None, &label, &mut out).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(out)
}

fn walk_into(
    bytes: Vec<u8>,
    owner: Option<&str>,
    label: &str,
    out: &mut Vec<Archive>,
) -> Result<(), String> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("{label}: {e}"))?;
    let names = entry_names(&mut zip).map_err(|e| format!("{label}: {e}"))?;
    let names: BTreeSet<String> = names.into_iter().collect();
    let toml = if names.contains(MODS_TOML) {
        let text = read_text(&mut zip, MODS_TOML).map_err(|e| format!("{label}: {e}"))?;
        text.parse::<toml::Table>()
            .map_err(|e| format!("{label}: {MODS_TOML}: {e}"))?
    } else {
        toml::Table::new()
    };
    let mod_id = mod_ids(&toml)
        .first()
        .map_or_else(|| owner.unwrap_or(label).to_owned(), |id| (*id).to_owned());
    let nested: Vec<&String> = names
        .iter()
        .filter(|n| n.starts_with("META-INF/jarjar/") && n.ends_with(".jar"))
        .collect();
    let mut inner = Vec::new();
    for name in nested {
        let stem = Path::new(name)
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
        inner.push((read_entry(&mut zip, name)?, format!("{label}>{stem}")));
    }
    out.push(Archive {
        mod_id: mod_id.clone(),
        label: label.to_owned(),
        zip,
        toml,
    });
    for (bytes, label) in inner {
        walk_into(bytes, Some(&mod_id), &label, out)?;
    }
    Ok(())
}

/// Port of mod_replaces.declared_configs.
pub fn declared_configs(archive: &mut Archive) -> Result<Vec<String>, String> {
    let mut configs: Vec<String> = archive
        .toml
        .get("mixins")
        .and_then(|m| m.as_array())
        .into_iter()
        .flatten()
        .filter_map(|m| m.get("config")?.as_str().map(str::to_owned))
        .collect();
    let names = entry_names(&mut archive.zip)?;
    if names.iter().any(|n| n == "META-INF/MANIFEST.MF") {
        let text = read_text(&mut archive.zip, "META-INF/MANIFEST.MF")?.replace("\r\n ", "");
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

/// Port of mod_replaces.data_kind.
pub fn data_kind(name: &str) -> String {
    let (root, prefix) = if name.starts_with("data/") {
        ("data", "data/minecraft/")
    } else {
        ("assets", "assets/minecraft/")
    };
    let rest = name.get(prefix.len()..).unwrap_or("");
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() < 2 {
        return format!("{root}:(root)");
    }
    let take = if matches!(parts[0], "tags" | "worldgen") && parts.len() > 2 {
        2
    } else {
        1
    };
    format!("{root}:{}", parts[..take].join("/"))
}

/// Port of mod_parity.lenient_json.
pub fn lenient_json(text: &str) -> Result<Json, String> {
    if let Ok(v) = serde_json::from_str(text) {
        return Ok(v);
    }
    let stripped: Vec<&str> = text
        .split('\n')
        .map(|l| {
            if pytext::lstrip(l).starts_with("//") {
                ""
            } else {
                l
            }
        })
        .collect();
    serde_json::from_str(&stripped.join("\n")).map_err(|e| e.to_string())
}

/// The non-empty lines of a text after cutting each at its first `#`.
fn code_lines(text: &str) -> Vec<&str> {
    pytext::splitlines(text)
        .into_iter()
        .map(|l| pytext::strip(l.split('#').next().unwrap_or("")))
        .filter(|l| !l.is_empty())
        .collect()
}

/// Port of mod_replaces.jar_facts.
pub fn jar_facts(archive: &mut Archive, spis: &BTreeSet<String>) -> Result<JarFacts, String> {
    let names = entry_names(&mut archive.zip)?;
    let present: BTreeSet<&str> = names.iter().map(String::as_str).collect();
    let mut facts = JarFacts::default();
    for name in &names {
        if ACTIVE_DATA.iter().any(|p| name.starts_with(p)) && !name.ends_with('/') {
            facts
                .data
                .entry(data_kind(name))
                .or_default()
                .push(name.clone());
        }
    }
    let mut at_files = vec!["META-INF/accesstransformer.cfg".to_owned()];
    at_files.extend(
        archive
            .toml
            .get("accessTransformers")
            .and_then(|m| m.as_array())
            .into_iter()
            .flatten()
            .filter_map(|a| a.get("file")?.as_str())
            .filter(|f| !f.is_empty())
            .map(str::to_owned),
    );
    let mut seen = BTreeSet::new();
    for file in at_files {
        if seen.insert(file.clone()) && present.contains(file.as_str()) {
            facts.at_lines += code_lines(&read_text(&mut archive.zip, &file)?).len();
        }
    }
    for name in &names {
        let slashes = name.matches('/').count();
        let last = name.rsplit('/').next().unwrap_or("");
        if name.starts_with("META-INF/services/") && slashes == 2 && spis.contains(last) {
            let text = read_text(&mut archive.zip, name)?;
            let providers = code_lines(&text).join(", ");
            let providers = if providers.is_empty() {
                "(empty)"
            } else {
                &providers
            };
            facts.opaque.push(format!("service {last}: {providers}"));
        } else if OPAQUE_FILES.contains(&name.as_str())
            || (name.starts_with("META-INF/coremods/") && name.ends_with(".js"))
        {
            facts.opaque.push(format!("coremod file {name}"));
        }
    }
    for config in declared_configs(archive)? {
        if present.contains(config.as_str()) {
            let text = read_text(&mut archive.zip, &config)?;
            let json = lenient_json(&text).map_err(|e| format!("{config}: {e}"))?;
            let plugin = json.get("plugin").and_then(Json::as_str).unwrap_or("");
            if !plugin.is_empty() {
                facts.plugins.insert(config, plugin.to_owned());
            }
        }
    }
    Ok(facts)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn jar(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    fn sample() -> Vec<u8> {
        let inner = jar(&[("x.txt", b"")]);
        let manifest =
            b"Manifest-Version: 1.0\r\nMixinConfigs: a.mixins.json, \r\n b.mixins.json\r\n";
        jar(&[
            (
                "META-INF/neoforge.mods.toml",
                b"[[mods]]\nmodId = \"one\"\n",
            ),
            ("META-INF/MANIFEST.MF", manifest),
            (
                "a.mixins.json",
                b"{\n// a comment\n  \"plugin\": \"x.Plugin\"\n}",
            ),
            ("data/minecraft/tags/block/foo/bar.json", b"{}"),
            ("assets/minecraft/lang/en_us.json", b"{}"),
            (
                "META-INF/accesstransformer.cfg",
                b"# a comment\npublic a.B c\n\npublic a.B d # tail\n",
            ),
            ("META-INF/jarjar/inner.jar", &inner),
        ])
    }

    #[test]
    fn walk_names_the_jar_and_its_nested_jar() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("outer.jar");
        std::fs::write(&path, sample()).unwrap();
        let mut archives = walk(&path).unwrap();
        let ids: Vec<(&str, &str)> = archives
            .iter()
            .map(|a| (a.mod_id.as_str(), a.label.as_str()))
            .collect();
        assert_eq!(ids, [("one", "outer"), ("one", "outer>inner")]);

        let facts = jar_facts(&mut archives[0], &BTreeSet::new()).unwrap();
        assert_eq!(
            facts.data["data:tags/block"],
            ["data/minecraft/tags/block/foo/bar.json"]
        );
        assert_eq!(
            facts.data["assets:lang"],
            ["assets/minecraft/lang/en_us.json"]
        );
        assert_eq!(facts.at_lines, 2);
        assert_eq!(
            facts.plugins,
            BTreeMap::from([("a.mixins.json".to_owned(), "x.Plugin".to_owned())])
        );
        assert!(facts.opaque.is_empty());
    }

    #[test]
    fn data_kinds() {
        assert_eq!(
            data_kind("data/minecraft/tags/block/x.json"),
            "data:tags/block"
        );
        assert_eq!(data_kind("assets/minecraft/lang/en_us.json"), "assets:lang");
        assert_eq!(data_kind("data/minecraft/tags/x.json"), "data:tags");
        assert_eq!(data_kind("data/minecraft/x.json"), "data:(root)");
    }

    #[test]
    fn opaque_services_and_coremods() {
        let spi = "a.Spi";
        let bytes = jar(&[
            ("META-INF/services/a.Spi", b"p.One # c\n\n"),
            ("META-INF/services/other", b"p.Two\n"),
            ("META-INF/coremods.json", b"{}"),
            ("META-INF/coremods/x.js", b""),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("o.jar");
        std::fs::write(&path, bytes).unwrap();
        let mut archives = walk(&path).unwrap();
        assert_eq!(archives[0].mod_id, "o");
        let spis = BTreeSet::from([spi.to_owned()]);
        let facts = jar_facts(&mut archives[0], &spis).unwrap();
        assert_eq!(
            facts.opaque,
            [
                "service a.Spi: p.One",
                "coremod file META-INF/coremods.json",
                "coremod file META-INF/coremods/x.js"
            ]
        );
    }

    #[test]
    fn bad_toml_names_the_jar() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.jar");
        std::fs::write(&path, jar(&[("META-INF/neoforge.mods.toml", b"= nope")])).unwrap();
        let err = walk(&path).err().unwrap();
        assert!(err.contains("bad.jar"), "{err}");
    }

    #[test]
    fn lenient_json_drops_comment_lines_only() {
        let v = lenient_json("{\n  // note\n \"a\": \"//kept\"\n}").unwrap();
        assert_eq!(v["a"], "//kept");
        assert!(lenient_json("{ // inline\n}").is_err());
    }
}
