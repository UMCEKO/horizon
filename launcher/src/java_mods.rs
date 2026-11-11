//! Java mods for Horizon (docs/java-mods.md): a folder of NeoForge 26.3 mod jars the game runs in
//! its embedded JVM. The launcher builds the install the game needs on this computer: a JDK 25
//! (Eclipse Temurin, sha256-checked, or one the player names), NeoForge's libraries from its own
//! installers, the player's jars, and Horizon's host, boot and plugin jars from the game version.
//! `refresh` writes the arguments and the vanilla asset index again at every launch, so a moved
//! install, a new game version or newly prepared resources never leave stale paths.
//!
//! Files: `<data>/java-mods/{config.json, cache/, install/}`; the install holds `jdk/`,
//! `libraries/`, `host/`, `mods/`, `client/`, `server/`, `assets-index/`, `install.json` and
//! `profiles.json`.

pub mod args;
pub mod jdk;
pub mod neoforge;

use crate::backend;
use crate::i18n::{tr, trf};
use crate::sources::Os;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The NeoForge release Horizon's host runs (docs/java-mods.md).
pub const NEOFORGE: &str = "26.3.0.101";
/// The jars a game release carries in `java-mods/` beside its program: Horizon's own code.
pub const HOST_JARS: [&str; 3] = [
    "horizon-java-host.jar",
    "horizon-java-boot.jar",
    "horizon-java-plugin.jar",
];
const FORMAT: u32 = 1;
/// A folder of a game version marks a local build with this file: its text names the folder of the
/// built jars (`.local/java-mods`), used when the version carries none.
pub const LOCAL_BUILD: &str = ".local-build";
/// A string only a game built with the `java-mods` feature holds (the host's own variable).
const FEATURE_MARK: &[u8] = b"HORIZON_JAVA_HOST_JAR";

/// What a long step says it is doing, with how far it is when it knows.
pub type Progress<'a> = &'a dyn Fn(&str, Option<f32>);

// ----- Settings ----------------------------------------------------------------------------------

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// The folder of mod jars.
    #[serde(default)]
    pub pack: Option<PathBuf>,
    /// A JDK 25 home to use instead of downloading Temurin.
    #[serde(default)]
    pub java: Option<PathBuf>,
    /// Whether Play starts the game with the pack.
    #[serde(default)]
    pub enabled: bool,
}

pub fn root() -> PathBuf {
    backend::data_dir().join("java-mods")
}

pub fn install_dir() -> PathBuf {
    root().join("install")
}

fn cache_dir() -> PathBuf {
    root().join("cache")
}

fn config_file() -> PathBuf {
    root().join("config.json")
}

impl Config {
    pub fn load() -> Config {
        fs::read_to_string(config_file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::create_dir_all(root()).map_err(|e| e.to_string())?;
        fs::write(config_file(), text).map_err(|e| e.to_string())
    }
}

/// The jars of a pack folder, by name.
pub fn pack_jars(pack: &Path) -> Vec<PathBuf> {
    let mut jars: Vec<PathBuf> = fs::read_dir(pack)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("jar")))
        .collect();
    jars.sort();
    jars
}

// ----- What is installed -------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ModFile {
    pub name: String,
    pub size: u64,
}

/// `install.json`: what the install was built from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub format: u32,
    pub neoforge: String,
    /// The JDK home: relative to the install when inside it, else as named.
    pub java: String,
    pub mods: Vec<ModFile>,
}

impl Record {
    pub fn java_home(&self, install: &Path) -> PathBuf {
        let java = Path::new(&self.java);
        if java.is_absolute() {
            java.to_path_buf()
        } else {
            install.join(java)
        }
    }
}

pub fn read_record(install: &Path) -> Option<Record> {
    let record: Record =
        serde_json::from_str(&fs::read_to_string(install.join("install.json")).ok()?).ok()?;
    (record.format == FORMAT).then_some(record)
}

fn read_profiles(install: &Path) -> Result<args::Profiles, String> {
    let file = install.join("profiles.json");
    fs::read_to_string(&file)
        .map_err(|e| format!("{}: {e}", file.display()))
        .and_then(|t| serde_json::from_str(&t).map_err(|e| format!("{}: {e}", file.display())))
}

