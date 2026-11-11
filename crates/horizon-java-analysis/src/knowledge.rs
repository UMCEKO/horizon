//! What only Horizon's tree knows, read from the boot jar's `analysis/` resources: the port ledger's verdict per vanilla method, the verdict map, the host's words.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek};
use std::path::Path;

use zip::ZipArchive;

const NATIVE: &str = "analysis/native.tsv";
const VERDICTS: &str = "analysis/verdicts.tsv";
const HOST_WORDS: &str = "analysis/host_words.txt";
const WEAK_STATUSES: [&str; 5] = ["mentioned", "inferred", "absent", "none", "unknown"];
const ROUTING_ORDER: [&str; 6] = [
    "java",
    "java-own-type",
    "undecided",
    "java-already",
    "native-hook",
    "inert",
];

/// What the port ledger says about a vanilla method: who owns it in Rust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeInfo {
    pub status: String,
    pub owned: bool,
    pub fns: Vec<String>,
    pub note: String,
}

/// The three resources of the boot jar.
pub struct Knowledge {
    native: HashMap<String, NativeInfo>,
    verdicts: HashMap<String, String>,
    pub host_words: HashSet<String>,
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn rows(text: &str) -> impl Iterator<Item = Vec<String>> + '_ {
    text.split('\n')
        .filter(|l| !l.is_empty())
        .map(|l| l.split('\t').map(unescape).collect())
}

fn read_resource<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    jar: &str,
    name: &str,
) -> Result<String, String> {
    let mut entry = zip.by_name(name).map_err(|_| {
        format!("{jar} has no {name}: build the host jars again (java_mods.py build)")
    })?;
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|e| format!("{jar}!{name}: {e}"))?;
    Ok(text)
}

impl Knowledge {
    /// The three resources of the boot jar; an error names the one missing.
    pub fn from_boot_jar(jar: &Path) -> Result<Knowledge, String> {
        let label = jar.display().to_string();
        let file = File::open(jar).map_err(|e| format!("{label}: {e}"))?;
        let mut zip = ZipArchive::new(file).map_err(|e| format!("{label}: {e}"))?;
        Knowledge::from_archive(&mut zip, &label)
    }

    /// The same from an open archive; `label` names it in errors.
    pub fn from_archive<R: Read + Seek>(
        zip: &mut ZipArchive<R>,
        label: &str,
    ) -> Result<Knowledge, String> {
        let native_text = read_resource(zip, label, NATIVE)?;
        let verdict_text = read_resource(zip, label, VERDICTS)?;
        let words_text = read_resource(zip, label, HOST_WORDS)?;
        let mut native = HashMap::new();
        for row in rows(&native_text) {
            let [key, status, owned, fns, note] = <[String; 5]>::try_from(row)
                .map_err(|row| format!("{label}!{NATIVE}: {} fields, want 5", row.len()))?;
            let fns = if fns.is_empty() {
                Vec::new()
            } else {
                fns.split(',').map(str::to_owned).collect()
            };
            native.insert(
                key,
                NativeInfo {
                    status,
                    owned: owned == "1",
                    fns,
                    note,
                },
            );
        }
        let mut verdicts = HashMap::new();
        for row in rows(&verdict_text) {
            let [key, verdict] = <[String; 2]>::try_from(row)
                .map_err(|row| format!("{label}!{VERDICTS}: {} fields, want 2", row.len()))?;
            verdicts.insert(key, verdict);
        }
        let host_words = words_text
            .split('\n')
            .filter(|w| !w.is_empty())
            .map(str::to_owned)
            .collect();
        Ok(Knowledge {
            native,
            verdicts,
            host_words,
        })
    }

    /// Port of mod_replaces.Native.info.
    pub fn info(&self, key: &str) -> NativeInfo {
        self.native.get(key).cloned().unwrap_or_else(|| NativeInfo {
            status: "unknown".to_owned(),
            owned: false,
            fns: Vec::new(),
            note: String::new(),
        })
    }

    /// Port of mod_replaces.verdict_of.
    pub fn verdict_of(&self, key: &str) -> Option<&str> {
        let (cls, rest) = key.split_once('#').unwrap_or((key, ""));
        let name = rest.split('(').next().unwrap_or("");
        let simple = cls.rsplit('/').next().unwrap_or(cls);
        let inner = simple.rsplit('$').next().unwrap_or(simple);
        [simple, inner]
            .into_iter()
            .find_map(|c| self.verdicts.get(&format!("{c}.{name}")))
            .map(String::as_str)
    }
}

/// Port of mod_replaces.route.
pub fn route(mechanism: &str, native: &NativeInfo, key: &str, k: &Knowledge) -> &'static str {
    if mechanism == "access" {
        return "inert";
    }
    if mechanism == "listener" {
        return "native-hook";
    }
    if mechanism == "mixin" && WEAK_STATUSES.contains(&native.status.as_str()) {
        let verdict = k.verdict_of(key);
        return match verdict {
            Some("port" | "inline") => "java",
            Some(v) if v == "missing" || v.starts_with("skip:") => "java-already",
            _ => "undecided",
        };
    }
    if mechanism == "implement" || !native.owned {
        return "java-already";
    }
    if mechanism == "mixin" {
        return "java";
    }
    if key.starts_with("net/minecraft/client/gui/") {
        "java-already"
    } else {
        "java-own-type"
    }
}

