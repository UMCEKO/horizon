// game-module-cfg: feature java-mods
//! Java mods' blocks and items as content, generated from the facts their boot exports and cached by what made them (`docs/java-mods.md`).
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicU8, Ordering},
    },
    time::Instant,
};

/// Bumped when carry writes something else from the same facts.
const GENERATION: u32 = 4;
const DONE: &str = "done";
const FACTS_HASH: &str = "facts.sha256";
const KEYS: &str = "keys.tsv";
const ENTITIES: &str = "entities.bin";
const ROUTES: &str = "routes.bin";
const ENTITY_TYPES: &str = "entity-types.bin";

static CARRIED: OnceLock<Vec<PathBuf>> = OnceLock::new();
/// The folder of this run's cache entry.
static CACHE_AT: OnceLock<PathBuf> = OnceLock::new();
/// The mods' configs as this run's boot found them, before the mods could write theirs.
static CONFIGS: OnceLock<Result<Vec<String>, String>> = OnceLock::new();
/// What the loading screen says while a launch waits on the mods: 0 nothing, 1 their boot, 2 their content.
static WAITING: AtomicU8 = AtomicU8::new(0);

/// The loading screen's line while the content waits on the Java mods.
pub(super) fn note() -> Option<&'static str> {
    match WAITING.load(Ordering::Relaxed) {
        1 => Some("Loading Java mods"),
        2 => Some("Building Java mods' content"),
        _ => None,
    }
}

fn cache_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join(".horizon-cache/java-mods")
}

/// Where this run's boot writes its facts.
fn facts_file() -> PathBuf {
    cache_dir().join(format!("boot-facts-{}.json", std::process::id()))
}

/// Starts the Java mods' boot, once the configs it starts from are in the fingerprint.
pub(super) fn start(bridges: Vec<horizon_java_host::bridge::Bridge>) {
    if let Some(run) = std::env::var_os("HORIZON_JAVA_RUN").map(PathBuf::from) {
        let _ = CONFIGS.set(configs(&run));
        horizon_client::mod_keys::expect_mod_keys();
    }
    horizon_java_host::start(bridges, facts_file());
}

/// The content folders of this run's Java mods: the cached ones, or, the first time these jars run, those generated once their boot is done.
pub(super) fn carried(resources: &Path) -> Vec<PathBuf> {
    CARRIED
        .get_or_init(|| {
            let Some(run) = std::env::var_os("HORIZON_JAVA_RUN").map(PathBuf::from) else {
                return Vec::new();
            };
            content(&run, resources).unwrap_or_else(|e| {
                log::error!("Horizon: Java mods' content not built: {e}");
                Vec::new()
            })
        })
        .clone()
}

fn content(run: &Path, resources: &Path) -> Result<Vec<PathBuf>, String> {
    let began = Instant::now();
    let dir = cache_dir();
    let key = fingerprint(run, resources)?;
    let at = dir.join(&key);
    let _ = CACHE_AT.set(at.clone());
    let next = dir.join(format!("{key}.next"));
    if next.join(DONE).exists() {
        let _ = fs::remove_dir_all(&at);
        fs::rename(&next, &at).map_err(|e| format!("{}: {e}", next.display()))?;
    }
    for old in fs::read_dir(&dir).into_iter().flatten().flatten() {
        let name = old.file_name().to_string_lossy().into_owned();
        if old.path().is_dir() && name != key && name != format!("{key}.next") {
            let _ = fs::remove_dir_all(old.path());
        }
    }
    if at.join(DONE).exists() {
        log::info!(
            "Horizon: Java mods' content cached ({key}), checked in {:.2}s",
            began.elapsed().as_secs_f64()
        );
        let mods = carried_in(&at);
        load_keys(&at.join(KEYS));
        let (run, resources) = (run.to_owned(), resources.to_owned());
        let _ = std::thread::Builder::new()
            .name("Java mods' facts".into())
            .spawn(move || verify(&run, &resources, &at, &next));
        return Ok(mods);
    }
    log::info!("Horizon: Java mods' content not cached ({key}): waiting for their boot");
    WAITING.store(1, Ordering::Relaxed);
    let host = horizon_java_host::get();
    let booted = began.elapsed().as_secs_f64();
    WAITING.store(2, Ordering::Relaxed);
    let result = match host {
        Some(host) => generate(host.facts(), &host.baked(), run, resources, &at),
        None => Err("their boot failed".into()),
    };
    WAITING.store(0, Ordering::Relaxed);
    let mods = result?;
    log::info!(
        "Horizon: Java mods' content generated: {} mods in {:.1}s ({booted:.1}s waiting for their boot)",
        mods.len(),
        began.elapsed().as_secs_f64()
    );
    Ok(mods)
}

