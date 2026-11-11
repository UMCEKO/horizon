//! What the embedded JVM starts with, per operating system: the `horizon-java.args` lines (JVM
//! options, classpath, FML arguments), the environment that lets the game find the JDK's
//! libraries, and the vanilla asset index NeoForge's client reads. All of it is a pure function of
//! the install's folders, so the launcher writes it again at every launch and a moved install works.

use crate::i18n::{tr, trf};
use crate::sources::Os;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The classpath separator of `os`.
pub fn separator(os: Os) -> char {
    if os == Os::Windows {
        ';'
    } else {
        ':'
    }
}

fn joined<I: IntoIterator<Item = String>>(os: Os, items: I) -> String {
    items
        .into_iter()
        .collect::<Vec<_>>()
        .join(&separator(os).to_string())
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// NeoForge's two launch descriptions, kept from the installers' output so the args can be
/// written again without them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Profiles {
    /// The client profile's JVM arguments (with `${library_directory}`) and game arguments.
    pub client_jvm: Vec<String>,
    pub client_game: Vec<String>,
    /// The client's library jars, relative to `libraries/`, in classpath order.
    pub client_jars: Vec<String>,
    /// The server installer's `unix_args.txt`.
    pub server_args: String,
}

/// The lines of `horizon-java.args` for the client (`dedicated` false) or the server, led by the JDK home and the host jar.
/// `assets` is the asset folder and its index id, used by the client only.
pub fn args_lines(
    os: Os,
    dedicated: bool,
    libs: &Path,
    neoforge: &str,
    profiles: &Profiles,
    assets: Option<(&Path, &str)>,
    java: &Path,
    host_jar: &Path,
) -> Vec<String> {
    let mut lines = vec![
        format!("java {}", text(java)),
        format!("host {}", text(host_jar)),
    ];
    let (jvm, game) = if dedicated {
        server_parts(os, libs, profiles, &mut lines)
    } else {
        lines.push("dist client".to_string());
        lines.push(format!(
            "cp {}",
            joined(os, profiles.client_jars.iter().map(|j| text(&libs.join(j))))
        ));
        let jvm = profiles
            .client_jvm
            .iter()
            .map(|a| a.replace("${library_directory}", &text(libs)))
            .collect();
        (jvm, profiles.client_game.clone())
    };
    let mut i = 0;
    while i < jvm.len() {
        if matches!(jvm[i].as_str(), "--add-opens" | "--add-exports") && i + 1 < jvm.len() {
            lines.push(format!("jvm {}={}", jvm[i], jvm[i + 1]));
            i += 2;
        } else {
            lines.push(format!("jvm {}", jvm[i]));
            i += 1;
        }
    }
    lines.extend(game.iter().map(|a| format!("prog {a}")));
    let universal = libs
        .join("net/neoforged/neoforge")
        .join(neoforge)
        .join(format!("neoforge-{neoforge}-universal.jar"));
    lines.push(format!("mod {}", text(&universal)));
    if let (false, Some((folder, id))) = (dedicated, assets) {
        lines.push("prog --assetsDir".into());
        lines.push(format!("prog {}", text(folder)));
        lines.push("prog --assetIndex".into());
        lines.push(format!("prog {id}"));
    }
    lines
}

/// The server's JVM and game arguments out of its `unix_args.txt`, and its `cp` line pushed on
/// `lines`. Its paths are relative to the folder holding `libraries/`.
fn server_parts(
    os: Os,
    libs: &Path,
    profiles: &Profiles,
    lines: &mut Vec<String>,
) -> (Vec<String>, Vec<String>) {
    let home = libs.parent().unwrap_or(libs);
    let args: Vec<&str> = profiles.server_args.lines().collect();
    let at = args.iter().position(|l| l.trim() == "-classpath");
    let Some(at) = at else {
        return (Vec::new(), Vec::new());
    };
    let entries = args.get(at + 1).map_or("", |l| l.trim());
    lines.push(format!(
        "cp {}",
        joined(
            os,
            entries
                .split(':')
                .filter(|e| !e.is_empty())
                .map(|e| text(&home.join(e)))
        )
    ));
    let (mut jvm, mut game) = (Vec::new(), Vec::new());
    for line in args[..at].iter().chain(args.get(at + 2..).unwrap_or(&[])) {
        let line = line.trim();
        let (head, rest) = line.split_once(' ').unwrap_or((line, ""));
        if matches!(head, "--add-opens" | "--add-exports") {
            jvm.push(head.to_string());
            jvm.push(rest.to_string());
        } else if head.starts_with("--fml") {
            game.push(head.to_string());
            game.push(rest.to_string());
        } else if head.starts_with("--") && !rest.is_empty() {
            jvm.push(format!("{head}={rest}"));
        } else if head.starts_with('-') {
            jvm.push(head.replace("=libraries", &format!("={}", text(libs))));
        }
    }
    (jvm, game)
}

