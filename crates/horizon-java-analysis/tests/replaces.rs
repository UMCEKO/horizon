//! The records and replaces.json against a fixture install built from the classes javac compiles from `tests/java/`.

mod common;

use std::io::{Cursor, Write};
use std::path::Path;

use common::classes;
use horizon_java_analysis::knowledge::Knowledge;
use horizon_java_analysis::mixins::MixinEntry;
use horizon_java_analysis::replaces::{
    Change, Replaces, analyse_with, class_listeners, override_records, replaces_json,
};
use horizon_java_analysis::{class, hierarchy::Hierarchy, libraries};
use serde_json::{Value as Json, json};
use zip::ZipArchive;
use zip::write::SimpleFileOptions;

const NATIVE: &str = "net/minecraft/world/Base#tick\tported\t1\tcrates/a.rs:7\t\n\
                      net/minecraft/world/Base#must\tabsent\t0\t\tnothing\n\
                      net/minecraft/world/Base#other\tnone\t0\t\t\n";
const EVENTS: &str = "net/neoforged/neoforge/event/";

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
                let name = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.push((name, std::fs::read(&path).unwrap()));
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

fn knowledge() -> Knowledge {
    let files = [
        text("analysis/native.tsv", NATIVE),
        text("analysis/verdicts.tsv", ""),
        text("analysis/host_words.txt", "PlainEvent\n"),
    ];
    let mut zip = ZipArchive::new(Cursor::new(jar_bytes(&files))).unwrap();
    Knowledge::from_archive(&mut zip, "boot.jar").unwrap()
}

fn entry(
    handler: &str,
    kind: &str,
    class: &str,
    method: &[&str],
    at: &[&str],
    side: &str,
    access: bool,
) -> MixinEntry {
    let strs = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect();
    MixinEntry {
        mod_id: "fix".to_owned(),
        config: "fix.mixins.json".to_owned(),
        side: side.to_owned(),
        mixin: if handler == "getSecret" {
            "fixture.mixin.B"
        } else {
            "fixture.mixin.A"
        }
        .to_owned(),
        handler: handler.to_owned(),
        kind: kind.to_owned(),
        target_class: strs(&[class]),
        target_method: strs(method),
        at: strs(at),
        access,
    }
}

fn fixture() -> Option<(tempfile::TempDir, Replaces)> {
    let out = classes()?;
    let dir = tempfile::tempdir().unwrap();
    let install = dir.path().join("install");
    let libs = install.join("libraries/net/neoforged");
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

    let mut inner = vec![text(
        "META-INF/neoforge.mods.toml",
        "[[mods]]\nmodId = \"inner\"",
    )];
    inner.extend(files_under(out, &["inner/"]));
    let mut fix = files_under(out, &["fixture/"]);
    fix.extend([
        text("META-INF/neoforge.mods.toml", "[[mods]]\nmodId = \"fix\"\n[[mixins]]\nconfig = \"fix.mixins.json\"\n"),
        text("fix.mixins.json", "{\"package\": \"fixture.mixin\", \"plugin\": \"fixture.Plugin\", \"mixins\": []}"),
        text("data/minecraft/recipe/a.json", "{}"),
        text("data/minecraft/recipe/b.json", "{}"),
        text("data/minecraft/tags/block/c.json", "{}"),
        text("data/minecraft/worldgen/biome/d.json", "{}"),
        text("data/minecraft/loot_table/blocks/e.json", "{}"),
        text("data/other/recipe/no.json", "{}"),
        text("assets/minecraft/lang/en_us.json", "{}"),
        text(
            "META-INF/accesstransformer.cfg",
            "# comment\npublic net.minecraft.world.Base tick()V\n\npublic net.minecraft.world.Base must()V # tail\n",
        ),
        ("META-INF/jarjar/inner-1.jar".to_owned(), jar_bytes(&inner)),
    ]);
    write_jar(&install.join("mods/fix.jar"), &fix);
    write_jar(
        &install.join("mods/plain.jar"),
        &[
            text("META-INF/neoforge.mods.toml", "[[mods]]\nmodId = \"plain\""),
            text(
                "META-INF/services/net.neoforged.neoforgespi.transformation.ClassProcessorProvider",
                "x.Provider\n",
            ),
        ],
    );
    for host in ["horizon-java-boot.jar", "horizon-java-plugin.jar"] {
        write_jar(
            &install.join("mods").join(host),
            &files_under(out, &["fixture/"]),
        );
    }

    let entries = vec![
        entry(
            "onTick",
            "inject",
            "net.minecraft.world.Thing",
            &["tick"],
            &["HEAD"],
            "common",
            false,
        ),
        entry(
            "onMust",
            "redirect",
            "net.minecraft.world.Base",
            &["Lnet/minecraft/world/Base;must()V"],
            &["INVOKE"],
            "client",
            false,
        ),
        entry(
            "getSecret",
            "accessor",
            "net.minecraft.world.Base",
            &[],
            &[],
            "common",
            true,
        ),
    ];
    let result = analyse_with(&install, &knowledge(), None, Some(entries), &mut |_| {}).unwrap();
    Some((dir, result))
}

