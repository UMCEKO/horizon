use super::args::*;
use super::*;
use serde_json::json;

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hz-java-mods-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn profiles() -> Profiles {
    Profiles {
        client_jvm: vec![
            "-DlibraryDirectory=${library_directory}".into(),
            "--add-opens".into(),
            "java.base/java.lang=ALL-UNNAMED".into(),
            "-Xss1m".into(),
        ],
        client_game: vec!["--fml.neoForgeVersion".into(), "9.9".into()],
        client_jars: vec!["a/a.jar".into(), "b/b.jar".into()],
        server_args: [
            "--add-opens java.base/x=ALL-UNNAMED",
            "-DlibraryDirectory=libraries",
            "-classpath",
            "libraries/a/a.jar:libraries/b/b.jar",
            "--add-modules ALL-MODULE-PATH",
            "--fml.mcVersion 26.3",
            "dev.Main",
            "",
        ]
        .join("\n"),
    }
}

#[test]
fn client_args_name_the_libraries_assets_and_neoforge() {
    let libs = Path::new("/i/libraries");
    let lines = args_lines(
        Os::Linux,
        false,
        libs,
        "9.9",
        &profiles(),
        Some((Path::new("/i/assets"), "34")),
    );
    assert_eq!(
        lines,
        [
            "dist client",
            "cp /i/libraries/a/a.jar:/i/libraries/b/b.jar",
            "jvm -DlibraryDirectory=/i/libraries",
            "jvm --add-opens=java.base/java.lang=ALL-UNNAMED",
            "jvm -Xss1m",
            "prog --fml.neoForgeVersion",
            "prog 9.9",
            "mod /i/libraries/net/neoforged/neoforge/9.9/neoforge-9.9-universal.jar",
            "prog --assetsDir",
            "prog /i/assets",
            "prog --assetIndex",
            "prog 34",
        ]
    );
}

#[test]
fn server_args_come_from_unix_args_and_have_no_dist_line() {
    let lines = args_lines(
        Os::Linux,
        true,
        Path::new("/i/libraries"),
        "9.9",
        &profiles(),
        None,
    );
    assert_eq!(lines[0], "cp /i/libraries/a/a.jar:/i/libraries/b/b.jar");
    assert!(lines.contains(&"jvm --add-opens=java.base/x=ALL-UNNAMED".to_string()));
    assert!(lines.contains(&"jvm -DlibraryDirectory=/i/libraries".to_string()));
    assert!(lines.contains(&"jvm --add-modules=ALL-MODULE-PATH".to_string()));
    assert!(lines.contains(&"prog --fml.mcVersion".to_string()));
    assert!(!lines
        .iter()
        .any(|l| l == "dist client" || l.contains("assetsDir")));
}

#[test]
fn windows_joins_the_classpath_with_semicolons() {
    let lines = args_lines(
        Os::Windows,
        false,
        Path::new("C:/i/libraries"),
        "9.9",
        &profiles(),
        None,
    );
    assert_eq!(lines[1], "cp C:/i/libraries/a/a.jar;C:/i/libraries/b/b.jar");
    let server = args_lines(
        Os::Windows,
        true,
        Path::new("C:/i/libraries"),
        "9.9",
        &profiles(),
        None,
    );
    assert_eq!(
        server[0],
        "cp C:/i/libraries/a/a.jar;C:/i/libraries/b/b.jar"
    );
}

