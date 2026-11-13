//! routing.json against a fixture install built from `tests/java-facts/` with `tests/java-routing/` over it (port of tests/test_mod_routing.py).

mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Write};
use std::path::Path;
use std::sync::OnceLock;

use common::routing_classes;
use horizon_java_analysis::knowledge::Knowledge;
use horizon_java_analysis::lambdas::Vanilla;
use horizon_java_analysis::mixins::MixinEntry;
use horizon_java_analysis::replaces::{Replaces, analyse_with};
use horizon_java_analysis::routing::{
    Bridges, Closure, GuardValue, Guards, Report, Route, routes_tsv, routing, tsv, view_classes,
};
use zip::ZipArchive;
use zip::write::SimpleFileOptions;

const RT: &str = "net/minecraft/world/rt/";
const KEY: &str = "net/minecraft/world/rt/Target#";
const ENTITY_SET_POS: &str = "net/minecraft/world/entity/Entity#setPos(DDD)V";
const GUARDED: &str = "net/minecraft/world/rt/Guarded";
const ITEM_ENTITY: &str = "net/minecraft/world/entity/item/ItemEntity";
const PANEL: &str = "net/minecraft/client/rt/Panel";
const CHUNK_MAP: &str = "net/minecraft/server/level/ChunkMap";
const GEN: &str = "net/minecraft/world/level/levelgen/RtGen";

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
    let mut fix = files_under(out, &["fixture/", "com/llamalad7/"]);
    fix.extend([
        text(
            "META-INF/neoforge.mods.toml",
            "[[mods]]\nmodId = \"fix\"\n[[mixins]]\nconfig = \"fix.mixins.json\"\n",
        ),
        text(
            "fix.mixins.json",
            "{\"package\": \"fixture.mixin\", \"mixins\": []}",
        ),
    ]);
    write_jar(&root.join("mods/fix.jar"), &fix);
}

fn knowledge(verdicts: &str) -> Knowledge {
    let mut native = String::new();
    let ported = [
        "head",
        "headLocal",
        "tail",
        "wrapped",
        "mixed",
        "varArg",
        "varNo",
        "run",
        "hook",
        "body",
        "bridged",
        "pure2",
    ]
    .iter()
    .map(|n| format!("{KEY}{n}"))
    .chain([
        "net/minecraft/world/entity/Entity#setPos".to_owned(),
        format!("{ITEM_ENTITY}#tick"),
    ])
    .chain(
        ["level", "logged", "twice", "chestAt", "join", "joinLike"]
            .iter()
            .map(|n| format!("{GUARDED}#{n}")),
    )
    .chain(
        [PANEL, CHUNK_MAP, GEN]
            .iter()
            .map(|c| format!("{c}#<init>")),
    );
    for key in ported {
        native.push_str(&format!("{key}\tported\t1\tcrates/a.rs:1\t\n"));
    }
    native.push_str(&format!("{KEY}weak\tmentioned\t0\t\ta comment\n"));
    let jar = jar_bytes(&[
        text("analysis/native.tsv", &native),
        text("analysis/verdicts.tsv", verdicts),
        text("analysis/host_words.txt", ""),
    ]);
    let mut zip = ZipArchive::new(Cursor::new(jar)).unwrap();
    Knowledge::from_archive(&mut zip, "boot.jar").unwrap()
}

fn mixin(handler: &str, method: &str, at: &[&str], kind: &str) -> MixinEntry {
    MixinEntry {
        mod_id: "fix".to_owned(),
        config: "fix.mixins.json".to_owned(),
        side: "common".to_owned(),
        mixin: "fixture.mixin.TMixin".to_owned(),
        handler: handler.to_owned(),
        kind: kind.to_owned(),
        target_class: vec!["net.minecraft.world.rt.Target".to_owned()],
        target_method: vec![method.to_owned()],
        at: at.iter().map(|a| (*a).to_owned()).collect(),
        access: false,
    }
}

fn into(mut e: MixinEntry, mixin: &str, target: &str) -> MixinEntry {
    mixin.clone_into(&mut e.mixin);
    e.target_class = vec![target.replace('/', ".")];
    e
}

