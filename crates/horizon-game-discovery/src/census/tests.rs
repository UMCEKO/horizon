use std::{fs, path::PathBuf};

use super::*;

/// Writes `files` under `<tmp>/crates/horizon-client/src` and returns the checkout root.
fn tree(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = std::env::temp_dir().join(format!("census-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for (path, text) in files {
        let path = root.join("crates/horizon-client/src").join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    root
}

const GAME_RS: &str = r#"
struct Game<'a> {
    hud: u8,
    world: u8,
    shared: u8,
    dual: u8,
    lan: Option<u8>,
    remote: Option<u8>,
    spare: u8,
    marker: std::marker::PhantomData<&'a ()>,
}

impl Game<'_> {
    fn render(&mut self) {
        self.draw_hud();
        self.shared_step();
        let _ = format!("{}", self.in_macro());
        self.lan_helper();
        let f = |g: &mut Game| g.closure_callee();
        f(self);
        Self::assoc_callee();
        self.tick();
    }
    fn tick_server_body(&mut self) {
        self.save_world();
        self.shared_step();
        self.lan_helper();
        self.tick();
        if self.remote.is_some() { self.remote_in_server(); }
        if self.lan.is_some() { self.server_branch(); } else { self.else_of_live(); }
    }
    fn poll_remote(&mut self) {
        if self.remote.is_some() { self.client_poll(); return; }
        self.after_return();
    }
    fn lan_helper(&mut self) {
        let Some(_l) = self.lan.as_mut() else { return };
        self.lan_tail();
    }
    fn lan_tail(&mut self) {
        self.world += 1;
    }
    fn tick(&mut self) {
        self.tick_world();
        self.tick_remote();
    }
    fn tick_world(&mut self) {}
    fn tick_remote(&mut self) {}
}

#[cfg(test)]
mod tests {
    impl Game<'_> {
        fn in_cfg_test(&mut self) {}
    }
}
"#;

const HUD_RS: &str = r#"
impl Game<'_> {
    fn draw_hud(&mut self) {
        self.hud += 1;
        self.dual += 1;
    }
    fn in_macro(&self) -> u8 { 0 }
    fn closure_callee(&mut self) {}
    fn assoc_callee() {}
    fn client_poll(&mut self) {}
    fn host_client_only(&mut self) {}
}
"#;

const SAVE_RS: &str = r#"
impl<'a> Game<'a> {
    fn save_world(&mut self) {
        self.world += 1;
        self.dual += 1;
    }
    fn server_branch(&mut self) {}
    fn host_server_only(&mut self) {}
}
"#;

const SHARED_RS: &str = r#"
impl Game<'_> {
    fn shared_step(&mut self) {
        self.shared += 1;
    }
}
"#;

const HOST_RS: &str = r#"
impl ClientHost for Game<'_> {
    fn on_frame(&mut self) { self.host_client_only(); }
}
impl ServerHost for Game<'_> {
    fn on_tick(&mut self) { self.host_server_only(); self.shared_step(); }
}
impl Drop for Game<'_> {
    fn drop(&mut self) { self.drop_callee(); }
}
"#;

const DEAD_RS: &str = r#"
impl Game<'_> {
    fn remote_in_server(&mut self) {}
    fn else_of_live(&mut self) {}
    fn after_return(&mut self) {}
    fn drop_callee(&mut self) {}
}
"#;

const TWIN_SHORT_RS: &str = "impl Game<'_> { fn twin(&mut self) {} }";
const TWIN_LONG_RS: &str = "impl Game<'_> {\n    fn twin(&mut self) {\n        let a = 1;\n        let _b = a;\n    }\n}\n";
const TEST_FILE_RS: &str =
    "impl Game<'_> { fn only_in_a_test_file(&mut self) { self.draw_hud(); } }";