#[test]
fn each_system_has_its_own_library_path() {
    let java = Path::new("/j");
    let none = |_: &str| None;
    let run = Path::new("/r");
    let host = Path::new("/h.jar");
    let linux = launch_env(Os::Linux, java, "9.9", host, run, &|n| {
        (n == "LD_LIBRARY_PATH").then(|| "/x".into())
    });
    let value = &linux["LD_LIBRARY_PATH"];
    assert!(value.starts_with("/j/lib/server:/j/lib"));
    assert!(value.ends_with(":/x"));
    assert_eq!(linux["JAVA_HOME"], "/j");
    assert_eq!(linux["HORIZON_JAVA_RUN"], "/r");
    assert_eq!(linux["HORIZON_JAVA_HOST_JAR"], "/h.jar");
    assert_eq!(linux["HORIZON_NEOFORGE"], "9.9");
    let mac = launch_env(Os::MacOs, java, "9.9", host, run, &none);
    assert_eq!(mac["DYLD_LIBRARY_PATH"], "/j/lib/server:/j/lib/jli:/j/lib");
    let windows = launch_env(Os::Windows, java, "9.9", host, run, &|n| {
        (n == "PATH").then(|| "C:/Windows".into())
    });
    assert_eq!(windows["PATH"], "/j/bin/server;/j/bin;C:/Windows");
    assert!(!windows.contains_key("LD_LIBRARY_PATH"));
}

#[test]
fn the_temurin_build_follows_the_system_and_architecture() {
    assert_eq!(jdk::platform(Os::Linux, "x86_64"), Ok(("linux", "x64")));
    assert_eq!(jdk::platform(Os::MacOs, "aarch64"), Ok(("mac", "aarch64")));
    assert_eq!(jdk::platform(Os::Windows, "x86_64"), Ok(("windows", "x64")));
    assert!(jdk::platform(Os::Linux, "riscv64").is_err());
    let url = jdk::query("linux", "x64");
    assert!(
        url.contains("/latest/25/hotspot")
            && url.contains("architecture=x64")
            && url.contains("image_type=jdk")
    );
}

#[test]
fn the_jdk_asset_carries_its_checksum_and_the_archive_kind_of_the_system() {
    let listing = |name: &str| json!([{ "release_name": "jdk-25+1", "binary": { "package": { "name": name, "link": "https://x/j", "checksum": "AB" } } }]);
    let asset = jdk::asset(&listing("j.tar.gz"), Os::Linux).unwrap();
    assert_eq!(
        (asset.sha256.as_str(), asset.release.as_str()),
        ("ab", "jdk-25+1")
    );
    assert!(jdk::asset(&listing("j.zip"), Os::Linux).is_err());
    assert!(jdk::asset(&listing("j.tar.gz"), Os::Windows).is_err());
    assert!(jdk::asset(&listing("j.zip"), Os::Windows).is_ok());
    assert!(jdk::asset(&json!([]), Os::Linux).is_err());
}

fn fake_jdk(home: &Path, version: &str) {
    fs::create_dir_all(home.join("bin")).unwrap();
    fs::write(jdk::java_program(home), "").unwrap();
    fs::write(
        home.join("release"),
        format!("JAVA_VERSION=\"{version}\"\n"),
    )
    .unwrap();
}

#[test]
fn a_chosen_jdk_must_be_25_and_a_macos_archive_is_found_in_contents_home() {
    let root = dir("jdk");
    fake_jdk(&root.join("old"), "21.0.1");
    assert!(jdk::given(&root.join("old"))
        .unwrap_err()
        .contains("not a JDK 25"));
    fake_jdk(&root.join("new"), "25.0.5");
    assert_eq!(
        jdk::given(&root.join("new")).unwrap(),
        fs::canonicalize(root.join("new")).unwrap()
    );
    fake_jdk(&root.join("unpacked/jdk-25+8/Contents/Home"), "25.0.5");
    assert_eq!(
        jdk::find(&root.join("unpacked")).unwrap(),
        root.join("unpacked/jdk-25+8/Contents/Home")
    );
    assert!(jdk::find(&root.join("old")).is_none());
}