fn entries() -> Vec<MixinEntry> {
    let helper = ["INVOKE:Lnet/minecraft/world/rt/Target;helper()V"];
    let mut out = vec![
        mixin("headInject", "head", &["HEAD"], "inject"),
        mixin("headLocal", "headLocal", &["HEAD"], "inject"),
        mixin("tailInject", "tail", &["TAIL"], "inject"),
        mixin("wrap", "wrapped", &helper, "wrap_operation"),
        mixin("mixHead", "mixed", &["HEAD"], "inject"),
        mixin("mixWrap", "mixed", &helper, "wrap_operation"),
        mixin("va", "varArg", &["HEAD"], "modify_variable"),
        mixin("vn", "varNo", &["HEAD"], "modify_variable"),
        mixin("inLambda", "lambda$run$0", &["HEAD"], "inject"),
        mixin("hookHead", "hook", &["HEAD"], "inject"),
        mixin(
            "bodyWrap",
            "body",
            &["INVOKE:Lnet/minecraft/world/rt/Target;pure()V"],
            "wrap_operation",
        ),
        mixin("weakHead", "weak", &["HEAD"], "inject"),
        mixin("wm", "pure2", &[], "wrap_method"),
        into(
            mixin("backpackTick", "tick", &["HEAD"], "inject"),
            "fixture.mixin.BackpackMixin",
            ITEM_ENTITY,
        ),
    ];
    for (handler, method) in [
        ("tool", "level"),
        ("logged", "logged"),
        ("guardedOnce", "twice"),
        ("openOnce", "twice"),
        ("chest", "chestAt"),
        ("shape", "join"),
        ("shapeLike", "joinLike"),
    ] {
        out.push(into(
            mixin(handler, method, &["HEAD"], "inject"),
            "fixture.mixin.GuardedMixin",
            GUARDED,
        ));
    }
    for (handler, owner) in [
        ("panelInit", PANEL),
        ("mapInit", CHUNK_MAP),
        ("genInit", GEN),
    ] {
        out.push(into(
            mixin(handler, "<init>", &["TAIL"], "inject"),
            "fixture.mixin.CtorMixin",
            owner,
        ));
    }
    out
}

fn bridges() -> Bridges {
    BTreeSet::from([(
        format!("{RT}Target"),
        "bridged".to_owned(),
        "()V".to_owned(),
    )])
}

fn install_dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<tempfile::TempDir>> = OnceLock::new();
    DIR.get_or_init(|| {
        let out = routing_classes()?;
        let dir = tempfile::tempdir().unwrap();
        install(out, dir.path());
        Some(dir)
    })
    .as_ref()
    .map(tempfile::TempDir::path)
}

fn analyse(verdicts: &str) -> Option<(Replaces, Report)> {
    let dir = install_dir()?;
    let k = knowledge(verdicts);
    let mut r = analyse_with(dir, &k, None, Some(entries()), &mut |_| {}).unwrap();
    let vanilla = Vanilla::new(&r.hierarchy.jars).unwrap();
    let report = routing(&mut r, &vanilla, &k, &bridges(), &view_classes()).unwrap();
    Some((r, report))
}

fn report() -> Option<&'static Report> {
    static REPORT: OnceLock<Option<Report>> = OnceLock::new();
    REPORT
        .get_or_init(|| analyse("").map(|(_, report)| report))
        .as_ref()
}

fn route<'a>(report: &'a Report, name: &str, desc: &str) -> &'a Route {
    &report.routes[&format!("{KEY}{name}{desc}")]
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn open_except(fields: &[(&str, GuardValue)]) -> Option<Guards> {
    let mut g = Guards::default();
    for (field, value) in fields {
        let i = horizon_java_analysis::routing::GUARD_FIELDS
            .iter()
            .position(|f| f == field)
            .unwrap();
        g.0[i] = Some(value.clone());
    }
    Some(g)
}

fn classes(items: &[&str]) -> GuardValue {
    GuardValue::Classes(strings(items))
}

fn indexed(index: &str, items: &[&str]) -> GuardValue {
    GuardValue::Indexed(vec![(index.to_owned(), strings(items))])
}

#[test]
fn a_head_inject_with_the_targets_parameters_is_an_edge_and_a_captured_local_makes_a_body() {
    let Some(r) = report() else { return };
    let head = route(r, "head", "(I)V");
    assert_eq!(
        (
            head.kind.as_str(),
            head.family.as_str(),
            head.args.clone(),
            head.ret.as_str(),
            head.is_static
        ),
        ("edge", "value", strings(&["I"]), "V", false)
    );
    assert_eq!(
        (head.changes[0].cancellable, head.changes[0].locals),
        (true, false)
    );
    let local = route(r, "headLocal", "(I)V");
    assert_eq!(local.kind, "body");
    assert!(local.changes[0].locals);
}

#[test]
fn a_tail_inject_is_an_edge_that_counts_the_vanilla_returns() {
    let Some(r) = report() else { return };
    let tail = route(r, "tail", "(I)I");
    assert_eq!(
        (tail.kind.as_str(), tail.tail, tail.returns),
        ("edge", true, 2)
    );
    assert!(!route(r, "head", "(I)V").tail);
}

