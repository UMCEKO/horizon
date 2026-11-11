//! The listener guards and entity type ids against classes javac compiles from `tests/java/`.

mod common;

use std::io::{Cursor, Write};
use std::path::Path;

use common::classes;
use horizon_java_analysis::entity_facts;
use horizon_java_analysis::entity_facts::guards::{Guard, subjects, type_ids};
use serde_json::{Value as Json, json};
use zip::write::SimpleFileOptions;

const EVENTS: &str = "net/neoforged/neoforge/event/entity/";
const LIVING: &str = "net/minecraft/world/entity/LivingEntity";
const PLAYER: &str = "net/minecraft/world/entity/player/Player";

fn class_files(root: &Path, prefixes: &[&str]) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let mut paths: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                walk(root, &path, out);
            } else if path.extension().is_some_and(|e| e == "class") {
                let name = path.strip_prefix(root).unwrap().to_string_lossy();
                out.push((name.into_owned(), std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut all = Vec::new();
    walk(root, root, &mut all);
    all.retain(|(n, _)| prefixes.iter().any(|p| n.starts_with(p)));
    all
}

fn write_jar(path: &Path, files: &[(String, Vec<u8>)]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(data).unwrap();
    }
    std::fs::write(path, zip.finish().unwrap().into_inner()).unwrap();
}

fn text(name: &str, body: &str) -> (String, Vec<u8>) {
    (name.to_owned(), body.as_bytes().to_vec())
}

fn install(out: &Path, root: &Path) {
    let libs = root.join("libraries/net/neoforged");
    write_jar(
        &libs.join("minecraft-client-patched/1/minecraft-client-patched-1.jar"),
        &class_files(out, &["net/minecraft/"]),
    );
    write_jar(
        &libs.join("minecraft-server-patched/1/minecraft-server-patched-1.jar"),
        &[text("placeholder.txt", "")],
    );
    write_jar(
        &libs.join("neoforge/1/neoforge-1-universal.jar"),
        &class_files(out, &["net/neoforged/neoforge/"]),
    );
    write_jar(
        &libs.join("fancymodloader/loader/1/loader-1.jar"),
        &class_files(out, &["net/neoforged/fml/", "net/neoforged/neoforgespi/"]),
    );
    write_jar(
        &libs.join("bus/1/bus-1.jar"),
        &class_files(out, &["net/neoforged/bus/"]),
    );
    let mut files = class_files(out, &["guardfix/"]);
    files.push(text(
        "META-INF/neoforge.mods.toml",
        "[[mods]]\nmodId = \"gf\"\n",
    ));
    write_jar(&root.join("mods/gf.jar"), &files);
}

fn guard(side: &str, require: &[&str], exclude: &[&str], types: Option<&[&str]>) -> Json {
    json!({
        "side": side,
        "require": require,
        "exclude": exclude,
        "types": types,
    })
}

fn listener(doc: &Json, event: &str, method: &str) -> Json {
    let key = format!("gf|guardfix.Guards|{method}|{EVENTS}{event}");
    doc["listeners"][&key].clone()
}

fn dumped() -> Option<Json> {
    let out = classes()?;
    let dir = tempfile::tempdir().unwrap();
    install(out, dir.path());
    Some(entity_facts::dump_guards(dir.path()).unwrap())
}

#[test]
fn a_prefix_of_checks_gives_side_require_and_exclude_and_stops_at_the_unknown_call() {
    let Some(doc) = dumped() else { return };
    assert_eq!(
        listener(&doc, "EntityJoinLevelEvent", "mek"),
        guard("server", &[LIVING], &[PLAYER], None)
    );
}

#[test]
fn the_client_side_test_gives_the_side() {
    let Some(doc) = dumped() else { return };
    let side = |m| listener(&doc, "EntityLeaveLevelEvent", m);
    assert_eq!(side("serverOnly"), guard("server", &[], &[], None));
    assert_eq!(side("clientOnly"), guard("client", &[], &[], None));
}

#[test]
fn comparing_the_type_to_a_vanilla_static_gives_its_registry_id_in_either_order() {
    let Some(doc) = dumped() else { return };
    let types = |m| listener(&doc, "EntityLeaveLevelEvent", m)["types"].clone();
    assert_eq!(types("zombie"), json!(["minecraft:zombie"]));
    assert_eq!(types("reversed"), json!(["minecraft:cow"]));
    assert_eq!(types("modded"), json!(["mod:thing"]));
}

#[test]
fn a_guard_after_an_unknown_call_or_a_branch_into_code_is_not_taken() {
    let Some(doc) = dumped() else { return };
    let all = serde_json::to_value(Guard::all()).unwrap();
    assert_eq!(listener(&doc, "ProbeEvent", "after"), all);
    assert_eq!(listener(&doc, "ProbeEvent", "stops"), all);
}

#[test]
fn a_lambda_listener_takes_the_guards_of_its_implementation() {
    let Some(doc) = dumped() else { return };
    assert_eq!(
        listener(&doc, "EntityJoinLevelEvent", "<init>"),
        guard("both", &[PLAYER], &[], None)
    );
}

#[test]
fn type_ids_come_from_the_vanilla_static_initialisers() {
    let Some(out) = classes() else { return };
    let dir = tempfile::tempdir().unwrap();
    let jar = dir.path().join("client.jar");
    write_jar(&jar, &class_files(out, &["net/minecraft/"]));
    let ids = type_ids(&[jar]).unwrap();
    let got: Vec<_> = ids.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(
        got,
        [
            ("COW", "minecraft:cow"),
            ("MODDED", "mod:thing"),
            ("ZOMBIE", "minecraft:zombie")
        ]
    );
    assert!(type_ids(&[]).unwrap().is_empty());
}

#[test]
fn an_event_filter_is_all_when_any_listener_is_and_else_the_distinct_guards() {
    let server = Guard {
        side: "server".to_owned(),
        ..Guard::all()
    };
    let both = Guard::all();
    assert_eq!(subjects(&[server.clone(), both.clone()]), None);
    assert_eq!(
        subjects(&[server.clone(), server.clone()]),
        Some(vec![server])
    );
}
