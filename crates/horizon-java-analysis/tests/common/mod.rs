//! The folders of classes javac compiles from `tests/java/` and `tests/java-facts/`, shared by the test binaries.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn java_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            java_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "java") {
            out.push(path);
        }
    }
}

fn compile(tree: &str) -> Option<PathBuf> {
    compile_trees(tree, &[tree])
}

/// Compiles the trees into one folder; a later tree's file replaces an earlier one's at the same path.
fn compile_trees(label: &str, trees: &[&str]) -> Option<PathBuf> {
    let Some(home) = std::env::var_os("JAVA_HOME") else {
        eprintln!("skipped: no JAVA_HOME");
        return None;
    };
    let javac = Path::new(&home).join("bin").join("javac");
    if !javac.is_file() {
        eprintln!("skipped: no JAVA_HOME");
        return None;
    }
    let out = std::env::temp_dir().join(format!(
        "horizon-java-analysis-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&out);
    let mut by_path = std::collections::BTreeMap::new();
    for tree in trees {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(tree);
        let mut found = Vec::new();
        java_files(&root, &mut found);
        for path in found {
            by_path.insert(path.strip_prefix(&root).unwrap().to_path_buf(), path);
        }
    }
    let status = Command::new(javac)
        .args(["-proc:none", "-nowarn", "-d"])
        .arg(&out)
        .args(by_path.values())
        .status()
        .unwrap();
    assert!(status.success(), "javac failed");
    Some(out)
}

/// The folder of compiled `tests/java/` classes, or None when there is no javac.
#[allow(dead_code)]
pub fn classes() -> Option<&'static Path> {
    static CLASSES: OnceLock<Option<PathBuf>> = OnceLock::new();
    CLASSES.get_or_init(|| compile("java")).as_deref()
}

/// The folder of compiled `tests/java-facts/` classes, or None when there is no javac.
#[allow(dead_code)]
pub fn facts_classes() -> Option<&'static Path> {
    static CLASSES: OnceLock<Option<PathBuf>> = OnceLock::new();
    CLASSES.get_or_init(|| compile("java-facts")).as_deref()
}

/// The folder of `tests/java-facts/` compiled with `tests/java-routing/` over it, or None when there is no javac.
#[allow(dead_code)]
pub fn routing_classes() -> Option<&'static Path> {
    static CLASSES: OnceLock<Option<PathBuf>> = OnceLock::new();
    CLASSES
        .get_or_init(|| compile_trees("java-routing", &["java-facts", "java-routing"]))
        .as_deref()
}
