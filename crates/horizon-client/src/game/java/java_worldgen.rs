// game-module-cfg: feature java-mods
//! A world's start with Java mods: `ServerAboutToStart` before native builds its levels, then their worldgen export, cached by the mods' content and the world's packs.
use super::Game;
use super::world_loading::{JavaWorldStart, JavaWorldgen, WorldOpening};
use horizon_java_host::wire::{In, Out};
use horizon_world::worldgen::overrides::WorldgenOverrides;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

/// `server_about_to_start`'s answers: the world started, or a level's generation context threw.
const STARTED: u8 = 1;
const CONTEXT_FAILED: u8 = 2;

/// The cached export the open world started from, exported again once its server started.
static RECHECK: Mutex<Option<PathBuf>> = Mutex::new(None);

impl Game<'_> {
    /// `server_about_to_start` for the world, then the export: the cached one, or Java's, waited for; an error when Java's generation context failed.
    pub(super) fn java_world_opening(&mut self, opening: &WorldOpening) -> JavaWorldStart {
        if horizon_java_host::get().is_none() {
            return Ok(None);
        }
        for dim in super::Dim::all() {
            self.java_register_level(dim);
        }
        let mut out = Out::default();
        out.text(&opening.root.to_string_lossy());
        out.long(opening.seed);
        // `ServerLevel.getSeed` is bridged, and `ChunkMap.<init>` seeds the `RandomState` with it before the world's level data is in place.
        self.level.seed = opening.seed;
        match self
            .java_request("server_about_to_start", &out.0)
            .as_deref()
        {
            Some([STARTED]) => {}
            Some([CONTEXT_FAILED]) => {
                return Err(
                    "the Java mods' generation context failed to build; see the log".into(),
                );
            }
            _ => {
                log::error!(
                    "Horizon: the Java server didn't take the world's start; no worldgen export"
                );
                return Ok(None);
            }
        }
        Ok(self.java_worldgen_after_start(opening))
    }

    /// The export once `server_about_to_start` ran: the cached one, or Java's, waited for.
    fn java_worldgen_after_start(&mut self, opening: &WorldOpening) -> JavaWorldgen {
        let (file, reuse) = match super::java_content::cache_at() {
            Some(dir) => (dir.join(export_name(&opening.packs)), true),
            None => {
                log::warn!(
                    "Horizon: the Java mods' content has no cache entry; their worldgen export isn't cached"
                );
                let name = format!("worldgen-{}.json", std::process::id());
                (std::env::temp_dir().join(name), false)
            }
        };
        let began = Instant::now();
        let read = read_or_export(&file, reuse, |path| self.java_worldgen_export(path));
        if !reuse {
            let _ = fs::remove_file(&file);
        }
        let parsed = read.and_then(|(text, cached)| {
            WorldgenOverrides::from_java_export(&text).map(|parsed| (parsed, cached))
        });
        let ((overrides, owners), cached) = match parsed {
            Ok(parsed) => parsed,
            Err(e) => {
                log::error!("Horizon: Java mods' worldgen export: {e}");
                return None;
            }
        };
        log::info!(
            "Horizon: worldgen export: {} entries, {} with mod types, {} seams with patched methods, {} ms{}",
            overrides.entry_count(),
            owners.with_mod_types(),
            owners.routed().len(),
            began.elapsed().as_millis(),
            if cached { " (cached)" } else { "" }
        );
        for warning in &owners.warnings {
            log::warn!("Horizon: {warning}");
        }
        for (bundle, id) in &owners.failed {
            log::warn!("Horizon: worldgen export: {bundle} {id} couldn't be encoded");
        }
        if cached {
            *RECHECK.lock().unwrap_or_else(PoisonError::into_inner) = Some(file);
        }
        Some(Arc::new(overrides))
    }
}

/// `wg_probe`: Java's generator of `level` at block points `[x, y, z]`, for `horizon-ctl worldgen probe`.
pub(super) fn java_worldgen_probe(level: &str, points: &[[i32; 3]]) -> Result<Value, String> {
    let host = horizon_java_host::get().ok_or("worldgen probe: no Java mods run")?;
    let mut out = Out::default();
    out.text(level);
    out.int(points.len() as i32);
    for [x, y, z] in points {
        out.int(*x);
        out.int(*y);
        out.int(*z);
    }
    let reply = horizon_java_host::bridge::without_active(|| host.request("wg_probe", &out.0))
        .ok_or("worldgen probe: Java threw")?;
    read_probe(&reply, points).ok_or_else(|| "worldgen probe: a short reply".to_owned())?
}