fn fixture(name: &str) -> Census {
    let root = tree(
        name,
        &[
            ("game.rs", GAME_RS),
            ("game/ui/hud.rs", HUD_RS),
            ("game/ui/hud_tests.rs", TEST_FILE_RS),
            ("game/world/save.rs", SAVE_RS),
            ("game/world/shared.rs", SHARED_RS),
            ("game/net/host.rs", HOST_RS),
            ("game/net/dead.rs", DEAD_RS),
            ("game/net/twin_a.rs", TWIN_SHORT_RS),
            ("game/net/twin_b.rs", TWIN_LONG_RS),
        ],
    );
    let census = run(&root).unwrap();
    fs::remove_dir_all(root).unwrap();
    census
}

#[test]
fn each_method_gets_the_side_that_reaches_it() {
    use Side::{Both, Client, Neither, Server};
    let census = fixture("methods");
    let mut got: Vec<(&str, Side)> = census
        .methods
        .iter()
        .map(|m| (m.name.as_str(), m.side))
        .collect();
    got.sort();
    let mut want = vec![
        // client roots, and what they call through a macro, a closure and `Self::`
        ("render", Client),
        ("poll_remote", Client),
        ("draw_hud", Client),
        ("in_macro", Client),
        ("closure_callee", Client),
        ("assoc_callee", Client),
        ("<ClientHost>::on_frame", Client),
        ("host_client_only", Client),
        // `if self.remote.is_some() { .. return }` ends the client's poll_remote
        ("client_poll", Client),
        ("after_return", Neither),
        // server roots
        ("tick_server_body", Server),
        ("save_world", Server),
        ("<ServerHost>::on_tick", Server),
        ("host_server_only", Server),
        // a guard of the other side fences its block, and the else of an always-true guard
        ("remote_in_server", Neither),
        ("server_branch", Server),
        ("else_of_live", Neither),
        // the client leaves lan_helper at its let-else, the server runs on to lan_tail
        ("lan_helper", Both),
        ("lan_tail", Server),
        // `tick` is cut: the client never enters tick_world, the server never tick_remote
        ("tick", Both),
        ("tick_world", Server),
        ("tick_remote", Client),
        ("shared_step", Both),
        // nobody calls Drop
        ("<Drop>::drop", Neither),
        ("drop_callee", Neither),
        ("twin", Neither),
    ];
    want.sort();
    assert_eq!(got, want);
}

#[test]
fn fields_are_classified_by_the_methods_that_touch_them() {
    use FieldKind::{Client, Direct, Server, ServerShared, Shared, Untouched};
    let census = fixture("fields");
    let got: Vec<(&str, FieldKind, [usize; 3])> = census
        .fields
        .iter()
        .map(|f| {
            (
                f.name.as_str(),
                f.kind,
                [f.client_methods, f.server_methods, f.both_methods],
            )
        })
        .collect();
    let want = [
        ("hud", Client, [1, 0, 0]),
        ("world", Server, [0, 2, 0]),
        ("shared", Shared, [0, 0, 1]),
        ("dual", Direct, [1, 1, 0]),
        ("lan", ServerShared, [0, 1, 1]),
        ("remote", Direct, [1, 1, 0]),
        ("spare", Untouched, [0, 0, 0]),
        ("marker", Untouched, [0, 0, 0]),
    ];
    assert_eq!(got, want);
    assert_eq!(
        census.totals.fields,
        SideCounts {
            server: 2,
            client: 1,
            both: 3,
            neither: 2
        }
    );
}