/// The variable the loader reads for shared libraries.
pub fn library_path_var(os: Os) -> &'static str {
    match os {
        Os::Linux => "LD_LIBRARY_PATH",
        Os::MacOs => "DYLD_LIBRARY_PATH",
        Os::Windows => "PATH",
    }
}

/// The environment the game needs for the Java mods of an install: the GL driver folder in front of `current`'s library path, and the run folder.
pub fn launch_env(
    os: Os,
    run: &Path,
    current: &dyn Fn(&str) -> Option<String>,
) -> BTreeMap<String, String> {
    let var = library_path_var(os);
    let mut dirs: Vec<String> = Vec::new();
    if os == Os::Linux && Path::new("/run/opengl-driver/lib").is_dir() {
        dirs.push("/run/opengl-driver/lib".into());
    }
    dirs.extend(current(var).filter(|v| !v.is_empty()));
    let mut env = BTreeMap::new();
    env.insert(var.to_string(), joined(os, dirs));
    env.insert("HORIZON_JAVA_RUN".into(), text(run));
    env
}

/// Vanilla's assets folder (`indexes/<id>.json`, `objects/<2 hex>/<sha1>`) as links over the
/// prepared assets, built once per content under `root/assets-index/`.
pub fn assets_index(resources: &Path, index_id: &str, root: &Path) -> Result<PathBuf, String> {
    let index = resources.join("asset-index.json");
    let source: Value = fs::read_to_string(&index)
        .map_err(|e| format!("{}: {e}", index.display()))
        .and_then(|t| serde_json::from_str(&t).map_err(|e| format!("{}: {e}", index.display())))?;
    let assets = fs::canonicalize(resources.join("assets"))
        .map_err(|e| format!("{}: {e}", resources.join("assets").display()))?;
    let mut objects = serde_json::Map::new();
    let mut entries: Vec<(&String, &Value)> = source["objects"]
        .as_object()
        .ok_or(tr("The asset index has no objects"))?
        .iter()
        .collect();
    entries.sort_by_key(|(k, _)| k.as_str());
    for (name, entry) in entries {
        if assets.join(name).is_file() {
            objects.insert(name.clone(), entry.clone());
        }
    }
    let index_json = serde_json::to_string(&serde_json::json!({ "objects": objects }))
        .map_err(|e| e.to_string())?;
    let key = {
        use sha1::{Digest, Sha1};
        let digest = Sha1::digest(format!("{}\0{index_id}\0{index_json}", assets.display()));
        digest[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let parent = root.join("assets-index");
    let folder = parent.join(&key);
    let file = folder.join("indexes").join(format!("{index_id}.json"));
    if file.is_file() {
        return Ok(folder);
    }
    let building = parent.join(format!("{key}.{}", std::process::id()));
    let _ = fs::remove_dir_all(&building);
    fs::create_dir_all(building.join("indexes")).map_err(|e| e.to_string())?;
    for (name, entry) in &objects {
        let Some(hash) = entry["hash"]
            .as_str()
            .filter(|h| h.len() == 40 && h.bytes().all(|b| b.is_ascii_hexdigit()))
        else {
            return Err(trf(
                "Bad asset hash for {name} in the asset index",
                &[("name", name)],
            ));
        };
        let link = building.join("objects").join(&hash[..2]).join(hash);
        fs::create_dir_all(link.parent().unwrap_or(&building)).map_err(|e| e.to_string())?;
        if fs::symlink_metadata(&link).is_err() {
            link_file(&assets.join(name), &link)?;
        }
    }
    fs::write(
        building.join("indexes").join(format!("{index_id}.json")),
        index_json,
    )
    .map_err(|e| e.to_string())?;
    if fs::rename(&building, &folder).is_err() {
        let _ = fs::remove_dir_all(&building);
    }
    Ok(folder)
}

/// A symlink, or a hard link or copy where links aren't allowed (Windows without developer mode).
fn link_file(target: &Path, link: &Path) -> Result<(), String> {
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(target, link);
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(target, link);
    linked
        .or_else(|_| fs::hard_link(target, link))
        .or_else(|_| fs::copy(target, link).map(|_| ()))
        .map_err(|e| format!("{}: {e}", link.display()))
}