fn pack_files(pack: &Path) -> Vec<ModFile> {
    pack_jars(pack)
        .iter()
        .map(|p| ModFile {
            name: p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            size: fs::metadata(p).map_or(0, |m| m.len()),
        })
        .collect()
}

/// Where the installed state stands, for the page and the CLI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    NoPack,
    EmptyPack,
    NotInstalled,
    /// The pack or the JDK choice changed since the install.
    Stale,
    Ready,
}

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub state: State,
    pub pack: Option<PathBuf>,
    pub mods: usize,
    pub enabled: bool,
    pub neoforge: String,
    pub java: Option<PathBuf>,
    pub install: PathBuf,
}

impl Status {
    pub fn now() -> Status {
        Self::of(&Config::load(), &install_dir())
    }

    pub fn of(config: &Config, install: &Path) -> Status {
        let record = read_record(install);
        let files = config.pack.as_deref().map(pack_files);
        let state = match (&config.pack, &files, &record) {
            (None, _, _) => State::NoPack,
            (_, Some(f), _) if f.is_empty() => State::EmptyPack,
            (_, _, None) => State::NotInstalled,
            (_, Some(f), Some(r))
                if *f != r.mods
                    || r.neoforge != NEOFORGE
                    || config
                        .java
                        .as_ref()
                        .is_some_and(|j| r.java_home(install) != *j) =>
            {
                State::Stale
            }
            _ => State::Ready,
        };
        Status {
            state,
            pack: config.pack.clone(),
            mods: files.map_or(0, |f| f.len()),
            enabled: config.enabled,
            neoforge: NEOFORGE.into(),
            java: record.map(|r| r.java_home(install)),
            install: install.to_path_buf(),
        }
    }

    /// One line for the page and the CLI.
    pub fn say(&self) -> String {
        match self.state {
            State::NoPack => tr("No mod folder chosen yet.").into(),
            State::EmptyPack => tr("The folder holds no .jar files.").into(),
            State::NotInstalled => trf(
                "{count} mods chosen, not installed yet.",
                &[("count", &self.mods.to_string())],
            ),
            State::Stale => trf(
                "{count} mods chosen; the install is out of date: Install / Update.",
                &[("count", &self.mods.to_string())],
            ),
            State::Ready => trf(
                "{count} mods installed (NeoForge {neoforge}).",
                &[
                    ("count", &self.mods.to_string()),
                    ("neoforge", &self.neoforge),
                ],
            ),
        }
    }
}

// ----- Downloads and files -----------------------------------------------------------------------