#[test]
fn the_asset_index_is_links_over_the_prepared_assets() {
    let root = dir("assets");
    let resources = root.join("res");
    let hash = "ab".repeat(20);
    let other = "cd".repeat(20);
    fs::create_dir_all(resources.join("assets/minecraft/lang")).unwrap();
    fs::write(resources.join("assets/minecraft/lang/en.json"), "{}").unwrap();
    fs::write(
        resources.join("asset-index.json"),
        json!({ "objects": {
            "minecraft/lang/en.json": { "hash": hash, "size": 2 },
            "minecraft/missing.png": { "hash": other, "size": 1 },
        } })
        .to_string(),
    )
    .unwrap();
    let folder = assets_index(&resources, "34", &root).unwrap();
    assert_eq!(assets_index(&resources, "34", &root).unwrap(), folder);
    let index: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(folder.join("indexes/34.json")).unwrap()).unwrap();
    assert!(index["objects"]["minecraft/lang/en.json"].is_object());
    assert!(index["objects"]["minecraft/missing.png"].is_null());
    assert_eq!(
        fs::read_to_string(folder.join("objects/ab").join(&hash)).unwrap(),
        "{}"
    );
    assert!(!folder.join("objects/cd").exists());
}

#[test]
fn the_asset_index_id_is_read_from_prepared_data_then_from_the_minecraft_files_beside_it() {
    let root = dir("index-id");
    let resources = root.join(".local/horizon-client/26.3");
    fs::create_dir_all(&resources).unwrap();
    assert!(asset_index_id_in(&resources, &[]).is_err());
    let manifest = root.join(".local/minecraft/26.3");
    fs::create_dir_all(&manifest).unwrap();
    fs::write(
        manifest.join("manifest.json"),
        r#"{"assetIndex":{"id":"34"}}"#,
    )
    .unwrap();
    assert_eq!(asset_index_id_in(&resources, &[]).unwrap(), "34");
    fs::write(resources.join("asset-index-id.txt"), "35\n").unwrap();
    assert_eq!(asset_index_id_in(&resources, &[]).unwrap(), "35");
    fs::write(resources.join("asset-index-id.txt"), "../x").unwrap();
    assert_eq!(asset_index_id_in(&resources, &[]).unwrap(), "34");
    let launcher = root.join("launcher/assets/indexes");
    fs::create_dir_all(&launcher).unwrap();
    fs::write(resources.join("asset-index.json"), "{\"objects\":{}}").unwrap();
    fs::write(launcher.join("12.json"), "other").unwrap();
    fs::write(launcher.join("40.json"), "{\"objects\":{}}").unwrap();
    fs::remove_dir_all(&manifest).unwrap();
    assert_eq!(
        asset_index_id_in(&resources, &[root.join("launcher")]).unwrap(),
        "40"
    );
}

#[test]
fn host_jars_come_from_the_version_and_only_a_local_build_falls_back() {
    let root = dir("host");
    let version = root.join("v1");
    let local = root.join("local");
    fs::create_dir_all(&local).unwrap();
    for jar in HOST_JARS {
        fs::write(local.join(jar), "").unwrap();
    }
    assert!(host_jars_in(&version, "v1").is_err());
    fs::create_dir_all(&version).unwrap();
    fs::write(version.join(LOCAL_BUILD), local.to_string_lossy().as_ref()).unwrap();
    assert_eq!(host_jars_in(&version, "v1").unwrap(), local);
    for jar in HOST_JARS {
        fs::create_dir_all(version.join("java-mods")).unwrap();
        fs::write(version.join("java-mods").join(jar), "").unwrap();
    }
    assert_eq!(
        host_jars_in(&version, "v1").unwrap(),
        version.join("java-mods")
    );
    let plain = root.join("v2");
    fs::create_dir_all(&plain).unwrap();
    assert!(host_jars_in(&plain, "v2").is_err());
}

#[test]
fn the_feature_is_found_in_a_program_that_holds_the_host_variable() {
    let root = dir("feature");
    let with = root.join("with");
    let without = root.join("without");
    let mut bytes = vec![7u8; 20_000_000];
    fs::write(&without, &bytes).unwrap();
    let at = 8 * 1024 * 1024 - 5;
    bytes[at..at + FEATURE_MARK.len()].copy_from_slice(FEATURE_MARK);
    fs::write(&with, &bytes).unwrap();
    assert!(has_feature(&with));
    assert!(!has_feature(&without));
    assert!(!has_feature(&root.join("missing")));
}

