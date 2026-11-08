//! `horizon-system` reads the machine locally and sends nothing (docs/telemetry.md): none of its
//! dependencies, direct or indirect, can open a connection.
use std::process::Command;

/// Crates that open network connections or speak HTTP.
const NETWORK_CRATES: &[&str] = &[
    "ureq",
    "reqwest",
    "hyper",
    "curl",
    "isahc",
    "surf",
    "attohttpc",
    "native-tls",
    "rustls",
    "openssl",
    "socket2",
    "mio",
    "tokio",
];

#[test]
fn the_probes_depend_on_no_network_crate() {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "tree",
            "--offline",
            "--prefix",
            "none",
            "--all-features",
            "--target",
            "all",
        ])
        .args(["--package", "horizon-system"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tree = String::from_utf8(output.stdout).unwrap();
    let found: Vec<_> = tree
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| NETWORK_CRATES.contains(name))
        .collect();
    assert!(
        found.is_empty(),
        "horizon-system reaches network crates: {found:?}"
    );
}