#[test]
fn files_and_areas_add_up_from_their_methods() {
    let census = fixture("files");
    let purity: Vec<(&str, FilePurity)> = census
        .files
        .iter()
        .map(|f| (f.file.as_str(), f.purity))
        .collect();
    assert_eq!(
        purity,
        [
            ("game.rs", FilePurity::Mixed),
            ("game/net/dead.rs", FilePurity::NeitherOnly),
            ("game/net/host.rs", FilePurity::Mixed),
            ("game/net/twin_b.rs", FilePurity::NeitherOnly),
            ("game/ui/hud.rs", FilePurity::PureClient),
            ("game/world/save.rs", FilePurity::PureServer),
            ("game/world/shared.rs", FilePurity::PureBoth),
        ]
    );
    // draw_hud is 4 lines and its five siblings 1 each
    let ui = census.areas.iter().find(|a| a.area == "ui").unwrap();
    assert_eq!(
        ui.lines,
        SideCounts {
            server: 0,
            client: 9,
            both: 0,
            neither: 0
        }
    );
    assert_eq!(ui.methods.client, 6);
    // save_world is 4 lines, server_branch and host_server_only 1 each, shared_step 3
    let world = census.areas.iter().find(|a| a.area == "world").unwrap();
    assert_eq!(
        world.lines,
        SideCounts {
            server: 6,
            client: 0,
            both: 3,
            neither: 0
        }
    );
    let total: usize = census.methods.iter().map(|m| m.lines).sum();
    assert_eq!(census.totals.lines.total(), total);
}

#[test]
fn test_code_is_left_out_and_cfg_twins_keep_the_longer_body() {
    let census = fixture("twins");
    assert!(
        census
            .methods
            .iter()
            .all(|m| m.name != "in_cfg_test" && m.name != "only_in_a_test_file")
    );
    let twin = census.methods.iter().find(|m| m.name == "twin").unwrap();
    assert_eq!((twin.file.as_str(), twin.lines), ("game/net/twin_b.rs", 4));
    assert_eq!(
        census.duplicates,
        [Duplicate {
            method: "twin".into(),
            files: vec!["game/net/twin_a.rs".into()]
        }]
    );
    // roots the census looks for that the fixture does not define
    assert!(census.missing_roots.contains(&"handle_packet".to_owned()));
    assert!(!census.missing_roots.contains(&"render".to_owned()));
}

#[test]
fn json_carries_the_whole_census() {
    let census = fixture("json");
    let back: Census = serde_json::from_str(&census.json().unwrap()).unwrap();
    assert_eq!(back, census);
}

const GUARDS_RS: &str = r#"
struct Game<'a> { ready: bool, marker: std::marker::PhantomData<&'a ()> }

impl Game<'_> {
    fn tick_server_body(&mut self) {
        if self.options.execution_role.is_client() { self.c1(); }
        if !self.options.execution_role.is_client() { self.s1(); } else { self.c2(); }
        if let Some(_w) = self.window.as_ref() { self.c3(); }
        if self.ready && self.integrated.is_some() { self.c4(); }
        if !self.is_logical_server() { self.c5(); }
        self.gate();
    }
    fn tick_remote(&mut self) {
        if self.options.dedicated.is_some() { self.s2(); }
        if self.is_dedicated_server() || self.ready { self.u1(); }
        self.gate();
    }
    fn gate(&mut self) {
        let Some(_s) = self.serving.as_mut() else { self.c6(); return; };
        self.s3();
    }
    fn c1(&mut self) {}
    fn c2(&mut self) {}
    fn c3(&mut self) {}
    fn c4(&mut self) {}
    fn c5(&mut self) {}
    fn c6(&mut self) {}
    fn s1(&mut self) {}
    fn s2(&mut self) {}
    fn s3(&mut self) {}
    fn u1(&mut self) {}
}
"#;

#[test]
fn every_guard_form_fences_the_other_sides_blocks() {
    use Side::{Both, Client, Neither, Server};
    let root = tree("guards", &[("game.rs", GUARDS_RS)]);
    let census = run(&root).unwrap();
    fs::remove_dir_all(root).unwrap();
    let side = |name: &str| census.methods.iter().find(|m| m.name == name).unwrap().side;
    // the server never runs client-guarded blocks, however they are written; `||` and plain conditions are not guards
    for name in ["c1", "c2", "c3", "c4", "c5"] {
        assert_eq!(side(name), Neither, "{name}");
    }
    assert_eq!(side("s1"), Server);
    assert_eq!(side("s2"), Neither);
    assert_eq!(side("u1"), Client);
    // the client takes the else of its let-else and stops; the server reads on past it
    assert_eq!((side("gate"), side("c6"), side("s3")), (Both, Both, Server));
}
