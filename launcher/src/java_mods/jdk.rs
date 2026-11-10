//! The JDK 25 the embedded JVM runs on: one the player names, or Eclipse Temurin from Adoptium's
//! API, checked against the sha256 it publishes and unpacked into the install.

use crate::i18n::{tr, trf};
use crate::sources::Os;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

pub const FEATURE: u32 = 25;
const MARKER: &str = ".horizon-jdk";

/// The archive Adoptium offers for this platform.
#[derive(Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub sha256: String,
    pub release: String,
}

/// Adoptium's (os, architecture) names for a platform.
pub fn platform(os: Os, arch: &str) -> Result<(&'static str, &'static str), String> {
    let name = match os {
        Os::Linux => "linux",
        Os::MacOs => "mac",
        Os::Windows => "windows",
    };
    let arch = match arch {
        "x86_64" | "amd64" | "x64" => "x64",
        "aarch64" | "arm64" => "aarch64",
        _ => {
            return Err(trf(
                "There is no JDK {feature} download for {arch}: choose one with --java.",
                &[("feature", &FEATURE.to_string()), ("arch", arch)],
            ))
        }
    };
    Ok((name, arch))
}

pub fn query(os: &str, arch: &str) -> String {
    format!(
        "https://api.adoptium.net/v3/assets/latest/{FEATURE}/hotspot?architecture={arch}&image_type=jdk&os={os}&vendor=eclipse"
    )
}

/// The archive to fetch out of Adoptium's answer: tar.gz everywhere but Windows, which gets a zip.
pub fn asset(listing: &Value, os: Os) -> Result<Asset, String> {
    let first = listing
        .get(0)
        .ok_or(tr("Adoptium offers no JDK for this platform"))?;
    let package = &first["binary"]["package"];
    let field = |key: &str| {
        package[key]
            .as_str()
            .map(String::from)
            .ok_or_else(|| trf("Adoptium's answer has no {key}", &[("key", key)]))
    };
    let name = field("name")?;
    let expected: &[&str] = if os == Os::Windows {
        &[".zip"]
    } else {
        &[".tar.gz", ".tgz"]
    };
    if !expected.iter().any(|e| name.ends_with(e)) {
        return Err(trf("Unexpected JDK archive {name}", &[("name", &name)]));
    }
    if !package["link"]
        .as_str()
        .is_some_and(|l| l.starts_with("https://"))
    {
        return Err(tr("Adoptium's download link is not https").into());
    }
    Ok(Asset {
        url: field("link")?,
        sha256: field("checksum")?.to_ascii_lowercase(),
        release: first["release_name"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
        name,
    })
}

/// `bin/java` of a JDK home.
pub fn java_program(home: &Path) -> PathBuf {
    home.join("bin")
        .join(if cfg!(windows) { "java.exe" } else { "java" })
}

/// The feature version of a JDK home (its `release` file), when it has a `java` program.
pub fn major(home: &Path) -> Option<u32> {
    let (_, major) = crate::java::release_version(home)?;
    java_program(home).is_file().then_some(major)
}

/// The JDK home inside an unpacked archive: `jdk-25+8/`, or `jdk-25+8/Contents/Home` on macOS.
pub fn find(root: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut children: Vec<PathBuf> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    children.sort();
    found.extend(children.iter().map(|c| c.join("Contents/Home")));
    found.extend(children);
    found.push(root.to_path_buf());
    found.into_iter().find(|c| major(c) == Some(FEATURE))
}

/// A JDK 25 the player named, refused when it is not one.
pub fn given(home: &Path) -> Result<PathBuf, String> {
    let mut home = fs::canonicalize(home).map_err(|e| format!("{}: {e}", home.display()))?;
    // Nix's package is a wrapper around lib/openjdk.
    if major(&home).is_none() && major(&home.join("lib/openjdk")).is_some() {
        home = home.join("lib/openjdk");
    }
    match major(&home) {
        Some(FEATURE) => Ok(home),
        Some(other) => Err(trf(
            "{path} is Java {other}, not a JDK {feature}.",
            &[
                ("path", &home.display().to_string()),
                ("other", &other.to_string()),
                ("feature", &FEATURE.to_string()),
            ],
        )),
        None => Err(trf(
            "{path} is not a JDK (no bin/java, or no release file).",
            &[("path", &home.display().to_string())],
        )),
    }
}

/// Unpacks a `.tar.gz` or `.zip` into `dest`; entries that would leave it are refused.
pub fn extract(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    if archive.extension().is_some_and(|e| e == "zip") {
        let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        zip.extract(dest).map_err(|e| e.to_string())
    } else {
        let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(file));
        tar.set_preserve_permissions(true);
        for entry in tar.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            let unpacked = entry.unpack_in(dest).map_err(|e| e.to_string())?;
            if !unpacked {
                return Err(tr("The JDK archive holds a path outside its folder").into());
            }
        }
        Ok(())
    }
}

/// The JDK home the install runs on: `chosen`, else the Temurin already unpacked in the install,
/// else Temurin downloaded into `cache` (sha256-checked) and unpacked.
pub fn provide(
    install: &Path,
    cache: &Path,
    chosen: Option<&Path>,
    progress: super::Progress,
) -> Result<PathBuf, String> {
    if let Some(home) = chosen {
        return given(home);
    }
    let target = install.join("jdk");
    if let Some(home) = target
        .join(MARKER)
        .is_file()
        .then(|| find(&target))
        .flatten()
    {
        return Ok(home);
    }
    let (os, arch) = platform(Os::current(), std::env::consts::ARCH)?;
    progress(tr("Looking up Java 25 (Eclipse Temurin)"), None);
    let listing: Value = serde_json::from_slice(&crate::backend::get_bytes(&query(os, arch))?)
        .map_err(|e| trf("Adoptium's answer: {error}", &[("error", &e.to_string())]))?;
    let asset = asset(&listing, Os::current())?;
    let archive = cache.join("jdk").join(&asset.name);
    super::download(
        &asset.url,
        &archive,
        super::Check::Sha256(&asset.sha256),
        &|f| {
            progress(
                &trf("Downloading {name}", &[("name", &asset.release)]),
                Some(f),
            )
        },
    )?;
    progress(&trf("Unpacking {name}", &[("name", &asset.release)]), None);
    let building = install.join(format!("jdk.{}", std::process::id()));
    let _ = fs::remove_dir_all(&building);
    extract(&archive, &building)?;
    fs::write(building.join(MARKER), &asset.sha256).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&target);
    fs::rename(&building, &target).map_err(|e| e.to_string())?;
    find(&target).ok_or_else(|| tr("The unpacked JDK has no java program").into())
}