#[test]
fn the_state_follows_the_pack_and_the_install() {
    let root = dir("state");
    let install = root.join("install");
    let pack = root.join("pack");
    fs::create_dir_all(&pack).unwrap();
    let mut config = Config::default();
    assert_eq!(Status::of(&config, &install).state, State::NoPack);
    config.pack = Some(pack.clone());
    assert_eq!(Status::of(&config, &install).state, State::EmptyPack);
    fs::write(pack.join("a.jar"), "12345").unwrap();
    fs::write(pack.join("notes.txt"), "x").unwrap();
    assert_eq!(Status::of(&config, &install).state, State::NotInstalled);
    fs::create_dir_all(&install).unwrap();
    let record = Record {
        format: FORMAT,
        neoforge: NEOFORGE.into(),
        java: "jdk/j".into(),
        mods: pack_files(&pack),
    };
    fs::write(install.join("install.json"), json_text(&record).unwrap()).unwrap();
    let ready = Status::of(&config, &install);
    assert_eq!((ready.state, ready.mods), (State::Ready, 1));
    assert_eq!(ready.java, Some(install.join("jdk/j")));
    fs::write(pack.join("b.jar"), "1").unwrap();
    assert_eq!(Status::of(&config, &install).state, State::Stale);
}

#[test]
fn links_follow_the_wanted_jars_only() {
    let root = dir("links");
    let jars: Vec<PathBuf> = ["a.jar", "b.jar"]
        .iter()
        .map(|n| {
            let p = root.join(n);
            fs::write(&p, n).unwrap();
            p
        })
        .collect();
    let folder = root.join("run/mods");
    sync_links(&folder, &jars).unwrap();
    sync_links(&folder, &jars[1..]).unwrap();
    let names: Vec<_> = fs::read_dir(&folder)
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(names, ["b.jar"]);
    assert_eq!(fs::read_to_string(folder.join("b.jar")).unwrap(), "b.jar");
}

#[test]
fn a_run_folder_is_made_for_a_moved_install_without_new_downloads() {
    let root = dir("run");
    let install = root.join("install");
    fs::create_dir_all(install.join("mods")).unwrap();
    fs::create_dir_all(install.join("libraries")).unwrap();
    fs::create_dir_all(install.join("host")).unwrap();
    fs::write(install.join("mods/m.jar"), "m").unwrap();
    for jar in HOST_JARS {
        fs::write(install.join("host").join(jar), jar).unwrap();
    }
    fs::write(
        install.join("profiles.json"),
        json_text(&profiles()).unwrap(),
    )
    .unwrap();
    let record = Record {
        format: FORMAT,
        neoforge: "9.9".into(),
        java: "jdk".into(),
        mods: vec![],
    };
    fs::write(install.join("install.json"), json_text(&record).unwrap()).unwrap();
    let run = root.join("server-folder");
    let tag = "v-none";
    // The tag is not installed: the host jars are missing, said in words.
    assert!(prepare_run(tag, &install, &run, true)
        .unwrap_err()
        .contains("carries no Java mods support"));
}

#[test]
fn a_bad_download_checksum_is_refused_before_it_takes_the_place_of_the_file() {
    let root = dir("download");
    let dest = root.join("x.bin");
    fs::write(&dest, "kept").unwrap();
    // Unreachable address: nothing is fetched, the good file stays.
    let err = download(
        "http://127.0.0.1:9/x",
        &dest,
        Check::Sha256(&"0".repeat(64)),
        &|_| {},
    )
    .unwrap_err();
    assert!(!err.is_empty());
    assert_eq!(fs::read_to_string(&dest).unwrap(), "kept");
    let sum = {
        let mut h = Hasher::Sha256(sha2::Digest::new());
        h.update(b"kept");
        h.hex()
    };
    download("http://127.0.0.1:9/x", &dest, Check::Sha256(&sum), &|_| {}).unwrap();
}
