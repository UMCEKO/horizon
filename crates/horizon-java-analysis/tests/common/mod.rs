//! The folder of classes javac compiles from `tests/java/`, shared by the test binaries.

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

/// The folder of compiled fixture classes, or None when there is no javac.
pub fn classes() -> Option<&'static Path> {
    static CLASSES: OnceLock<Option<PathBuf>> = OnceLock::new();
    CLASSES
        .get_or_init(|| {
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
                "horizon-java-analysis-classes-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&out);
            let mut sources = Vec::new();
            java_files(
                &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/java"),
                &mut sources,
            );
            let status = Command::new(javac)
                .args(["-proc:none", "-nowarn", "-d"])
                .arg(&out)
                .args(&sources)
                .status()
                .unwrap();
            assert!(status.success(), "javac failed");
            Some(out)
        })
        .as_deref()
}
