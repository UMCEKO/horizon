//! entity-facts.json against a fixture install built from the classes javac compiles from `tests/java-facts/`.

mod common;

use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Write};
use std::path::Path;
use std::process::Command;

use common::facts_classes;
use horizon_java_analysis::entity_facts::facts::{
    HOOKS, LibraryClasses, analyse_hooks, event_classes, hook_closure,
};
use horizon_java_analysis::hierarchy::Hierarchy;
use horizon_java_analysis::knowledge::Knowledge;
use horizon_java_analysis::libraries::{build_library_index, library_jars};
use horizon_java_analysis::mixins::MixinEntry;
use horizon_java_analysis::replaces::analyse_with;
use serde_json::{Value as Json, json};
use zip::ZipArchive;
use zip::write::SimpleFileOptions;

const ENTITY: &str = "net/minecraft/world/entity/Entity";
const LIVING: &str = "net/minecraft/world/entity/LivingEntity";
const ITEM: &str = "net/minecraft/world/entity/item/ItemEntity";
const EVENTS: &str = "net/neoforged/neoforge/event/entity/";
const HOOK: &str = "net/neoforged/neoforge/common/TestHooks#";

fn files_under(root: &Path, prefixes: &[&str]) -> Vec<(String, Vec<u8>)> {
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

fn jar_bytes(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn write_jar(path: &Path, files: &[(String, Vec<u8>)]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, jar_bytes(files)).unwrap();
}

fn text(name: &str, body: &str) -> (String, Vec<u8>) {
    (name.to_owned(), body.as_bytes().to_vec())
}

fn boot_jar() -> Vec<u8> {
    jar_bytes(&[
        text("analysis/native.tsv", ""),
        text("analysis/verdicts.tsv", ""),
        text("analysis/host_words.txt", ""),
    ])
}

fn mixin(handler: &str, method: &str, at: &str, config: &str, side: &str) -> MixinEntry {
    MixinEntry {
        mod_id: "fix".to_owned(),
        config: config.to_owned(),
        side: side.to_owned(),
        mixin: "fixture.mixin.ItemMixin".to_owned(),
        handler: handler.to_owned(),
        kind: "inject".to_owned(),
        target_class: vec![ITEM.replace('/', ".")],
        target_method: vec![method.to_owned()],
        at: vec![at.to_owned()],
        access: false,
    }
}

fn entries() -> Vec<MixinEntry> {
    let fix = "fix.mixins.json";
    vec![
        mixin("head", "tick", "HEAD", fix, "common"),
        mixin("ret", "tick", "RETURN", fix, "common"),
        mixin("captured", "tick", "HEAD", fix, "common"),
        mixin("middle", "hurt", "INVOKE", "cond.mixins.json", "common"),
        mixin("pick", "tryPick", "HEAD", fix, "client"),
    ]
}

fn install(out: &Path, root: &Path) {
    let libs = root.join("libraries/net/neoforged");
    write_jar(
        &libs.join("minecraft-client-patched/1/minecraft-client-patched-1.jar"),
        &files_under(out, &["net/minecraft/"]),
    );
    write_jar(
        &libs.join("minecraft-server-patched/1/minecraft-server-patched-1.jar"),
        &[text("placeholder.txt", "")],
    );
    write_jar(
        &libs.join("neoforge/1/neoforge-1-universal.jar"),
        &files_under(out, &["net/neoforged/neoforge/"]),
    );
    write_jar(
        &libs.join("fancymodloader/loader/1/loader-1.jar"),
        &files_under(out, &["net/neoforged/fml/", "net/neoforged/neoforgespi/"]),
    );
    write_jar(
        &libs.join("bus/1/bus-1.jar"),
        &files_under(out, &["net/neoforged/bus/"]),
    );
    let mut fix = files_under(out, &["fixture/"]);
    fix.extend([
        text(
            "META-INF/neoforge.mods.toml",
            "[[mods]]\nmodId = \"fix\"\n[[mixins]]\nconfig = \"fix.mixins.json\"\n[[mixins]]\nconfig = \"cond.mixins.json\"\n",
        ),
        text("fix.mixins.json", "{\"package\": \"fixture.mixin\", \"mixins\": []}"),
        text(
            "cond.mixins.json",
            "{\"package\": \"fixture.mixin\", \"plugin\": \"fixture.Plugin\", \"mixins\": []}",
        ),
    ]);
    write_jar(&root.join("mods/fix.jar"), &fix);
    write_jar(
        &root.join("mods/plain.jar"),
        &[
            text(
                "META-INF/neoforge.mods.toml",
                "[[mods]]\nmodId = \"plain\"\n",
            ),
            text(
                "META-INF/services/net.neoforged.neoforgespi.transformation.ClassProcessorProvider",
                "x.Provider\n",
            ),
        ],
    );
    write_jar(
        &root.join("mods/core.jar"),
        &[
            text(
                "META-INF/neoforge.mods.toml",
                "[[mods]]\nmodId = \"core\"\n",
            ),
            text("META-INF/coremods.json", "{}"),
        ],
    );
}

fn facts_with(hooks: &[(&str, String)]) -> Option<Json> {
    let out = facts_classes()?;
    let dir = tempfile::tempdir().unwrap();
    install(out, dir.path());
    let mut zip = ZipArchive::new(Cursor::new(boot_jar())).unwrap();
    let knowledge = Knowledge::from_archive(&mut zip, "boot.jar").unwrap();
    let mut r = analyse_with(dir.path(), &knowledge, None, Some(entries()), &mut |_| {}).unwrap();
    let table: Vec<(String, String)> = hooks
        .iter()
        .map(|(h, k)| ((*h).to_owned(), k.clone()))
        .collect();
    Some(analyse_hooks(&mut r, &table).unwrap())
}

fn facts() -> Option<Json> {
    let table: Vec<(&str, String)> = HOOKS.iter().map(|(h, k)| (h.as_str(), k.clone())).collect();
    facts_with(&table)
}

fn hook(name: &str, extra: &str) -> String {
    format!("{HOOK}{name}(Lnet/minecraft/world/entity/Entity;{extra})V")
}

fn writes_of(facts: &Json, receiver: &str) -> Vec<(String, String, Json)> {
    facts["writes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["receiver"] == receiver)
        .map(|w| {
            (
                w["key"].as_str().unwrap().to_owned(),
                w["group"].as_str().unwrap().to_owned(),
                w["mods"].clone(),
            )
        })
        .collect()
}

#[test]
fn calls_give_groups_per_receiver_and_writers_with_their_mods() {
    let Some(f) = facts() else { return };
    assert!(
        f["reads"][ENTITY]
            .as_array()
            .unwrap()
            .contains(&json!("position"))
    );
    assert_eq!(
        writes_of(&f, ENTITY),
        [(
            format!("{ENTITY}#setPos(DDD)V"),
            "position".to_owned(),
            json!(["fix"])
        )]
    );
}

#[test]
fn set_health_on_a_living_receiver_writes_and_reads_health() {
    let Some(f) = facts() else { return };
    assert_eq!(f["reads"][LIVING], json!(["health"]));
    assert_eq!(
        writes_of(&f, LIVING),
        [(
            format!("{LIVING}#setHealth(F)V"),
            "health".to_owned(),
            json!(["fix"])
        )]
    );
}

#[test]
fn get_data_is_java_state_and_is_not_carried() {
    let Some(f) = facts() else { return };
    assert!(f["reads"].get(ITEM).is_none());
    assert!(writes_of(&f, ITEM).is_empty());
}

#[test]
fn a_mods_own_method_and_a_static_are_not_entity_state_and_an_unmapped_one_is_unclassified() {
    let Some(f) = facts() else { return };
    let keys: Vec<&str> = f["writes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["key"].as_str().unwrap())
        .collect();
    assert!(
        !keys
            .iter()
            .any(|k| k.contains("own") || k.contains("spawn"))
    );
    assert_eq!(
        f["unclassified"],
        json!([format!("{ENTITY}#frobnicate (fix)")])
    );
}

#[test]
fn reflection_in_a_class_that_names_entities_is_listed_per_mod() {
    let Some(f) = facts() else { return };
    assert_eq!(f["reflective_entity_access"], json!({"fix": 1}));
}

#[test]
fn an_event_filter_is_all_when_any_listener_is_and_subclasses_hear_their_supers() {
    let Some(f) = facts() else { return };
    let events = &f["events"];
    assert_eq!(events[format!("{EVENTS}ProbeEvent")]["subjects"], "all");
    let join = events[format!("{EVENTS}EntityJoinLevelEvent")]["subjects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["side"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(join, ["server", "both"]);
    let child = &events[format!("{EVENTS}ChildEvent")];
    let mut methods: Vec<&str> = child["listeners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["method"].as_str().unwrap())
        .collect();
    methods.sort_unstable();
    assert_eq!(methods, ["after", "stops"]);
    assert_eq!(child["subjects"], "all");
    assert_eq!(
        f["event_supers"][format!("{EVENTS}ChildEvent")],
        json!([
            format!("{EVENTS}ProbeEvent"),
            format!("{EVENTS}EntityEvent")
        ])
    );
}

fn item_routes(f: &Json) -> &Json {
    &f["routes"][ITEM]
}

fn handlers<'a>(rows: &'a Json, fields: &[&str]) -> Vec<Vec<&'a Json>> {
    rows.as_array()
        .unwrap()
        .iter()
        .map(|h| fields.iter().map(|k| &h[*k]).collect())
        .collect()
}

#[test]
fn head_and_return_injectors_without_locals_are_edges_with_their_cancellable_flag() {
    let Some(f) = facts() else { return };
    let tick = &item_routes(&f)["edge"][format!("{ITEM}#tick()V")];
    assert_eq!(
        handlers(tick, &["handler", "at", "cancellable", "conditional"]),
        [
            [&json!("head"), &json!("HEAD"), &json!(true), &json!(false)],
            [
                &json!("ret"),
                &json!("RETURN"),
                &json!(false),
                &json!(false)
            ]
        ]
    );
    let pick = &item_routes(&f)["edge"][format!("{ITEM}#tryPick(I)Z")];
    assert_eq!(
        handlers(pick, &["handler", "cancellable"]),
        [[&json!("pick"), &json!(true)]]
    );
}

#[test]
fn a_captured_local_or_another_position_makes_a_body() {
    let Some(f) = facts() else { return };
    assert_eq!(
        item_routes(&f)["body"],
        json!([format!("{ITEM}#hurt(F)Z"), format!("{ITEM}#tick()V")])
    );
}

#[test]
fn body_handlers_of_a_config_plugin_mixin_are_listed_for_boot_to_check() {
    let Some(f) = facts() else { return };
    assert_eq!(
        item_routes(&f)["body_conditional"],
        json!({format!("{ITEM}#hurt(F)Z"): [{"mixin": "fixture.mixin.ItemMixin", "handler": "middle", "conditional": true, "side": "common"}]})
    );
}

#[test]
fn a_handler_carries_the_side_of_its_mixin() {
    let Some(f) = facts() else { return };
    let pick = &item_routes(&f)["edge"][format!("{ITEM}#tryPick(I)Z")];
    assert_eq!(
        handlers(pick, &["handler", "side"]),
        [[&json!("pick"), &json!("client")]]
    );
    let tick = &item_routes(&f)["edge"][format!("{ITEM}#tick()V")];
    let sides: HashSet<&str> = tick
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h["side"].as_str().unwrap())
        .collect();
    assert_eq!(sides, HashSet::from(["common"]));
}

#[test]
fn own_type_overrides_are_keyed_by_the_mod_class() {
    let Some(f) = facts() else { return };
    assert_eq!(
        f["overrides"],
        json!({"fixture/Zom": [format!("{ENTITY}#tick()V")]})
    );
}

#[test]
fn a_registration_with_a_vanilla_static_goes_to_the_type_and_the_rest_is_left_for_boot() {
    let Some(f) = facts() else { return };
    assert_eq!(f["caps"], json!({"minecraft:cow": ["fixture/Caps.CAP"]}));
    assert_eq!(f["caps_unresolved"], json!({"fix": 1}));
}

#[test]
fn a_transformation_service_and_a_coremod_are_opaque_with_their_mods() {
    let Some(f) = facts() else { return };
    let opaque: Vec<&str> = f["opaque"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o.as_str().unwrap())
        .collect();
    assert_eq!(opaque.len(), 2);
    assert_eq!(opaque[0], "core: coremod file META-INF/coremods.json");
    assert!(opaque[1].starts_with("plain: service "));
}

#[test]
fn the_hooks_the_neoforge_jar_lacks_are_missing() {
    let Some(f) = facts() else { return };
    let lines: Vec<&str> = HOOKS.iter().map(|(_, k)| k.as_str()).collect();
    assert_eq!(f["hooks_missing"], json!(lines));
    assert_eq!(f["hooks"], json!({}));
    assert_eq!(f["format"], 2);
}

fn closure(name: &str, extra: &str) -> Option<(Json, BTreeMap<String, String>)> {
    let out = facts_classes()?;
    let dir = tempfile::tempdir().unwrap();
    install(out, dir.path());
    let jars = library_jars(&dir.path().join("libraries")).unwrap();
    let mut hier = Hierarchy::new(build_library_index(&jars).unwrap());
    hier.jars = jars;
    let lib = LibraryClasses::new(&hier.jars).unwrap();
    let events: HashSet<String> = event_classes(&hier).into_iter().collect();
    let mut unclassified = BTreeMap::new();
    let got = hook_closure(&hier, &lib, &hook(name, extra), &events, &mut unclassified)
        .unwrap()
        .unwrap();
    Some((got, unclassified))
}

fn write_keys(got: &Json) -> Vec<&str> {
    got["writes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["key"].as_str().unwrap())
        .collect()
}

#[test]
fn a_hook_calling_set_air_supply_lists_the_writer() {
    let Some((got, _)) = closure("direct", "") else {
        return;
    };
    assert_eq!(
        got["writes"],
        json!([{"key": format!("{ENTITY}#setAirSupply(I)V"), "receiver": ENTITY, "group": "air"}])
    );
    assert_eq!(got["reads"], json!(["position", "air"]));
}

#[test]
fn a_hook_reaching_a_helper_or_a_lambda_follows_it() {
    let Some((outer, _)) = closure("outer", "") else {
        return;
    };
    assert_eq!(write_keys(&outer), [format!("{ENTITY}#setPos(DDD)V")]);
    let Some((lambda, _)) = closure("lambda", "") else {
        return;
    };
    assert_eq!(write_keys(&lambda), [format!("{ENTITY}#setAirSupply(I)V")]);
}

#[test]
fn the_walk_stops_at_post_and_lists_the_events_a_hook_constructs() {
    let Some((got, _)) = closure("posts", "Lnet/neoforged/bus/api/IEventBus;") else {
        return;
    };
    assert_eq!(got["writes"], json!([]));
    assert_eq!(got["events"], json!([format!("{EVENTS}ProbeEvent")]));
}

#[test]
fn an_unmapped_entity_call_in_a_hook_is_unclassified_as_neoforge() {
    let Some((_, unclassified)) = closure("bad", "") else {
        return;
    };
    assert_eq!(
        unclassified,
        BTreeMap::from([(format!("{ENTITY}#wobble"), "neoforge".to_owned())])
    );
    let Some(f) = facts_with(&[("bad", hook("bad", ""))]) else {
        return;
    };
    assert!(
        f["unclassified"]
            .as_array()
            .unwrap()
            .contains(&json!(format!("{ENTITY}#wobble (neoforge)")))
    );
}

#[test]
fn hook_writes_join_the_top_level_writes_as_neoforge() {
    let Some(f) = facts_with(&[("direct", hook("direct", ""))]) else {
        return;
    };
    assert_eq!(f["hooks"]["direct"]["reads"], json!(["position", "air"]));
    assert_eq!(f["hooks_missing"], json!([]));
    let mods: Vec<&Json> = f["writes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|w| w["key"].as_str().unwrap().ends_with("#setAirSupply(I)V"))
        .map(|w| &w["mods"])
        .collect();
    assert_eq!(mods, [&json!(["neoforge"])]);
}

#[test]
fn the_cli_writes_the_files_then_exits_1_on_unclassified_methods() {
    let Some(out) = facts_classes() else { return };
    let dir = tempfile::tempdir().unwrap();
    install(out, dir.path());
    let boot = dir.path().join("boot.jar");
    std::fs::write(&boot, boot_jar()).unwrap();
    let written = dir.path().join("out");
    let run = Command::new(env!("CARGO_BIN_EXE_horizon-java-analysis"))
        .arg("replaces")
        .arg("--install")
        .arg(dir.path())
        .arg("--boot-jar")
        .arg(&boot)
        .arg("--out")
        .arg(&written)
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(stderr.contains(
        "entity methods without a state group (add them to crates/horizon-java-analysis/data/entity_groups.tsv):\n  net/minecraft/world/entity/Entity#frobnicate (fix)"
    ));
    let facts: Json =
        serde_json::from_str(&std::fs::read_to_string(written.join("entity-facts.json")).unwrap())
            .unwrap();
    assert_eq!(
        facts["unclassified"],
        json!([format!("{ENTITY}#frobnicate (fix)")])
    );
    let replaces: Json =
        serde_json::from_str(&std::fs::read_to_string(written.join("replaces.json")).unwrap())
            .unwrap();
    assert!(replaces.get("entity_facts").is_none());
}