/// After a cached launch, the live boot's facts against the ones the cache was built from.
fn verify(run: &Path, resources: &Path, at: &Path, next: &Path) {
    let Some(host) = horizon_java_host::get() else {
        return;
    };
    let live = match boot_hash(host.facts(), &host.baked()) {
        Ok(h) => h,
        Err(e) => {
            log::error!("Horizon: the Java mods' boot facts can't be read: {e}");
            return;
        }
    };
    refresh_keys(&host.keys(), &at.join(KEYS));
    refresh_file(&host.entity_facts(), &at.join(ENTITIES));
    refresh_file(&host.routes_file(), &at.join(ROUTES));
    let cached = fs::read_to_string(at.join(FACTS_HASH)).unwrap_or_default();
    if live == cached.trim() {
        let _ = fs::remove_file(host.facts());
        let _ = fs::remove_file(host.baked());
        let _ = fs::remove_file(host.keys());
        let _ = fs::remove_file(host.entity_facts());
        let _ = fs::remove_file(host.routes_file());
        return;
    }
    let warning = "Horizon: WARNING: the Java mods registered other blocks or items than the cached content was built from; this run's tables don't match them, and the next launch uses rebuilt ones";
    log::warn!("{warning}");
    eprintln!("{warning}");
    if let Err(e) = generate(host.facts(), &host.baked(), run, resources, next) {
        log::error!("Horizon: Java mods' content not rebuilt: {e}");
    }
}