#[test]
fn a_wrap_operation_is_a_body_and_so_is_a_key_that_also_has_a_head_inject() {
    let Some(r) = report() else { return };
    assert_eq!(route(r, "wrapped", "()V").kind, "body");
    let mixed = route(r, "mixed", "()V");
    let mut kinds: Vec<&str> = mixed
        .changes
        .iter()
        .map(|c| c.kind.as_deref().unwrap())
        .collect();
    kinds.sort_unstable();
    assert_eq!(
        (mixed.kind.as_str(), kinds),
        ("body", vec!["inject", "wrap_operation"])
    );
}

#[test]
fn modify_variable_is_an_edge_only_at_head_with_args_only() {
    let Some(r) = report() else { return };
    assert_eq!(route(r, "varArg", "(I)V").kind, "edge");
    assert_eq!(route(r, "varNo", "(I)V").kind, "body");
}

#[test]
fn wrap_method_is_an_edge() {
    let Some(r) = report() else { return };
    assert_eq!(route(r, "pure2", "()V").kind, "edge");
}

#[test]
fn a_mixin_into_a_lambda_routes_the_enclosing_method_as_a_body() {
    let Some(r) = report() else { return };
    let run = route(r, "run", "()V");
    assert_eq!(
        (run.kind.as_str(), run.changes[0].lambda.as_deref()),
        ("body", Some("lambda$run$0()V"))
    );
    assert_eq!(
        r.lambdas,
        BTreeMap::from([(format!("{KEY}lambda$run$0()V"), format!("{KEY}run()V"))])
    );
    assert!(!r.routes.contains_key(&format!("{KEY}lambda$run$0()V")));
}

#[test]
fn overriders_say_whether_they_reach_the_base_body_through_super() {
    let Some(r) = report() else { return };
    let found: BTreeMap<&str, bool> = route(r, "hook", "()V")
        .overriders
        .iter()
        .map(|o| (o.class.rsplit('/').next().unwrap(), o.super_call))
        .collect();
    assert_eq!(
        found,
        BTreeMap::from([("S", true), ("T", false), ("U", true), ("V", false)])
    );
}

#[test]
fn the_closure_of_a_body_route_sorts_bridged_view_routed_and_pure_callees() {
    let Some(r) = report() else { return };
    let closure = &route(r, "body", "()V").closure;
    assert_eq!(closure.bridged, [format!("{KEY}bridged()V")]);
    assert_eq!(closure.needs_bridge, [ENTITY_SET_POS]);
    assert_eq!(
        closure.routed,
        [format!("{KEY}head(I)V"), format!("{KEY}pure2()V")]
    );
    assert_eq!(closure.pure, [format!("{KEY}pure()V")]);
    assert_eq!(route(r, "head", "(I)V").closure, Closure::default());
}

#[test]
fn a_weak_status_is_undecided_until_the_verdict_map_answers() {
    let Some(r) = report() else { return };
    let weak = format!("{KEY}weak()V");
    let undecided = &r.undecided[&weak];
    assert_eq!(r.undecided.len(), 1);
    assert_eq!(
        (undecided.status.as_str(), undecided.mods.clone()),
        ("mentioned", strings(&["fix"]))
    );
    assert!(!r.routes.contains_key(&weak));
    let (_, ported) = analyse("Target.weak\tport\n").unwrap();
    assert!(ported.undecided.is_empty());
    assert_eq!(ported.routes[&weak].verdict.as_deref(), Some("port"));
    let (skipped, report) = analyse("Target.weak\tmissing\n").unwrap();
    assert!(report.undecided.is_empty());
    assert_eq!(
        skipped.methods[&format!("{KEY}weak")].changes[0]
            .note
            .as_deref(),
        Some("native lacks it")
    );
    assert!(!report.routes.contains_key(&weak));
}

#[test]
fn a_private_helper_guard_admits_backpack_item_entities_only() {
    let Some(r) = report() else { return };
    assert_eq!(
        r.routes[&format!("{ITEM_ENTITY}#tick()V")].guards,
        open_except(&[
            ("recv", classes(&[ITEM_ENTITY])),
            ("recv_item", classes(&["fixture/FakeBackpackItem"])),
        ])
    );
}

#[test]
fn a_static_handler_guarded_by_its_stacks_item_admits_that_item_class() {
    let Some(r) = report() else { return };
    let key = format!(
        "{GUARDED}#level(Lnet/minecraft/core/Holder;Lnet/minecraft/world/item/ItemInstance;)I"
    );
    assert_eq!(
        r.routes[&key].guards,
        open_except(&[("arg_item", indexed("1", &["fixture/FakeTool"]))])
    );
}