/// What a download must hash to.
pub enum Check<'a> {
    Sha256(&'a str),
    Sha1(&'a str),
}

enum Hasher {
    Sha256(sha2::Sha256),
    Sha1(sha1::Sha1),
}

impl Hasher {
    fn of(check: &Check) -> Hasher {
        match check {
            Check::Sha256(_) => Hasher::Sha256(sha2::Digest::new()),
            Check::Sha1(_) => Hasher::Sha1(sha1::Digest::new()),
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        match self {
            Hasher::Sha256(h) => sha2::Digest::update(h, bytes),
            Hasher::Sha1(h) => sha1::Digest::update(h, bytes),
        }
    }

    fn hex(self) -> String {
        let bytes = match self {
            Hasher::Sha256(h) => sha2::Digest::finalize(h).to_vec(),
            Hasher::Sha1(h) => sha1::Digest::finalize(h).to_vec(),
        };
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

fn expected<'a>(check: &Check<'a>) -> &'a str {
    match check {
        Check::Sha256(s) | Check::Sha1(s) => s,
    }
}

fn hash_file(path: &Path, check: &Check) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Hasher::of(check);
    let mut buffer = vec![0; 1 << 20];
    loop {
        match file.read(&mut buffer).ok()? {
            0 => return Some(hasher.hex()),
            n => hasher.update(&buffer[..n]),
        }
    }
}

/// `url` into `dest`, kept: a file already there with the right checksum is reused, and a bad
/// download never takes its place.
pub fn download(
    url: &str,
    dest: &Path,
    check: Check,
    fraction: &dyn Fn(f32),
) -> Result<(), String> {
    let want = expected(&check).to_ascii_lowercase();
    if hash_file(dest, &check).as_deref() == Some(want.as_str()) {
        return Ok(());
    }
    fs::create_dir_all(dest.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
    let part = dest.with_extension(format!("{}.part", std::process::id()));
    let result = (|| {
        let response = backend::get(url)?;
        let total: Option<u64> = response
            .header("Content-Length")
            .and_then(|v| v.parse().ok());
        let mut reader = response.into_reader();
        let mut out = fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
        let mut hasher = Hasher::of(&check);
        let mut buffer = vec![0; 1 << 16];
        let mut done = 0u64;
        loop {
            let n = reader
                .read(&mut buffer)
                .map_err(|e| format!("{url}: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            out.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            if let Some(total) = total.filter(|t| *t > 0) {
                fraction(done as f32 / total as f32);
            }
        }
        out.flush().map_err(|e| e.to_string())?;
        let got = hasher.hex();
        if got != want {
            return Err(trf(
                "{url} is not the file expected (checksum {got})",
                &[("url", url), ("got", &got)],
            ));
        }
        fs::rename(&part, dest).map_err(|e| format!("{}: {e}", dest.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

/// `dest` as a hard link (a copy across disks) of `source`, left alone when it already is one.
fn sync_file(source: &Path, dest: &Path) -> Result<(), String> {
    let Ok(source) = fs::canonicalize(source) else {
        return Err(trf(
            "{path} is missing",
            &[("path", &source.display().to_string())],
        ));
    };
    let source = source.as_path();
    let (Ok(from), to) = (fs::metadata(source), fs::symlink_metadata(dest)) else {
        return Err(trf(
            "{path} is missing",
            &[("path", &source.display().to_string())],
        ));
    };
    if let Ok(to) = &to {
        let same = !to.file_type().is_symlink()
            && to.len() == from.len()
            && to.modified().ok() == from.modified().ok();
        if same {
            return Ok(());
        }
        fs::remove_file(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    fs::create_dir_all(dest.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
    fs::hard_link(source, dest)
        .or_else(|_| fs::copy(source, dest).map(|_| ()))
        .map_err(|e| format!("{}: {e}", dest.display()))
}

fn files_under(root: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn sync_tree(source: &Path, dest: &Path, progress: Progress) -> Result<(), String> {
    let mut files = Vec::new();
    files_under(source, &mut files);
    for (i, file) in files.iter().enumerate() {
        if i % 200 == 0 {
            progress(
                tr("Copying NeoForge's libraries"),
                Some(i as f32 / files.len() as f32),
            );
        }
        let relative = file.strip_prefix(source).map_err(|e| e.to_string())?;
        sync_file(file, &dest.join(relative))?;
    }
    Ok(())
}

/// A link to `target` (a symlink; a hard link or copy of a file where links aren't allowed).
fn link(target: &Path, at: &Path) -> Result<(), String> {
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(target, at);
    #[cfg(windows)]
    let made = if target.is_dir() {
        crate::sources::junction(target, at)
    } else {
        fs::hard_link(target, at).or_else(|_| fs::copy(target, at).map(|_| ()))
    };
    made.map_err(|e| format!("{}: {e}", at.display()))
}

/// `folder` holds exactly links to `targets` (jars), each pointing where it should.
fn sync_links(folder: &Path, targets: &[PathBuf]) -> Result<(), String> {
    fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let wanted: Vec<&std::ffi::OsStr> = targets.iter().filter_map(|t| t.file_name()).collect();
    for old in fs::read_dir(folder).into_iter().flatten().flatten() {
        let name = old.file_name();
        let jar = Path::new(&name).extension().is_some_and(|e| e == "jar");
        if jar && !wanted.contains(&name.as_os_str()) {
            let _ = fs::remove_file(old.path());
        }
    }
    for target in targets {
        let at = folder.join(target.file_name().unwrap_or_default());
        let right = match fs::read_link(&at) {
            Ok(now) => now == *target,
            Err(_) => cfg!(windows) && at.is_file(),
        };
        if cfg!(windows) && at.is_file() {
            sync_file(target, &at)?;
        } else if !right {
            let _ = fs::remove_file(&at);
            link(target, &at)?;
        }
    }
    Ok(())
}

fn write_if_changed(path: &Path, text: &str) -> Result<(), String> {
    if fs::read_to_string(path).is_ok_and(|now| now == text) {
        return Ok(());
    }
    fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

// ----- The game's side ---------------------------------------------------------------------------

/// The folder holding the host, boot and plugin jars of a game version: its own `java-mods/`, or
/// the one a local build names (`.local-build`).
pub fn host_jars_dir(tag: &str) -> Result<PathBuf, String> {
    host_jars_in(&backend::version_dir(tag), tag)
}

fn host_jars_in(version: &Path, tag: &str) -> Result<PathBuf, String> {
    let own = version.join("java-mods");
    if HOST_JARS.iter().all(|j| own.join(j).is_file()) {
        return Ok(own);
    }
    if let Ok(named) = fs::read_to_string(version.join(LOCAL_BUILD)) {
        let named = named.trim();
        let local = if named.is_empty() {
            std::env::var_os("HORIZON_JAVA_MODS_DIR").map(PathBuf::from)
        } else {
            Some(PathBuf::from(named))
        };
        if let Some(local) = local.filter(|l| HOST_JARS.iter().all(|j| l.join(j).is_file())) {
            return Ok(local);
        }
    }
    Err(trf(
        "{tag} carries no Java mods support (no java-mods/ folder with {jars}). Use a Horizon release built with it.",
        &[("tag", tag), ("jars", &HOST_JARS.join(", "))],
    ))
}

/// Whether game version `tag` can run Java mods: it carries the host jars and was built with the feature.
pub fn support(tag: &str) -> Result<(), String> {
    host_jars_dir(tag)?;
    if has_feature(&backend::version_dir(tag).join(backend::CLIENT)) {
        Ok(())
    } else {
        Err(trf(
            "{tag} was not built with Java mods support (the java-mods feature). Pick another version.",
            &[("tag", tag)],
        ))
    }
}

/// Whether the game program at `path` was built with the `java-mods` feature.
pub fn has_feature(program: &Path) -> bool {
    let Ok(mut file) = fs::File::open(program) else {
        return false;
    };
    let mut buffer = vec![0u8; 8 << 20];
    let mut carried = 0;
    loop {
        let Ok(n) = file.read(&mut buffer[carried..]) else {
            return false;
        };
        if n == 0 {
            return false;
        }
        let end = carried + n;
        if buffer[..end]
            .windows(FEATURE_MARK.len())
            .any(|w| w == FEATURE_MARK)
        {
            return true;
        }
        let keep = FEATURE_MARK.len() - 1;
        let from = end.saturating_sub(keep);
        buffer.copy_within(from..end, 0);
        carried = end - from;
    }
}

/// The id of the vanilla asset index a prepared Horizon was made from: recorded by preparing, or
/// found as the Minecraft index with the same bytes in the player's launchers' folders, or (a
/// local build's) in the Minecraft files beside it.
pub fn asset_index_id(resources: &Path) -> Result<String, String> {
    asset_index_id_in(resources, &backend::minecraft_folders())
}

fn asset_index_id_in(resources: &Path, folders: &[PathBuf]) -> Result<String, String> {
    if let Ok(id) = fs::read_to_string(resources.join("asset-index-id.txt")) {
        let id = id.trim();
        if !id.is_empty() && !id.contains(['/', '\\']) {
            return Ok(id.to_string());
        }
    }
    if let Some(id) = fs::read(resources.join("asset-index.json"))
        .ok()
        .and_then(|bytes| index_named_like(&bytes, folders))
    {
        return Ok(id);
    }
    let real = fs::canonicalize(resources).unwrap_or_else(|_| resources.to_path_buf());
    let beside = real
        .parent()
        .and_then(Path::parent)
        .zip(real.file_name())
        .map(|(local, version)| local.join("minecraft").join(version).join("manifest.json"));
    beside
        .and_then(|m| fs::read_to_string(m).ok())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["assetIndex"]["id"].as_str().map(String::from))
        .ok_or_else(|| {
            trf(
                "{path} does not say which asset index it was prepared from. Prepare the version again.",
                &[("path", &resources.display().to_string())],
            )
        })
}

/// The name of the index file in `folders` (`assets/indexes/<id>.json`) holding exactly `bytes`.
fn index_named_like(bytes: &[u8], folders: &[PathBuf]) -> Option<String> {
    folders.iter().find_map(|folder| {
        fs::read_dir(folder.join("assets/indexes"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .find(|e| fs::read(e.path()).is_ok_and(|b| b == bytes))
            .and_then(|e| {
                e.path()
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
            })
    })
}

// ----- Install -----------------------------------------------------------------------------------

/// Builds or brings up to date the install for the configured pack: the JDK, NeoForge's
/// libraries, the pack's jars and the host jars of game version `tag`. Downloads are kept in the
/// cache, so a second run fetches nothing it has.
pub fn install(tag: &str, progress: Progress) -> Result<Status, String> {
    let config = Config::load();
    let pack = config
        .pack
        .clone()
        .ok_or(tr("Choose the folder of mod jars first."))?;
    let jars = pack_jars(&pack);
    if jars.is_empty() {
        return Err(trf(
            "{path} holds no .jar files.",
            &[("path", &pack.display().to_string())],
        ));
    }
    let host = host_jars_dir(tag)?;
    let install = install_dir();
    let cache = cache_dir();
    fs::create_dir_all(&install).map_err(|e| e.to_string())?;
    let java = jdk::provide(&install, &cache, config.java.as_deref(), progress)?;
    let found = neoforge::install(NEOFORGE, &cache, &jdk::java_program(&java), progress)?;
    let libraries = install.join("libraries");
    for home in [&found.client_home, &found.server_home] {
        sync_tree(&home.join("libraries"), &libraries, progress)?;
    }
    progress(tr("Installing the mods"), None);
    sync_host_jars(&host, &install)?;
    let mods = install.join("mods");
    fs::create_dir_all(&mods).map_err(|e| e.to_string())?;
    let names: Vec<_> = jars.iter().filter_map(|j| j.file_name()).collect();
    for old in fs::read_dir(&mods).into_iter().flatten().flatten() {
        if !names.contains(&old.file_name().as_os_str()) {
            let _ = fs::remove_file(old.path());
        }
    }
    for jar in &jars {
        sync_file(jar, &mods.join(jar.file_name().unwrap_or_default()))?;
    }
    let java_text = java.strip_prefix(&install).map_or_else(
        |_| java.to_string_lossy().into_owned(),
        |r| r.to_string_lossy().into_owned(),
    );
    let record = Record {
        format: FORMAT,
        neoforge: NEOFORGE.into(),
        java: java_text,
        mods: pack_files(&pack),
    };
    write_if_changed(&install.join("profiles.json"), &json_text(&found.profiles)?)?;
    write_if_changed(&install.join("install.json"), &json_text(&record)?)?;
    progress(tr("Writing the launch arguments"), None);
    refresh(tag, &install)?;
    Ok(Status::now())
}

fn json_text<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value)
        .map(|t| t + "\n")
        .map_err(|e| e.to_string())
}

fn sync_host_jars(host: &Path, install: &Path) -> Result<(), String> {
    for jar in HOST_JARS {
        sync_file(&host.join(jar), &install.join("host").join(jar))?;
    }
    Ok(())
}

/// The jars a run folder links: the pack's and Horizon's boot and plugin.
fn mod_links(install: &Path) -> Vec<PathBuf> {
    let mut links = pack_jars(&install.join("mods"));
    links.extend(
        ["horizon-java-boot.jar", "horizon-java-plugin.jar"].map(|j| install.join("host").join(j)),
    );
    links
}

/// Makes `run` a folder the embedded JVM can start from (its mods, its libraries, its arguments)
/// for the installed game version `tag`, with every path as the install stands now.
pub fn prepare_run(tag: &str, install: &Path, run: &Path, dedicated: bool) -> Result<(), String> {
    let record = read_record(install).ok_or(tr(
        "The Java mods are not installed: press Install / Update.",
    ))?;
    let profiles = read_profiles(install)?;
    sync_host_jars(&host_jars_dir(tag)?, install)?;
    let libraries = install.join("libraries");
    let assets = if dedicated {
        None
    } else {
        let resources = backend::resources(tag);
        let id = asset_index_id(&resources)?;
        Some((args::assets_index(&resources, &id, install)?, id))
    };
    fs::create_dir_all(run).map_err(|e| e.to_string())?;
    sync_links(&run.join("mods"), &mod_links(install))?;
    let at = run.join("libraries");
    if fs::read_link(&at).ok().as_deref() != Some(libraries.as_path()) {
        let _ = fs::remove_file(&at);
        let _ = fs::remove_dir(&at);
        if !at.exists() {
            link(&libraries, &at)?;
        }
    }
    let lines = args::args_lines(
        Os::current(),
        dedicated,
        &libraries,
        &record.neoforge,
        &profiles,
        assets.as_ref().map(|(f, id)| (f.as_path(), id.as_str())),
        &record.java_home(install),
        &install.join("host").join(HOST_JARS[0]),
    );
    write_if_changed(&run.join("horizon-java.args"), &(lines.join("\n") + "\n"))
}

/// The client run folder and the server run folder of the install, brought up to date.
pub fn refresh(tag: &str, install: &Path) -> Result<(), String> {
    for dedicated in [false, true] {
        let run = install.join(if dedicated { "server" } else { "client" });
        prepare_run(tag, install, &run, dedicated)?;
    }
    Ok(())
}

// ----- Servers hosted with horizon-server ----------------------------------------------------------

fn server_mark(dir: &Path) -> PathBuf {
    dir.join("java-mods.enabled")
}

/// Whether the server in `dir` runs with the Java mods.
pub fn server_enabled(dir: &Path) -> bool {
    server_mark(dir).is_file()
}

pub fn set_server(dir: &Path, on: bool) -> Result<(), String> {
    if on {
        fs::write(server_mark(dir), "").map_err(|e| e.to_string())
    } else {
        let _ = fs::remove_file(server_mark(dir));
        Ok(())
    }
}

/// The environment `horizon-server` and its game get for the server in `dir`, with its run folder
/// `dir/java` made ready; the game version is the one the server programs were set up from.
pub fn server_env(tag: &str, dir: &Path) -> Result<Vec<(String, String)>, String> {
    let mut probe = Command::new("unused");
    apply(&mut probe, tag, true, Some(&dir.join("java")))?;
    Ok(probe
        .get_envs()
        .filter_map(|(k, v)| {
            Some((
                k.to_string_lossy().into_owned(),
                v?.to_string_lossy().into_owned(),
            ))
        })
        .collect())
}

// ----- Launch ------------------------------------------------------------------------------------

/// Gives the game command what the Java mods need: the run folder brought up to date and the
/// environment. `run` is the server's own folder for a dedicated server, else the install's
/// `client/`. Refused with a plain reason when the install isn't ready or the game lacks support.
pub fn apply(
    cmd: &mut Command,
    tag: &str,
    dedicated: bool,
    run: Option<&Path>,
) -> Result<(), String> {
    let config = Config::load();
    let install = install_dir();
    let status = Status::of(&config, &install);
    match status.state {
        State::Ready => {}
        State::Stale => {
            return Err(
                tr("The mod folder changed since the install: press Install / Update.").into(),
            )
        }
        _ => {
            return Err(trf(
                "{status} Press Install / Update.",
                &[("status", &status.say())],
            ))
        }
    }
    support(tag)?;
    let record = read_record(&install).ok_or(tr("The Java mods are not installed."))?;
    let run = match run {
        Some(run) => run.to_path_buf(),
        None => install.join(if dedicated { "server" } else { "client" }),
    };
    prepare_run(tag, &install, &run, dedicated)?;
    let env = args::launch_env(Os::current(), &run, &|name| std::env::var(name).ok());
    cmd.envs(env);
    Ok(())
}

#[cfg(test)]
#[path = "java_mods/tests.rs"]
mod tests;
