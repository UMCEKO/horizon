//! The game's own analysis of the mods, made before the JVM starts and cached by what it was made from.

use horizon_java_analysis::knowledge::Knowledge;
use horizon_java_analysis::lambdas::Vanilla;
use horizon_java_analysis::routing::{self, Bridges};
use horizon_java_analysis::{entity_facts, libraries, replaces};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Bumped when the analysis writes something else from the same jars.
pub const GENERATION: u32 = 1;

/// The folder holding this run's analysis (`entity-facts.json`, `routing.json`, `routing.tsv`), made when not cached.
pub fn prepare(run: &Path, bridges: &Bridges) -> Result<PathBuf, String> {
    let root = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join(".horizon-cache/java-analysis");
    let library = libraries::library_jars(&run.join("libraries"))?;
    let key = fingerprint(run, &library, bridges)?;
    let mut made = None;
    let folder = cached_or(&root, &key, |out| {
        let started = std::time::Instant::now();
        let knowledge = Knowledge::from_boot_jar(&run.join("mods/horizon-java-boot.jar"))?;
        let mut result =
            replaces::analyse(run, &knowledge, Some(&root.join("libraries")), &mut |_| {})?;
        let facts = entity_facts::facts::analyse(&mut result)?;
        let vanilla = Vanilla::new(&result.hierarchy.jars)?;
        let report = routing::routing(
            &mut result,
            &vanilla,
            &knowledge,
            bridges,
            &routing::view_classes(),
        )?;
        if let Some(error) = entity_facts::facts::unclassified_error(&facts) {
            return Err(error);
        }
        entity_facts::facts::write_boot_files(out, &facts, &report)?;
        made = Some((
            started.elapsed().as_secs_f64(),
            report.routes.len(),
            report.undecided.len(),
        ));
        Ok(())
    })?;
    match made {
        Some((seconds, routes, undecided)) => println!(
            "Horizon: Java mods analysed in {seconds:.1}s: {routes} routes, {undecided} undecided"
        ),
        None => println!("Horizon: Java mods' analysis cached ({key})"),
    }
    Ok(folder)
}

fn stamp(hash: &mut Sha256, path: &Path) -> Result<(), String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let nanos = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    hash.update(meta.len().to_le_bytes());
    hash.update(nanos.to_le_bytes());
    Ok(())
}

fn fingerprint(run: &Path, libraries: &[PathBuf], bridges: &Bridges) -> Result<String, String> {
    let mut hash = Sha256::new();
    hash.update(GENERATION.to_le_bytes());
    let mods = run.join("mods");
    let mut jars: Vec<PathBuf> = std::fs::read_dir(&mods)
        .map_err(|e| format!("{}: {e}", mods.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jar"))
        .collect();
    jars.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    for jar in &jars {
        hash.update(jar.file_name().unwrap_or_default().as_encoded_bytes());
        hash.update([0]);
        stamp(&mut hash, jar)?;
    }
    for jar in libraries {
        stamp(&mut hash, jar)?;
    }
    let mut sorted: Vec<_> = bridges.iter().collect();
    sorted.sort();
    for (class, method, desc) in sorted {
        for part in [class, method, desc] {
            hash.update(part.as_bytes());
            hash.update([0]);
        }
    }
    let hex: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Ok(hex[..20].to_string())
}

fn cached_or(
    root: &Path,
    key: &str,
    make: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<PathBuf, String> {
    let io = |path: &Path, e: std::io::Error| format!("{}: {e}", path.display());
    let done = root.join(key);
    if done.join("done").exists() {
        return Ok(done);
    }
    let partial = root.join(format!("{key}.partial"));
    let _ = std::fs::remove_dir_all(&partial);
    std::fs::create_dir_all(&partial).map_err(|e| io(&partial, e))?;
    make(&partial)?;
    std::fs::write(partial.join("done"), b"").map_err(|e| io(&partial, e))?;
    let _ = std::fs::remove_dir_all(&done);
    std::fs::rename(&partial, &done).map_err(|e| io(&done, e))?;
    for entry in std::fs::read_dir(root).map_err(|e| io(root, e))?.flatten() {
        let name = entry.file_name();
        if name != "libraries" && name.to_string_lossy() != key {
            let path = entry.path();
            let _ = std::fs::remove_dir_all(&path).or_else(|_| std::fs::remove_file(&path));
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn bridge(name: &str) -> (String, String, String) {
        (name.into(), "m".into(), "()V".into())
    }

    #[test]
    fn a_made_analysis_is_reused_for_the_same_key() {
        let root = tempfile::tempdir().unwrap();
        let runs = Cell::new(0);
        let make = |out: &Path| {
            runs.set(runs.get() + 1);
            std::fs::write(out.join("entity-facts.json"), "{}").map_err(|e| e.to_string())
        };
        let first = cached_or(root.path(), "k1", make).unwrap();
        assert_eq!(first, root.path().join("k1"));
        assert!(first.join("done").exists());
        assert!(first.join("entity-facts.json").exists());
        let again = cached_or(root.path(), "k1", |_| panic!("made twice")).unwrap();
        assert_eq!(again, first);
        assert_eq!(runs.get(), 1);
    }

    #[test]
    fn a_new_key_replaces_the_old_folder_and_keeps_libraries() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("libraries")).unwrap();
        cached_or(root.path(), "old", |_| Ok(())).unwrap();
        cached_or(root.path(), "new", |_| Ok(())).unwrap();
        assert!(!root.path().join("old").exists());
        assert!(root.path().join("new/done").exists());
        assert!(root.path().join("libraries").is_dir());
    }

    #[test]
    fn a_failed_make_leaves_nothing_cached_and_is_tried_again() {
        let root = tempfile::tempdir().unwrap();
        let err = cached_or(root.path(), "k", |_| Err("no".into())).unwrap_err();
        assert_eq!(err, "no");
        assert!(!root.path().join("k").exists());
        let runs = Cell::new(0);
        cached_or(root.path(), "k", |_| {
            runs.set(1);
            Ok(())
        })
        .unwrap();
        assert_eq!(runs.get(), 1);
        assert!(root.path().join("k/done").exists());
    }

    fn run_with(jars: &[(&str, &[u8])]) -> tempfile::TempDir {
        let run = tempfile::tempdir().unwrap();
        std::fs::create_dir(run.path().join("mods")).unwrap();
        for (name, bytes) in jars {
            let path = run.path().join("mods").join(name);
            std::fs::write(&path, bytes).unwrap();
            let file = std::fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1000))
                .unwrap();
        }
        run
    }

    #[test]
    fn the_key_follows_the_jars_and_the_bridges() {
        let bridges: Bridges = [bridge("a"), bridge("b")].into();
        let base = fingerprint(run_with(&[("x.jar", b"1")]).path(), &[], &bridges).unwrap();
        assert_eq!(base.len(), 20);
        let bigger = run_with(&[("x.jar", b"12")]);
        assert_ne!(fingerprint(bigger.path(), &[], &bridges).unwrap(), base);
        let more = run_with(&[("x.jar", b"1"), ("y.jar", b"1")]);
        assert_ne!(fingerprint(more.path(), &[], &bridges).unwrap(), base);
        let extra: Bridges = [bridge("a"), bridge("b"), bridge("c")].into();
        let same = run_with(&[("x.jar", b"1")]);
        assert_ne!(fingerprint(same.path(), &[], &extra).unwrap(), base);
        let reordered: Bridges = [bridge("b"), bridge("a")].into();
        let a = fingerprint(same.path(), &[], &bridges).unwrap();
        assert_eq!(fingerprint(same.path(), &[], &reordered).unwrap(), a);
    }
}
