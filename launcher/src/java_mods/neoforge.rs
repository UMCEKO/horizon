//! NeoForge's own installers, run once into the cache: the server's, whose libraries and
//! `unix_args.txt` start the dedicated server, and the client's, whose profile and libraries (with
//! the vanilla ones it inherits, fetched as the official launcher does) start the game window.

use super::args::Profiles;
use super::{Check, Progress};
use crate::i18n::{tr, trf};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const MAVEN: &str = "https://maven.neoforged.net/releases/net/neoforged/neoforge";

/// What the installers left in the cache.
pub struct Installed {
    pub server_home: PathBuf,
    pub client_home: PathBuf,
    pub profiles: Profiles,
}

fn installer(version: &str, cache: &Path, progress: Progress) -> Result<PathBuf, String> {
    let jar = cache.join(format!("neoforge-{version}-installer.jar"));
    if jar.is_file() {
        return Ok(jar);
    }
    let url = format!("{MAVEN}/{version}/neoforge-{version}-installer.jar");
    let sum = crate::backend::get_bytes(&format!("{url}.sha256"))
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .and_then(|t| t.split_whitespace().next().map(str::to_ascii_lowercase))
        .filter(|s| s.len() == 64)
        .ok_or(tr("NeoForge's maven publishes no sha256 for its installer"))?;
    super::download(&url, &jar, Check::Sha256(&sum), &|f| {
        progress(tr("Downloading NeoForge's installer"), Some(f))
    })?;
    Ok(jar)
}

/// Runs the installer on `java` with `args`; its output is kept in `log`, its tail is the error.
fn run(java: &Path, jar: &Path, args: &[&Path], cwd: &Path, log: &Path) -> Result<(), String> {
    let file = fs::File::create(log).map_err(|e| format!("{}: {e}", log.display()))?;
    let status = crate::process::command(java)
        .arg("-jar")
        .arg(jar)
        .args(args)
        .current_dir(cwd)
        .stdin(std::process::Stdio::null())
        .stdout(file.try_clone().map_err(|e| e.to_string())?)
        .stderr(file)
        .status()
        .map_err(|e| {
            trf(
                "Could not run {path}: {error}",
                &[
                    ("path", &java.display().to_string()),
                    ("error", &e.to_string()),
                ],
            )
        })?;
    if status.success() {
        return Ok(());
    }
    let text = fs::read_to_string(log).unwrap_or_default();
    let tail: Vec<&str> = text.lines().rev().take(8).collect();
    Err(trf(
        "NeoForge's installer failed ({status}); see {path}\n{tail}",
        &[
            ("status", &status.to_string()),
            ("path", &log.display().to_string()),
            (
                "tail",
                &tail.into_iter().rev().collect::<Vec<_>>().join("\n"),
            ),
        ],
    ))
}

fn read(path: &Path) -> Result<Value, String> {
    fs::read_to_string(path)
        .map_err(|e| format!("{}: {e}", path.display()))
        .and_then(|t| serde_json::from_str(&t).map_err(|e| format!("{}: {e}", path.display())))
}

/// `names` holds every library name of a manifest: a plain `natives-<os>` jar is the x64 build
/// when an ARM variant of it is listed too.
fn native_matches(library: &Value, names: &HashSet<&str>) -> bool {
    let arm = cfg!(target_arch = "aarch64");
    let name = library["name"].as_str().unwrap_or_default();
    let classifier = name.split(':').nth(3).unwrap_or_default();
    let ends = |tokens: &[&str]| {
        tokens
            .iter()
            .any(|t| classifier == *t || classifier.ends_with(&format!("-{t}")))
    };
    if ends(&["arm64", "aarch64", "aarch_64"]) {
        arm
    } else if ends(&["x86_64", "x64"]) {
        !arm
    } else if arm && classifier.starts_with("natives-") {
        !["arm64", "aarch64"]
            .iter()
            .any(|s| names.contains(format!("{name}-{s}").as_str()))
    } else {
        true
    }
}