fn changes<'a>(r: &'a Replaces, key: &str, mechanism: Option<&str>) -> Vec<&'a Change> {
    r.methods[key]
        .changes
        .iter()
        .filter(|c| mechanism.is_none_or(|m| c.mechanism == m))
        .collect()
}

fn event<'a>(r: &'a Replaces, name: &str) -> &'a horizon_java_analysis::replaces::EventRow {
    &r.events[&format!("{EVENTS}{name}")]
}

#[test]
fn an_override_needs_the_same_name_and_descriptor() {
    let Some((_dir, r)) = fixture() else { return };
    let tick = changes(&r, "net/minecraft/world/Base#tick", Some("override"));
    let got: std::collections::BTreeSet<_> = tick
        .iter()
        .map(|c| (c.mod_id.as_str(), c.desc.as_deref().unwrap()))
        .collect();
    assert_eq!(got, [("fix", "()V"), ("inner", "()V")].into());
    assert!(
        !tick
            .iter()
            .any(|c| matches!(c.desc.as_deref(), Some("(J)V" | "(I)V")))
    );
    assert_eq!(tick[0].scope.as_deref(), Some("own-type"));
    assert_eq!(tick[0].routing, "java-own-type");
    assert_eq!(tick[0].classes, ["fixture.One"]);
}

#[test]
fn a_new_method_and_a_static_are_not_overrides() {
    let Some((_dir, r)) = fixture() else { return };
    assert!(!r.methods.contains_key("fixture/One#own"));
    assert!(
        !r.methods
            .keys()
            .any(|k| k.ends_with("#own") || k.ends_with("#st"))
    );
}

#[test]
fn extension_defaults_count() {
    let Some((_dir, r)) = fixture() else { return };
    let key = "net/neoforged/neoforge/common/extensions/IThingExtension#friction";
    let change = changes(&r, key, None)[0];
    assert_eq!(
        (change.mechanism.as_str(), change.extension),
        ("override", true)
    );
}

#[test]
fn implementing_an_abstract_method_is_not_a_replacement() {
    let Some((_dir, r)) = fixture() else { return };
    let must = changes(&r, "net/minecraft/world/Base#must", Some("implement"));
    let mods: std::collections::BTreeSet<_> = must.iter().map(|c| c.mod_id.as_str()).collect();
    assert_eq!(mods, ["fix", "inner"].into());
    assert!(must.iter().all(|c| c.routing == "java-already"));
}

#[test]
fn jar_in_jar_mods_are_their_own_and_the_host_jars_are_skipped() {
    let Some((_dir, r)) = fixture() else { return };
    let want = [("implement".to_owned(), 1), ("override".to_owned(), 1)].into();
    assert_eq!(r.mods["inner"].counts, want);
    assert_eq!(r.mods.keys().collect::<Vec<_>>(), ["fix", "inner", "plain"]);
    assert_eq!(r.mods["fix"].jars, ["fix"]);
    assert_eq!(r.mods["inner"].jars, ["fix>inner-1"]);
}

#[test]
fn a_mixin_resolves_to_the_declaring_class() {
    let Some((_dir, r)) = fixture() else { return };
    let change = changes(&r, "net/minecraft/world/Base#tick", Some("mixin"))[0];
    assert_eq!(change.kind.as_deref(), Some("inject"));
    assert_eq!(change.at, ["HEAD"]);
    assert_eq!(change.side.as_deref(), Some("common"));
    assert_eq!(change.routing, "java");
    assert_eq!(
        change.conditional.as_deref(),
        Some("config plugin fixture.Plugin")
    );
}

#[test]
fn accessors_are_inert() {
    let Some((_dir, r)) = fixture() else { return };
    let change = changes(&r, "net/minecraft/world/Base#getSecret", None)[0];
    assert_eq!(
        (change.mechanism.as_str(), change.routing.as_str()),
        ("access", "inert")
    );
}