/// Carries every mod jar of the run with `facts` into `dest`, which it replaces whole.
fn generate(
    facts: &Path,
    baked: &Path,
    run: &Path,
    resources: &Path,
    dest: &Path,
) -> Result<Vec<PathBuf>, String> {
    let partial = dest.with_extension("partial");
    let _ = fs::remove_dir_all(&partial);
    fs::create_dir_all(&partial).map_err(|e| format!("{}: {e}", partial.display()))?;
    let kept = partial.join("facts.json");
    fs::rename(facts, &kept)
        .or_else(|_| fs::copy(facts, &kept).map(drop))
        .map_err(|e| format!("{}: {e}", facts.display()))?;
    let kept_baked = partial.join("baked-models.json");
    let textures = baked.with_extension("textures");
    let kept_textures = kept_baked.with_extension("textures");
    if textures.exists() {
        copy_textures(&textures, &kept_textures)?;
    }
    if baked.exists() {
        fs::rename(baked, &kept_baked)
            .or_else(|_| fs::copy(baked, &kept_baked).map(drop))
            .map_err(|e| format!("{}: {e}", baked.display()))?;
    } else {
        log::warn!(
            "Horizon: the Java mods' boot baked no models, so blocks whose models their code builds have none"
        );
    }
    if let Some(host) = horizon_java_host::get() {
        let live = host.keys();
        if live.exists() {
            fs::copy(&live, partial.join(KEYS)).map_err(|e| format!("{}: {e}", live.display()))?;
            load_keys(&partial.join(KEYS));
        }
        let analysed = host.entity_facts();
        if analysed.exists() {
            fs::copy(&analysed, partial.join(ENTITIES))
                .map_err(|e| format!("{}: {e}", analysed.display()))?;
        }
        let routed = host.routes_file();
        if routed.exists() {
            fs::copy(&routed, partial.join(ROUTES))
                .map_err(|e| format!("{}: {e}", routed.display()))?;
        }
    }
    fs::write(partial.join(FACTS_HASH), boot_hash(&kept, &kept_baked)?)
        .map_err(|e| e.to_string())?;
    let log = |line: String| log::info!("Horizon: Java mods' content: {line}");
    horizon_port_pack::pipeline::carry_boot(
        &kept,
        kept_baked.exists().then_some(kept_baked.as_path()),
        &mod_jars(run)?,
        &classpath(run),
        resources,
        &partial.join("mods"),
        &log,
    )?;
    if kept_textures.exists() {
        copy_textures(
            &kept_textures,
            &partial.join("mods/horizon_java_textures/carry"),
        )?;
    }
    fs::write(partial.join(DONE), "").map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(dest);
    fs::rename(&partial, dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    Ok(carried_in(dest))
}

/// The Controls screen's mod keys from a boot's key list.
fn load_keys(path: &Path) {
    let Ok(text) = fs::read_to_string(path) else {
        return;
    };
    match horizon_client::mod_keys::ModKeys::parse_export(&text) {
        Ok(keys) => horizon_client::mod_keys::set_mod_keys(keys),
        Err(e) => log::error!("Horizon: the Java mods' key mappings are unreadable: {e}"),
    }
}

/// After a cached launch, the live boot's key list replaces the cached one when they differ.
fn refresh_keys(live: &Path, cached: &Path) {
    let Ok(text) = fs::read_to_string(live) else {
        return;
    };
    if fs::read_to_string(cached).is_ok_and(|c| c == text) {
        return;
    }
    if fs::write(cached, &text).is_ok() {
        load_keys(cached);
    }
}

/// The live boot's file replaces the cached one when they differ.
fn refresh_file(live: &Path, cached: &Path) {
    if let Ok(bytes) = fs::read(live) {
        if fs::read(cached).is_ok_and(|c| c == bytes) {
            return;
        }
        let _ = fs::write(cached, bytes);
    }
}

/// After the first server start: the entity types' classes, from this cache entry or built by Java once.
pub(super) fn entity_types() {
    let (Some(host), Some(at)) = (horizon_java_host::get(), CACHE_AT.get()) else {
        return;
    };
    if let Err(e) = host.entity_types(&at.join(ENTITY_TYPES)) {
        log::error!("Horizon: the entity types of the Java mods are not loaded: {e}");
    }
}

fn carried_in(at: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = fs::read_dir(at.join("mods"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|m| m.path().join("carry"))
        .filter(|c| c.join(horizon_mod_package::manifest::MANIFEST).exists())
        .collect();
    out.sort();
    out
}

/// The jars FML loads as mods: the run's `mods/`, and NeoForge's own, which `java_mods.py` names with a `mod` line.
fn mod_jars(run: &Path) -> Result<Vec<PathBuf>, String> {
    let dir = run.join("mods");
    let mut jars: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jar"))
        .collect();
    jars.sort();
    let args = fs::read_to_string(run.join("horizon-java.args")).unwrap_or_default();
    jars.extend(
        args.lines()
            .filter_map(|l| l.strip_prefix("mod "))
            .map(PathBuf::from),
    );
    Ok(jars)
}

/// The JVM's classpath, as `java_mods.py` wrote it for the run.
fn classpath(run: &Path) -> Vec<PathBuf> {
    let args = fs::read_to_string(run.join("horizon-java.args")).unwrap_or_default();
    args.lines()
        .filter_map(|l| l.strip_prefix("cp "))
        .flat_map(|cp| cp.split(':'))
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// What the content is built from: the facts, and the baked models when the boot has them.
fn boot_hash(facts: &Path, baked: &Path) -> Result<String, String> {
    let mut textures = Vec::new();
    let root = baked.with_extension("textures");
    files_under(&root, &mut textures);
    textures.sort();
    let mut texture_hash = Sha256::new();
    for path in textures {
        texture_hash.update(
            path.strip_prefix(&root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .as_bytes(),
        );
        texture_hash.update(file_hash(&path)?.as_bytes());
    }
    let texture_hash = format!("{:x}", texture_hash.finalize());
    let baked = if baked.exists() {
        file_hash(baked)?
    } else {
        "none".to_owned()
    };
    Ok(format!("{} {baked} {texture_hash}", file_hash(facts)?))
}

fn copy_textures(source: &Path, target: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    files_under(source, &mut files);
    for file in files {
        let relative = file.strip_prefix(source).map_err(|e| e.to_string())?;
        let dest = target.join(relative);
        fs::create_dir_all(dest.parent().ok_or("texture export has no parent")?)
            .map_err(|e| e.to_string())?;
        fs::copy(&file, &dest).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok(())
}

fn file_hash(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buf = vec![0; 1 << 20];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// What the generated content depends on: every mod jar's bytes, the JVM's arguments (the loader and its libraries), the mods' configs, the game's tables and this build.
fn fingerprint(run: &Path, resources: &Path) -> Result<String, String> {
    let configs = match CONFIGS.get() {
        Some(c) => c.clone()?,
        None => configs(run)?,
    };
    fingerprint_of(&mod_jars(run)?, run, &configs, resources)
}

/// Each file under the run's `config/` with its hash, sorted.
fn configs(run: &Path) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    files_under(&run.join("config"), &mut files);
    files.sort();
    files
        .iter()
        .map(|config| {
            let rel = config.strip_prefix(run).unwrap_or(config).to_string_lossy();
            Ok(format!("config {rel} {}\n", file_hash(config)?))
        })
        .collect()
}

fn fingerprint_of(
    jars: &[PathBuf],
    run: &Path,
    configs: &[String],
    resources: &Path,
) -> Result<String, String> {
    let mut hash = Sha256::new();
    hash.update(format!(
        "horizon java content {GENERATION} {}\n",
        env!("CARGO_PKG_VERSION")
    ));
    for jar in jars {
        let name = jar.file_name().unwrap_or_default().to_string_lossy();
        let size = fs::metadata(jar)
            .map_err(|e| format!("{}: {e}", jar.display()))?
            .len();
        hash.update(format!("jar {name} {size} {}\n", file_hash(jar)?));
    }
    let args = fs::read(run.join("horizon-java.args")).unwrap_or_default();
    hash.update(b"args\n");
    hash.update(&args);
    for analysis in String::from_utf8_lossy(&args)
        .lines()
        .filter_map(|l| l.strip_prefix("analysis "))
    {
        hash.update(b"analysis\n");
        hash.update(fs::read(analysis).unwrap_or_default());
        hash.update(b"routing\n");
        hash.update(
            fs::read(Path::new(analysis).with_file_name("routing.json")).unwrap_or_default(),
        );
    }
    for config in configs {
        hash.update(config);
    }
    for table in [
        "items.json",
        "facts.json",
        "block-behaviours.json",
        "gameplay-facts.json",
    ] {
        let meta = fs::metadata(resources.join(table));
        let len = meta.as_ref().map_or(0, |m| m.len());
        let modified = meta
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        hash.update(format!("table {table} {len} {modified}\n"));
    }
    Ok(format!("{:x}", hash.finalize())[..32].to_owned())
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_under(&path, out);
        } else {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "horizon-java-content-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("run/mods")).unwrap();
        fs::create_dir_all(dir.join("res")).unwrap();
        fs::write(dir.join("res/items.json"), "{}").unwrap();
        dir
    }

    #[test]
    fn captured_texture_pixels_survive_carry_and_invalidate_boot_cache() {
        let dir = scratch("textures");
        let facts = dir.join("facts.json");
        let baked = dir.join("facts.baked.json");
        fs::write(&facts, "{}").unwrap();
        let absent = boot_hash(&facts, &baked).unwrap();
        let source = baked.with_extension("textures");
        let image = source.join("assets/horizon_java_textures/textures/gui/probe.png");
        fs::create_dir_all(image.parent().unwrap()).unwrap();
        fs::write(&image, [1, 2, 3, 4]).unwrap();
        let present = boot_hash(&facts, &baked).unwrap();
        assert_ne!(absent, present);
        let target = dir.join("mods/horizon_java_textures/carry");
        copy_textures(&source, &target).unwrap();
        assert_eq!(
            fs::read(target.join(image.strip_prefix(&source).unwrap())).unwrap(),
            [1, 2, 3, 4]
        );
        fs::write(&image, [4, 3, 2, 1]).unwrap();
        assert_ne!(present, boot_hash(&facts, &baked).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_fingerprint_follows_jars_configs_and_arguments() {
        let dir = scratch("fingerprint");
        let (run, res) = (dir.join("run"), dir.join("res"));
        let jar = run.join("mods/a.jar");
        fs::write(&jar, b"one").unwrap();
        fs::write(run.join("horizon-java.args"), "cp /x.jar\n").unwrap();
        let first = fingerprint(&run, &res).unwrap();
        assert_eq!(
            first,
            fingerprint(&run, &res).unwrap(),
            "unchanged, the same key"
        );
        fs::write(&jar, b"two").unwrap();
        let other_bytes = fingerprint(&run, &res).unwrap();
        assert_ne!(first, other_bytes, "a jar's bytes");
        fs::write(&jar, b"one").unwrap();
        assert_eq!(
            first,
            fingerprint(&run, &res).unwrap(),
            "back to the same bytes"
        );
        fs::write(run.join("mods/b.jar"), b"").unwrap();
        assert_ne!(first, fingerprint(&run, &res).unwrap(), "a jar more");
        fs::remove_file(run.join("mods/b.jar")).unwrap();
        fs::create_dir_all(run.join("config")).unwrap();
        fs::write(run.join("config/m.toml"), "x = 1").unwrap();
        let configured = fingerprint(&run, &res).unwrap();
        assert_ne!(first, configured, "a mod's config");
        fs::write(run.join("config/m.toml"), "x = 2").unwrap();
        assert_ne!(
            configured,
            fingerprint(&run, &res).unwrap(),
            "a config's value"
        );
        fs::remove_dir_all(run.join("config")).unwrap();
        fs::write(run.join("horizon-java.args"), "cp /y.jar\n").unwrap();
        assert_ne!(
            first,
            fingerprint(&run, &res).unwrap(),
            "the loader's classpath"
        );
        let analysis = dir.join("entity-facts.json");
        fs::write(&analysis, "{}").unwrap();
        fs::write(
            run.join("horizon-java.args"),
            format!("cp /y.jar\nanalysis {}\n", analysis.display()),
        )
        .unwrap();
        let analysed = fingerprint(&run, &res).unwrap();
        fs::write(&analysis, r#"{"events":{}}"#).unwrap();
        assert_ne!(
            analysed,
            fingerprint(&run, &res).unwrap(),
            "the entity analysis's bytes"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_live_boots_entity_facts_replace_the_cached_ones() {
        let dir = scratch("entities");
        let (live, cached) = (dir.join("live.bin"), dir.join("cached.bin"));
        fs::write(&cached, b"old").unwrap();
        fs::write(&live, b"new").unwrap();
        refresh_file(&live, &cached);
        assert_eq!(fs::read(&cached).unwrap(), b"new");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_live_boots_keys_replace_the_cached_ones() {
        let dir = scratch("keys");
        let (live, cached) = (dir.join("live.tsv"), dir.join("cached.tsv"));
        let line =
            |name: &str| format!("{name}\t0\ttest_mod:keys\tkey.keyboard.k\tNONE\tIN_GAME\n");
        fs::write(&cached, line("key.test_mod.old")).unwrap();
        fs::write(&live, line("key.test_mod.new")).unwrap();
        refresh_keys(&live, &cached);
        assert_eq!(
            fs::read_to_string(&cached).unwrap(),
            line("key.test_mod.new")
        );
        let keys = horizon_client::mod_keys::mod_keys();
        assert_eq!(keys.keys.len(), 1);
        assert_eq!(keys.keys[0].id, "key.test_mod.new");
        horizon_client::mod_keys::set_mod_keys(Default::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_cache_holds_only_finished_content() {
        let dir = scratch("carried");
        let at = dir.join("cache");
        fs::create_dir_all(at.join("mods/m/carry")).unwrap();
        fs::create_dir_all(at.join("mods/broken/carry")).unwrap();
        fs::write(
            at.join("mods/m/carry")
                .join(horizon_mod_package::manifest::MANIFEST),
            "",
        )
        .unwrap();
        assert_eq!(carried_in(&at), [at.join("mods/m/carry")]);
        let _ = fs::remove_dir_all(&dir);
    }
}
