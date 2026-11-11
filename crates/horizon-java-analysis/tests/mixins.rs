//! The mixin scan against classes javac compiles from `tests/java/` and jars built from them.

mod common;

use std::io::{Cursor, Write};

use common::classes;
use horizon_java_analysis::mixins::{
    MixinEntry, jar_mixin_classes, mixins_json, mixins_markdown, scan_class,
};
use zip::ZipArchive;
use zip::write::SimpleFileOptions;

fn load(name: &str) -> Option<Vec<u8>> {
    classes().map(|dir| std::fs::read(dir.join(name)).unwrap())
}

fn zip_of(files: &[(&str, &[u8])]) -> ZipArchive<Cursor<Vec<u8>>> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(data).unwrap();
    }
    ZipArchive::new(Cursor::new(writer.finish().unwrap().into_inner())).unwrap()
}

fn strs(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn handlers_come_out_in_method_order_with_their_kinds() {
    let Some(data) = load("mixfix/M.class") else {
        return;
    };
    let entries = scan_class("m", "m.mixins.json", "client", &data).unwrap();
    let got: Vec<_> = entries
        .iter()
        .map(|e| {
            (
                e.handler.as_str(),
                e.kind.as_str(),
                e.target_method.clone(),
                e.at.clone(),
                e.access,
            )
        })
        .collect();
    let want = vec![
        (
            "h",
            "inject",
            strs(&["tick"]),
            strs(&["INVOKE:La;b()V"]),
            false,
        ),
        ("must", "overwrite", strs(&["must"]), vec![], false),
        ("getX", "accessor", strs(&["x"]), vec![], true),
        (
            "c",
            "modify_constant",
            strs(&["tick"]),
            strs(&["CONSTANT:intValue=5"]),
            false,
        ),
    ];
    assert_eq!(got, want);
    for e in &entries {
        assert_eq!(e.target_class, ["net.minecraft.world.Base"]);
        assert_eq!(
            (
                e.mod_id.as_str(),
                e.config.as_str(),
                e.side.as_str(),
                e.mixin.as_str()
            ),
            ("m", "m.mixins.json", "client", "mixfix.M")
        );
    }
}

#[test]
fn mixin_targets_are_classes_then_dotted_strings_and_constants_render_deterministically() {
    let Some(data) = load("mixfix/Rich.class") else {
        return;
    };
    let entries = scan_class("m", "c", "common", &data).unwrap();
    assert_eq!(entries.len(), 2, "the unmarked method is skipped");
    assert_eq!(
        entries[0].target_class,
        ["java.util.List", "int[]", "a.b.Hidden"]
    );
    assert_eq!(entries[0].at, ["HEAD"]);
    assert_eq!(entries[0].target_method, Vec::<String>::new());
    let condition = "Lorg/spongepowered/asm/mixin/injection/Constant$Condition;";
    let want = format!(
        "CONSTANT:doubleValue=1.5E10,stringValue=s,classValue=[Ljava/lang/String;,expandZeroConditions=[{condition}.GREATER_THAN_ZERO,{condition}.LESS_THAN_ZERO]"
    );
    assert_eq!(entries[1].at, [want]);
}

#[test]
fn a_jar_yields_its_declared_classes_and_warns_about_missing_ones() {
    let (Some(m), Some(rich)) = (load("mixfix/M.class"), load("mixfix/Rich.class")) else {
        return;
    };
    let toml = b"[[mods]]\nmodId = \"themod\"\n[[mixins]]\nconfig = \"t.mixins.json\"\n";
    let config = br#"// comment
{"package": "mixfix", "plugin": "p.Plugin", "mixins": ["M", "Gone"], "client": ["Rich"]}"#;
    let inner = zip_of(&[
        ("META-INF/MANIFEST.MF", b"MixinConfigs: n.mixins.json\n"),
        ("mixfix/M.class", &m),
        ("n.mixins.json", br#"{"mixins": ["mixfix.M"]}"#),
    ]);
    let inner_bytes = inner.into_inner().into_inner();
    let manifest = b"Manifest-Version: 1.0\r\nMixinConfigs: n.mixins.json,\r\n t.mixins.json\r\n";
    let mut jar = zip_of(&[
        ("META-INF/neoforge.mods.toml", toml),
        ("META-INF/MANIFEST.MF", manifest),
        ("t.mixins.json", config),
        ("mixfix/M.class", &m),
        ("mixfix/Rich.class", &rich),
        ("META-INF/jarjar/x.jar", &inner_bytes),
    ]);
    let mut warnings = Vec::new();
    let found = jar_mixin_classes(&mut jar, None, "themod-1.0", &mut warnings).unwrap();
    let got: Vec<_> = found
        .iter()
        .map(|(m, c, s, n, b)| {
            (
                m.as_str(),
                c.as_str(),
                s.as_str(),
                n.as_str(),
                !b.is_empty(),
            )
        })
        .collect();
    let want = vec![
        ("themod", "t.mixins.json", "common", "mixfix.M", true),
        ("themod", "t.mixins.json", "client", "mixfix.Rich", true),
        ("themod", "n.mixins.json", "common", "mixfix.M", true),
    ];
    assert_eq!(got, want);
    assert_eq!(
        warnings,
        [
            "themod: t.mixins.json has a config plugin (p.Plugin) that may drop or add mixins at runtime",
            "themod: t.mixins.json lists mixfix.Gone, which is not in the jar",
            "themod: config n.mixins.json is not in the jar",
        ]
    );
}

#[test]
fn a_config_that_is_not_utf8_is_an_error_naming_the_jar() {
    let mut jar = zip_of(&[
        (
            "META-INF/neoforge.mods.toml",
            b"[[mixins]]\nconfig = \"c.json\"\n",
        ),
        ("c.json", &[b'{', 0xff, b'}']),
    ]);
    let err = jar_mixin_classes(&mut jar, None, "bad-jar", &mut Vec::new()).unwrap_err();
    assert!(err.starts_with("bad-jar: c.json"), "{err}");
}

fn entry(mod_id: &str, kind: &str, target: &str, access: bool) -> MixinEntry {
    MixinEntry {
        mod_id: mod_id.to_owned(),
        config: "c".to_owned(),
        side: "client".to_owned(),
        mixin: "p.M".to_owned(),
        handler: "h".to_owned(),
        kind: kind.to_owned(),
        target_class: strs(&[target]),
        target_method: Vec::new(),
        at: strs(&["HEAD", "caf\u{e9}"]),
        access,
    }
}

#[test]
fn the_summary_counts_per_mod_and_ranks_targets_by_count_then_first_appearance() {
    let entries = vec![
        entry("b", "inject", "T1", false),
        entry("a", "accessor", "T2", true),
        entry("a", "inject", "T3", false),
        entry("a", "inject", "T3", false),
    ];
    let want = "# Mixin handlers: 4 in 7 mixin classes\n\
\n\
| mod | behaviour | access | accessor | inject |\n\
|---|---|---|---|---|\n\
| a | 2 | 1 | 1 | 2 |\n\
| b | 1 | 0 | 0 | 1 |\n\
\n\
## Top target classes (behaviour handlers)\n\
\n\
| class | handlers |\n\
|---|---|\n\
| T3 | 2 |\n\
| T1 | 1 |\n\
\n\
## Notes\n\
\n\
- careful\n";
    assert_eq!(mixins_markdown(&entries, 7, &strs(&["careful"])), want);
}

#[test]
fn the_json_is_pythons_indent_one_with_ascii_escapes() {
    let want = "[\n {\n  \"mod\": \"a\",\n  \"config\": \"c\",\n  \"mixin\": \"p.M\",\n  \"handler\": \"h\",\n  \"kind\": \"inject\",\n  \"target_class\": [\n   \"T\"\n  ],\n  \"target_method\": [],\n  \"at\": [\n   \"HEAD\",\n   \"caf\\u00e9\"\n  ],\n  \"side\": \"client\",\n  \"access\": false\n }\n]\n";
    assert_eq!(mixins_json(&[entry("a", "inject", "T", false)]), want);
    assert_eq!(mixins_json(&[]), "[]\n");
}