#[test]
fn a_call_before_the_instanceof_stops_the_walk() {
    let Some(r) = report() else { return };
    let route = &r.routes[&format!("{GUARDED}#logged(Lnet/minecraft/world/item/ItemStack;)V")];
    assert_eq!(route.guards, None);
    assert_eq!(
        route.changes[0].guard_stop.as_deref(),
        Some("logged@0: getstatic java/lang/System.out")
    );
}

#[test]
fn one_open_change_opens_the_route() {
    let Some(r) = report() else { return };
    let route = &r.routes[&format!("{GUARDED}#twice(Lnet/minecraft/world/item/ItemStack;)V")];
    assert_eq!(route.guards, None);
    let by_handler: BTreeMap<&str, Option<Guards>> = route
        .changes
        .iter()
        .map(|c| (c.handler.as_str(), c.guards.clone()))
        .collect();
    assert_eq!(
        by_handler,
        BTreeMap::from([
            (
                "guardedOnce",
                open_except(&[("arg_item", indexed("0", &["fixture/FakeTool"]))])
            ),
            ("openOnce", None),
        ])
    );
}

#[test]
fn the_block_at_a_position_argument_is_a_guard() {
    let Some(r) = report() else { return };
    let key = format!(
        "{GUARDED}#chestAt(Lnet/minecraft/world/level/LevelReader;Lnet/minecraft/core/BlockPos;)Z"
    );
    assert_eq!(
        r.routes[&key].guards,
        open_except(&[("block_at", classes(&["fixture/FakeChestBlock"]))])
    );
}

#[test]
fn an_argument_class_guard_on_a_static_handler() {
    let Some(r) = report() else { return };
    let key = format!(
        "{GUARDED}#join(Lnet/minecraft/world/phys/shapes/VoxelShape;Lnet/minecraft/world/phys/shapes/VoxelShape;)Z"
    );
    assert_eq!(
        r.routes[&key].guards,
        open_except(&[("arg_class", indexed("0", &["fixture/FakeShape"]))])
    );
    assert!(r.routes[&key].inert);
    let chest = format!(
        "{GUARDED}#chestAt(Lnet/minecraft/world/level/LevelReader;Lnet/minecraft/core/BlockPos;)Z"
    );
    assert!(!r.routes[&chest].inert);
    let like = &r.routes[&key.replace("#join(", "#joinLike(")];
    assert_eq!(
        (like.guards.as_ref().unwrap().get("arg_class"), like.inert),
        (Some(&indexed("1", &["fixture/ShapeLike"])), false),
        "a mixin may add a mod interface to a vanilla class"
    );
}

#[test]
fn routes_of_unguarded_handlers_admit_all() {
    let Some(r) = report() else { return };
    assert_eq!(route(r, "head", "(I)V").guards, None);
}

#[test]
fn a_constructor_route_is_never_bound_and_says_who_runs_its_patches() {
    let Some(r) = report() else { return };
    let by: BTreeMap<&str, &Route> = [(PANEL, "(I)V"), (CHUNK_MAP, "(I)V"), (GEN, "()V")]
        .into_iter()
        .map(|(owner, desc)| (owner, &r.routes[&format!("{owner}#<init>{desc}")]))
        .collect();
    let got: BTreeMap<&str, (&str, bool)> = by
        .iter()
        .map(|(o, r)| (*o, (r.construct.as_ref().unwrap().by.as_str(), r.inert)))
        .collect();
    assert_eq!(
        got,
        BTreeMap::from([
            (PANEL, ("java", true)),
            (CHUNK_MAP, ("seam", true)),
            (GEN, ("deferred", false)),
        ])
    );
    assert!(
        by[GEN]
            .construct
            .as_ref()
            .unwrap()
            .reason
            .contains("no post-construct seam")
    );
    assert!(by[CHUNK_MAP].overriders.is_empty());
}

#[test]
fn the_view_list_is_read_from_view_inits() {
    assert!(view_classes().contains(CHUNK_MAP));
    assert!(view_classes().contains("net/minecraft/client/Minecraft"));
}

#[test]
fn the_tsv_and_the_reference_list_carry_no_handler_names() {
    let Some(r) = report() else { return };
    let rows: Vec<Vec<String>> = tsv(r)
        .lines()
        .map(|l| l.split('\t').map(str::to_owned).collect())
        .collect();
    assert!(rows.contains(&strings(&[
        "net/minecraft/world/rt/Target",
        "tail",
        "(I)I",
        "edge",
        "1",
        "2"
    ])));
    let listing = routes_tsv(r, "pack");
    assert_eq!(
        listing.lines().next(),
        Some("# routes of pack, mod_routing format 1")
    );
    assert!(listing.contains(&format!("{KEY}head(I)V\tedge\tvalue\t0\t1\tfix\t0")));
    assert!(!listing.contains("headInject"));
}