#[test]
fn subscribe_event_keeps_priority_and_receive_canceled() {
    let Some((_dir, r)) = fixture() else { return };
    let plain = event(&r, "PlainEvent");
    let change = plain.changes.iter().find(|c| c.via == "subscribe").unwrap();
    assert_eq!(
        (
            change.priority.as_str(),
            change.receive_canceled,
            change.class.as_str(),
            change.method.as_str()
        ),
        ("HIGH", true, "fixture.Listeners", "onPlain")
    );
    assert!(plain.host_mentions);
}

#[test]
fn a_method_reference_gives_the_event() {
    let Some((_dir, r)) = fixture() else { return };
    let cancel = event(&r, "CancelEvent");
    let change = &cancel.changes[0];
    assert_eq!(
        (
            change.via.as_str(),
            change.method.as_str(),
            change.priority.as_str()
        ),
        ("addListener", "<init>", "NORMAL")
    );
    assert_eq!(cancel.cancellable, Some(true));
    assert_eq!(event(&r, "PlainEvent").cancellable, Some(false));
}

#[test]
fn a_lambda_gives_the_event_and_the_priority() {
    let Some((_dir, r)) = fixture() else { return };
    let thing = event(&r, "ModThing");
    let change = &thing.changes[0];
    assert_eq!(
        (change.priority.as_str(), change.bus.as_deref()),
        ("LOW", Some("mod"))
    );
    assert_eq!(thing.bus, ["mod"]);
}

#[test]
fn an_explicit_class_argument_wins() {
    let Some((_dir, r)) = fixture() else { return };
    let vias: Vec<_> = event(&r, "PlainEvent")
        .changes
        .iter()
        .map(|c| (c.via.as_str(), c.bus.as_deref()))
        .collect();
    assert!(vias.contains(&("addListener", Some("game"))), "{vias:?}");
}

#[test]
fn a_subscriber_class_names_its_bus() {
    let Some((_dir, r)) = fixture() else { return };
    let plain = event(&r, "PlainEvent");
    let change = plain
        .changes
        .iter()
        .find(|c| c.via == "subscriber-class")
        .unwrap();
    assert_eq!(
        (change.class.as_str(), change.bus.as_deref()),
        ("fixture.Sub", Some("mod"))
    );
    assert_eq!(plain.bus, ["game", "mod"]);
}

#[test]
fn events_route_to_the_native_hook() {
    let Some((_dir, r)) = fixture() else { return };
    assert!(r.events.values().all(|e| e.routing == "native-hook"));
}

#[test]
fn data_overrides_by_kind_in_the_minecraft_namespace_only() {
    let Some((_dir, r)) = fixture() else { return };
    assert_eq!(r.data["data:recipe"]["fix"].files, 2);
    assert_eq!(
        r.data["data:recipe"]["fix"].samples,
        [
            "data/minecraft/recipe/a.json",
            "data/minecraft/recipe/b.json"
        ]
    );
    let kinds: Vec<_> = r.data.keys().map(String::as_str).collect();
    assert_eq!(
        kinds,
        [
            "assets:lang",
            "data:loot_table",
            "data:recipe",
            "data:tags/block",
            "data:worldgen/biome"
        ]
    );
}

#[test]
fn access_transformer_lines_are_counted() {
    let Some((_dir, r)) = fixture() else { return };
    assert_eq!(r.mods["fix"].counts["access_transformer"], 2);
}

#[test]
fn a_transformation_service_is_opaque_and_a_config_plugin_is_not() {
    let Some((_dir, r)) = fixture() else { return };
    let plain = &r.mods["plain"];
    assert!(!plain.can_analyse_ahead);
    assert!(plain.opaque[0].contains("ClassProcessorProvider"));
    let fix = &r.mods["fix"];
    assert!(fix.can_analyse_ahead);
    assert!(fix.opaque.is_empty());
    assert!(r.mods["inner"].can_analyse_ahead);
}

#[test]
fn native_owned_changes_run_in_java_and_the_rest_already_do() {
    let Some((_dir, r)) = fixture() else { return };
    let tick = &r.methods["net/minecraft/world/Base#tick"];
    assert_eq!(
        (tick.routing.as_str(), tick.native.status.as_str()),
        ("java", "ported")
    );
    assert_eq!(tick.native.fns, ["crates/a.rs:7"]);
    let must = &r.methods["net/minecraft/world/Base#must"];
    assert_eq!(
        (must.routing.as_str(), must.native.status.as_str()),
        ("undecided", "absent")
    );
    let mixins: Vec<_> = must
        .changes
        .iter()
        .filter(|c| c.mechanism == "mixin")
        .map(|c| c.routing.as_str())
        .collect();
    assert_eq!(mixins, ["undecided"]);
}