/// Port of mod_replaces.top_routing.
pub fn top_routing(routes: &[&str]) -> &'static str {
    routes
        .iter()
        .filter_map(|r| ROUTING_ORDER.iter().position(|o| o == r))
        .min()
        .map_or("", |i| ROUTING_ORDER[i])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn jar(files: &[(&str, &str)]) -> ZipArchive<Cursor<Vec<u8>>> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, text) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        ZipArchive::new(zip.finish().unwrap()).unwrap()
    }

    fn sample() -> Knowledge {
        let native = "net/minecraft/world/Base#tick\tported\t1\tcrates/a.rs:7\t\n\
                      net/minecraft/world/Base#must\tabsent\t0\t\tnothing\n\
                      net/minecraft/world/Base#other\tnone\t0\t\t\n";
        let verdicts = "Base.must\tmissing\nBase.tock\tport\nInner.sk\tskip:later\n";
        let mut zip = jar(&[
            (NATIVE, native),
            (VERDICTS, verdicts),
            (HOST_WORDS, "alpha\nbeta\n"),
        ]);
        Knowledge::from_archive(&mut zip, "boot.jar").unwrap()
    }

    #[test]
    fn info_reads_the_ledger_rows_and_misses_are_unknown() {
        let k = sample();
        let tick = k.info("net/minecraft/world/Base#tick");
        assert_eq!(tick.status, "ported");
        assert!(tick.owned);
        assert_eq!(tick.fns, ["crates/a.rs:7"]);
        let must = k.info("net/minecraft/world/Base#must");
        assert_eq!((must.status.as_str(), must.owned), ("absent", false));
        assert_eq!(must.note, "nothing");
        assert_eq!(k.info("net/minecraft/world/Base#other").status, "none");
        assert_eq!(k.info("net/minecraft/world/Base#zzz").status, "unknown");
        assert!(k.host_words.contains("beta"));
        assert_eq!(k.host_words.len(), 2);
    }

    #[test]
    fn mixins_route_by_ownership_then_by_the_verdict_map() {
        let k = sample();
        let tick = "net/minecraft/world/Base#tick()V";
        let must = "net/minecraft/world/Base#must()V";
        let tock = "net/minecraft/world/Base#tock()V";
        assert_eq!(
            route("mixin", &k.info("net/minecraft/world/Base#tick"), tick, &k),
            "java"
        );
        assert_eq!(
            route("mixin", &k.info("net/minecraft/world/Base#must"), must, &k),
            "java-already"
        );
        assert_eq!(
            route("mixin", &k.info("net/minecraft/world/Base#tock"), tock, &k),
            "java"
        );
        assert_eq!(
            route(
                "mixin",
                &k.info("net/minecraft/world/Base#zzz"),
                "net/minecraft/world/Base#zzz()V",
                &k
            ),
            "undecided"
        );
        let skipped = "net/minecraft/Outer$Inner#sk()V";
        assert_eq!(route("mixin", &k.info("x"), skipped, &k), "java-already");
        assert_eq!(k.verdict_of("pkg/Outer$Inner#sk()V"), Some("skip:later"));
    }

    #[test]
    fn other_mechanisms_route_by_their_kind() {
        let k = sample();
        let owned = k.info("net/minecraft/world/Base#tick");
        assert_eq!(
            route(
                "override",
                &owned,
                "net/minecraft/client/gui/Screen#init()V",
                &k
            ),
            "java-already"
        );
        assert_eq!(
            route("override", &owned, "net/minecraft/world/Base#tick()V", &k),
            "java-own-type"
        );
        assert_eq!(
            route("implement", &owned, "net/minecraft/world/Base#tick()V", &k),
            "java-already"
        );
        assert_eq!(route("access", &owned, "x", &k), "inert");
        assert_eq!(route("listener", &owned, "x", &k), "native-hook");
        assert_eq!(
            top_routing(&["inert", "java-already", "undecided"]),
            "undecided"
        );
    }

    #[test]
    fn escaped_text_survives_and_a_missing_resource_is_named() {
        let note = "a\\tb\\nc\\\\d";
        let native = format!("k#m\tskipped\t0\t\t{note}\n");
        let mut zip = jar(&[(NATIVE, &native), (VERDICTS, ""), (HOST_WORDS, "")]);
        let k = Knowledge::from_archive(&mut zip, "boot.jar").unwrap();
        assert_eq!(k.info("k#m").note, "a\tb\nc\\d");
        let mut zip = jar(&[(NATIVE, ""), (HOST_WORDS, "")]);
        let err = Knowledge::from_archive(&mut zip, "boot.jar").err().unwrap();
        assert_eq!(
            err,
            "boot.jar has no analysis/verdicts.tsv: build the host jars again (java_mods.py build)"
        );
    }
}
