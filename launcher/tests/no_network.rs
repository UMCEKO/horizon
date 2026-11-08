//! The launcher sends nothing about the player or the machine (docs/telemetry.md). Every request
//! it makes to a non-local address goes through the proxy named by `HTTPS_PROXY`, so a proxy that
//! counts connections sees any call: the commands that write local reports make none.
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;

fn home() -> PathBuf {
    let root = std::env::temp_dir().join(format!("horizon-no-network-{}", std::process::id()));
    for folder in ["game", "launcher", "data", "config"] {
        std::fs::create_dir_all(root.join(folder)).unwrap();
    }
    root
}

#[test]
fn writing_a_bug_report_and_reading_settings_make_no_network_call() {
    let root = home();
    std::fs::create_dir_all(root.join("game/logs")).unwrap();
    std::fs::write(root.join("game/logs/latest.log"), "[INFO] started\n").unwrap();
    let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
    proxy.set_nonblocking(true).unwrap();
    let address = format!("http://{}", proxy.local_addr().unwrap());
    let run = |args: &[&str]| {
        let out = Command::new(env!("CARGO_BIN_EXE_horizon-launcher"))
            .args(args)
            .env("HORIZON_HOME", root.join("game"))
            .env("HORIZON_LAUNCHER_HOME", root.join("launcher"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("HORIZON_LAUNCHER_NO_SELF_UPDATE", "1")
            .env("HORIZON_NO_KEYCHAIN", "1")
            .env("HTTPS_PROXY", &address)
            .env("HTTP_PROXY", &address)
            .env("ALL_PROXY", &address)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    run(&["bug-report", "it crashed", "--log"]);
    run(&["settings"]);

    let reports: Vec<_> = std::fs::read_dir(root.join("game/bug-reports"))
        .unwrap()
        .collect();
    assert_eq!(reports.len(), 1, "one local bug report file");
    assert!(
        proxy.accept().is_err(),
        "the launcher connected to the network"
    );
    let _ = std::fs::remove_dir_all(&root);
}