#[test]
fn totals_and_summary() {
    let Some((_dir, r)) = fixture() else { return };
    let routing: Vec<_> = r.mods["fix"]
        .routing
        .iter()
        .map(|(k, v)| (k.as_str(), *v))
        .collect();
    assert_eq!(
        routing,
        [
            ("java", 1),
            ("java-own-type", 1),
            ("undecided", 1),
            ("java-already", 2),
            ("native-hook", 3),
            ("inert", 1)
        ]
    );
    assert_eq!(r.totals["access"], 1);
    assert_eq!(r.totals["data"], 6);
    assert_eq!(r.totals["listener"], 5);
}

#[test]
fn the_json_writes_each_mechanism_with_its_own_fields_in_python_s_shape() {
    let Some((_dir, r)) = fixture() else { return };
    let text = replaces_json(&r, Path::new("/i"), 1.26).unwrap();
    let doc: Json = serde_json::from_str(&text).unwrap();
    assert_eq!(doc["format"], 1);
    assert_eq!(doc["seconds"], 1.3);
    let methods = &doc["methods"];
    let tick = &methods["net/minecraft/world/Base#tick"]["changes"];
    assert_eq!(
        tick[0],
        json!({"mod": "fix", "mechanism": "mixin", "kind": "inject", "handler": "fixture.mixin.A.onTick", "desc": null, "at": ["HEAD"],
               "side": "common", "conditional": "config plugin fixture.Plugin", "routing": "java"})
    );
    assert_eq!(
        tick[1],
        json!({"mod": "fix", "mechanism": "override", "desc": "()V", "classes": ["fixture.One"], "classes_n": 1, "abstract": false,
               "extension": false, "scope": "own-type", "routing": "java-own-type"})
    );
    assert_eq!(
        methods["net/minecraft/world/Base#tick"]["native"],
        json!({"status": "ported", "owned": true, "fns": ["crates/a.rs:7"], "note": ""})
    );
    let sub = doc["events"][format!("{EVENTS}PlainEvent")]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["via"] == "subscriber-class")
        .unwrap()
        .clone();
    assert_eq!(
        sub,
        json!({"mod": "fix", "mechanism": "listener", "via": "subscriber-class", "class": "fixture.Sub", "method": "on", "priority": "NORMAL",
               "receive_canceled": false, "bus": "mod"})
    );
    assert!(text.starts_with("{\n \"format\": 1,\n \"install\": \"/i\",\n \"mods\": {\n  \"fix\""));
}

#[test]
fn the_class_helpers_read_the_fixture_classes_directly() {
    let Some(out) = classes() else { return };
    let jars = [out.join("lib.jar")];
    write_jar(&jars[0], &files_under(out, &["net/"]));
    let hier = Hierarchy::new(libraries::build_library_index(&jars).unwrap());
    let parse = |name: &str| {
        class::parse_class(&std::fs::read(out.join(name)).unwrap(), class::Parse::all()).unwrap()
    };
    let mut hier = hier;
    let one = parse("fixture/One.class");
    hier.add(&one.name, one.sup.clone(), one.ifaces.clone());
    let overrides = override_records(&one, "fix", &hier);
    let keys: Vec<_> = overrides
        .iter()
        .map(|((k, d), v)| (k.as_str(), d.as_str(), *v))
        .collect();
    assert_eq!(
        keys,
        [
            ("net/minecraft/world/Base#tick", "()V", (false, false)),
            ("net/minecraft/world/Base#must", "()V", (false, true)),
            (
                "net/neoforged/neoforge/common/extensions/IThingExtension#friction",
                "()I",
                (true, false)
            ),
        ]
    );
    let found = class_listeners(&parse("fixture/Listeners.class"), &hier);
    let got: Vec<_> = found
        .iter()
        .map(|f| {
            (
                f.event.as_deref().unwrap(),
                f.via.as_str(),
                f.method.as_str(),
                f.priority.as_str(),
                f.receive_canceled,
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            (
                "net/neoforged/neoforge/event/PlainEvent",
                "subscribe",
                "onPlain",
                "HIGH",
                true
            ),
            (
                "net/neoforged/neoforge/event/CancelEvent",
                "addListener",
                "<init>",
                "NORMAL",
                false
            ),
            (
                "net/neoforged/neoforge/event/ModThing",
                "addListener",
                "<init>",
                "LOW",
                false
            ),
            (
                "net/neoforged/neoforge/event/PlainEvent",
                "addListener",
                "<init>",
                "NORMAL",
                false
            ),
        ]
    );
    let sub = class_listeners(&parse("fixture/Sub.class"), &hier);
    assert_eq!(
        (sub[0].via.as_str(), sub[0].explicit_bus.as_deref()),
        ("subscriber-class", Some("mod"))
    );
}