/// The client's library jars (relative to `libraries/`): NeoForge's, then the vanilla ones it
/// inherits, those this system's rules allow; missing files are downloaded, sha1-checked.
fn client_jars(
    home: &Path,
    neo: &Value,
    base: &Value,
    progress: Progress,
) -> Result<Vec<String>, String> {
    let os = crate::java_launch::os_name();
    let mut jars: Vec<String> = Vec::new();
    for manifest in [neo, base] {
        let libraries = manifest["libraries"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let names: HashSet<&str> = libraries
            .iter()
            .filter_map(|l| l["name"].as_str())
            .collect();
        for (i, library) in libraries.iter().enumerate() {
            let artifact = &library["downloads"]["artifact"];
            if artifact.is_null()
                || !crate::java_launch::allowed(&library["rules"], os, &[])
                || !native_matches(library, &names)
            {
                continue;
            }
            let path = artifact["path"].as_str().ok_or("A library has no path")?;
            let file = home.join("libraries").join(crate::mojang::relative(path)?);
            if !file.is_file() {
                let (url, sha1) = (
                    artifact["url"].as_str().ok_or(tr("A library has no url"))?,
                    artifact["sha1"]
                        .as_str()
                        .ok_or(tr("A library has no sha1"))?,
                );
                progress(
                    tr("Downloading the game's libraries"),
                    Some(i as f32 / libraries.len().max(1) as f32),
                );
                super::download(url, &file, Check::Sha1(sha1), &|_| {})?;
            }
            if !jars.iter().any(|j| j == path) {
                jars.push(path.to_string());
            }
        }
    }
    Ok(jars)
}

/// The profile's JVM arguments that apply here, as plain strings.
fn jvm_arguments(neo: &Value) -> Vec<String> {
    let os = crate::java_launch::os_name();
    let mut out = Vec::new();
    for argument in neo["arguments"]["jvm"].as_array().into_iter().flatten() {
        match argument {
            Value::String(s) => out.push(s.clone()),
            Value::Object(_) if crate::java_launch::allowed(&argument["rules"], os, &[]) => {
                match &argument["value"] {
                    Value::String(s) => out.push(s.clone()),
                    Value::Array(list) => {
                        out.extend(list.iter().filter_map(|v| v.as_str().map(String::from)))
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    out
}

/// Both installers, once per version: the server's libraries and the client's profile.
pub fn install(
    version: &str,
    cache: &Path,
    java: &Path,
    progress: Progress,
) -> Result<Installed, String> {
    let root = cache.join("neoforge");
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let jar = installer(version, &root, progress)?;
    let server_home = root.join(version);
    let unix_args = server_home
        .join("libraries/net/neoforged/neoforge")
        .join(version)
        .join("unix_args.txt");
    if !unix_args.is_file() {
        progress(
            tr("Running NeoForge's server installer (a few minutes the first time)"),
            None,
        );
        fs::create_dir_all(&server_home).map_err(|e| e.to_string())?;
        run(
            java,
            &jar,
            &[Path::new("--installServer"), &server_home],
            &root,
            &root.join(format!("install-{version}.log")),
        )?;
    }
    let client_home = root.join(format!("client-{version}"));
    let profile = client_home
        .join("versions")
        .join(format!("neoforge-{version}"))
        .join(format!("neoforge-{version}.json"));
    if !profile.is_file() {
        progress(
            tr("Running NeoForge's client installer (a few minutes the first time)"),
            None,
        );
        fs::create_dir_all(&client_home).map_err(|e| e.to_string())?;
        fs::write(
            client_home.join("launcher_profiles.json"),
            "{\"profiles\":{}}",
        )
        .map_err(|e| e.to_string())?;
        run(
            java,
            &jar,
            &[Path::new("--installClient"), &client_home],
            &root,
            &root.join(format!("install-client-{version}.log")),
        )?;
    }
    let neo = read(&profile)?;
    let parent = neo["inheritsFrom"]
        .as_str()
        .ok_or(tr("NeoForge's profile has no inheritsFrom"))?;
    let base = read(
        &client_home
            .join("versions")
            .join(crate::mojang::relative(parent)?)
            .join(format!("{parent}.json")),
    )?;
    let game = neo["arguments"]["game"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    let profiles = Profiles {
        client_jvm: jvm_arguments(&neo),
        client_game: game,
        client_jars: client_jars(&client_home, &neo, &base, progress)?,
        server_args: fs::read_to_string(&unix_args)
            .map_err(|e| format!("{}: {e}", unix_args.display()))?,
    };
    Ok(Installed {
        server_home,
        client_home,
        profiles,
    })
}