fn read_probe(reply: &[u8], points: &[[i32; 3]]) -> Option<Result<Value, String>> {
    let mut reply = In(reply);
    if reply.byte()? != 0 {
        return Some(Err(format!("worldgen probe: {}", reply.text()?)));
    }
    let (generator, biome_source, final_density) = (reply.text()?, reply.text()?, reply.text()?);
    let mut sampled = Vec::with_capacity(points.len());
    for [x, y, z] in points {
        let (height, biome) = (reply.int()?, reply.text()?);
        sampled.push(json!({"x": x, "y": y, "z": z, "height": height, "biome": biome}));
    }
    Some(Ok(json!({
        "generator": generator,
        "biome_source": biome_source,
        "final_density": final_density,
        "points": sampled,
    })))
}

/// The cache file of a world whose enabled packs are `packs` (sorted).
fn export_name(packs: &[String]) -> String {
    let mut hash = Sha256::new();
    for pack in packs {
        hash.update(pack.as_bytes());
        hash.update(b"\n");
    }
    let key: String = hash.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("worldgen-{key}.json")
}

/// The export at `file`: the cached one when `reuse` finds it, or the one `export` writes there, waited for; whether it was cached.
fn read_or_export(
    file: &Path,
    reuse: bool,
    export: impl FnOnce(&Path) -> bool,
) -> Result<(String, bool), String> {
    if reuse && let Ok(text) = fs::read_to_string(file) {
        return Ok((text, true));
    }
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    if !export(file) {
        return Err("Java didn't write it".into());
    }
    let text = fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok((text, false))
}

/// After `server_start` of a world opened from a cached export: Java exports again beside it, off the server thread.
pub(super) fn recheck_export() {
    let Some(file) = RECHECK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
    else {
        return;
    };
    let Some(host) = horizon_java_host::get() else {
        return;
    };
    let _ = std::thread::Builder::new()
        .name("Java mods' facts".into())
        .spawn(move || {
            let next = next_of(&file);
            let Some(path) = next.to_str() else {
                return;
            };
            let wrote = horizon_java_host::bridge::without_active(|| {
                host.request("worldgen_export", path.as_bytes())
            });
            if wrote.as_deref() == Some(&[1]) {
                settle(&file, &next);
            } else {
                log::error!(
                    "Horizon: Java mods' worldgen export not checked: Java didn't write it"
                );
            }
        });
}

fn next_of(file: &Path) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".next");
    PathBuf::from(name)
}

/// The live export against the cached one: a changed one takes its place for the next open, never mid-world.
fn settle(file: &Path, next: &Path) -> bool {
    let read = |path: &Path| {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    };
    let (cached, live) = (read(file), read(next));
    if live.is_none() || cached == live {
        let _ = fs::remove_file(next);
        return false;
    }
    let warning =
        "Horizon: Java mods' worldgen export changed; the next world open uses the new one";
    log::warn!("{warning}");
    eprintln!("{warning}");
    if let Err(e) = fs::rename(next, file) {
        log::error!("Horizon: {}: {e}", file.display());
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "horizon-java-worldgen-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_cache_file_changes_with_the_pack_list() {
        let vanilla = export_name(&["vanilla".into()]);
        let more = export_name(&["file/terralith".into(), "vanilla".into()]);
        assert_ne!(vanilla, more);
        assert_eq!(vanilla, export_name(&["vanilla".into()]));
        assert_ne!(
            export_name(&["ab".into(), "c".into()]),
            export_name(&["a".into(), "bc".into()])
        );
    }

    #[test]
    fn a_hit_reads_the_file_and_a_miss_calls_the_export() {
        let dir = scratch("hit");
        let file = dir.join("worldgen-k.json");
        let mut calls = 0;
        let (text, cached) = read_or_export(&file, true, |path| {
            calls += 1;
            fs::write(path, r#"{"format":1}"#).is_ok()
        })
        .unwrap();
        assert_eq!(
            (text.as_str(), cached, calls),
            (r#"{"format":1}"#, false, 1)
        );
        let (text, cached) = read_or_export(&file, true, |_| panic!("a hit exports")).unwrap();
        assert_eq!((text.as_str(), cached), (r#"{"format":1}"#, true));
        assert!(read_or_export(&dir.join("other.json"), true, |_| false).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_changed_export_replaces_the_cached_one_and_an_equal_one_goes() {
        let dir = scratch("settle");
        fs::create_dir_all(&dir).unwrap();
        let (file, next) = (dir.join("w.json"), dir.join("w.json.next"));
        fs::write(&file, r#"{"a":1,"b":2}"#).unwrap();
        fs::write(&next, r#"{"b":2,"a":1}"#).unwrap();
        assert!(!settle(&file, &next));
        assert!(!next.exists());
        fs::write(&next, r#"{"a":3}"#).unwrap();
        assert!(settle(&file, &next));
        assert_eq!(fs::read_to_string(&file).unwrap(), r#"{"a":3}"#);
        assert!(!next.exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
