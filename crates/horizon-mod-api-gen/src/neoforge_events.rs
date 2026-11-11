//! The NeoForge event classes the catalog names, each with the hooks that fire where it posts.
use super::{Hook, hook_id};
use std::{fs, path::Path};

/// Whether a counterpart is a class name (`RenderGuiLayerEvent.Pre`), not prose such as `none (...)`.
fn is_class(name: &str) -> bool {
    name.split('.').all(|part| {
        part.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            && part.chars().all(|c| c.is_ascii_alphanumeric())
    })
}

/// `pub const EVENTS`, sorted by event name: an event with more hooks than one has an entry per hook.
pub(super) fn table<'a>(hooks: impl Iterator<Item = &'a Hook>) -> String {
    let mut pairs: Vec<(String, String)> = hooks
        .flat_map(|hook| {
            hook.counterparts
                .neoforge
                .iter()
                .filter(|name| is_class(name))
                .map(|name| (name.clone(), hook_id(&hook.name)))
        })
        .collect();
    pairs.sort();
    pairs.dedup();
    let entries: String = pairs
        .iter()
        .map(|(event, id)| format!("({event:?}, super::HookId::{id}),\n"))
        .collect();
    format!("pub const EVENTS: &[(&str, super::HookId)] = &[\n{entries}];\n")
}

/// The marker line of a Java listener family file in `game/java/`.
pub(super) const FAMILY_MARKER: &str = "// java-listener-family";

/// `FAMILIES`: each `game/java/` module marked a listener family, by its `family` constructor.
pub(super) fn families(java: &Path) -> Result<String, String> {
    let mut modules = Vec::new();
    for entry in fs::read_dir(java).map_err(|e| format!("{}: {e}", java.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let source = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        if source.lines().any(|line| line.trim_end() == FAMILY_MARKER) {
            let stem = path.file_stem().and_then(|stem| stem.to_str());
            modules.push(stem.ok_or("a family file name is not UTF-8")?.to_owned());
        }
    }
    modules.sort();
    let entries: String = modules
        .iter()
        .map(|module| format!("super::{module}::family,\n"))
        .collect();
    Ok(format!(
        "// @generated from the `{FAMILY_MARKER}` files of game/java; do not edit.\npub(super) const FAMILIES: &[fn() -> Family] = &[\n{entries}];\n"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Side;

    fn hook(name: &str, neoforge: &[&str]) -> Hook {
        let mut hook = Hook::sample("horizon_client::game::Game<'_>::native_x", Side::Client);
        hook.name = name.into();
        hook.counterparts.neoforge = neoforge.iter().map(|n| n.to_string()).collect();
        hook
    }

    #[test]
    fn events_list_class_names_sorted_with_every_hook_of_each() {
        let hooks = [
            hook("gui_layer_pre", &["RenderGuiLayerEvent.Pre"]),
            hook("mod_layer_pre", &["RenderGuiLayerEvent.Pre"]),
            hook("break_block", &["BreakBlockEvent"]),
            hook("frame", &["none (NeoForge has no such event)"]),
            hook(
                "place",
                &[
                    "BlockEvent.EntityPlaceEvent",
                    "BlockEvent.EntityMultiPlaceEvent",
                ],
            ),
        ];
        let code = table(hooks.iter());
        let entries: Vec<_> = code.lines().filter(|l| l.starts_with('(')).collect();
        assert_eq!(
            entries,
            [
                "(\"BlockEvent.EntityMultiPlaceEvent\", super::HookId::Place),",
                "(\"BlockEvent.EntityPlaceEvent\", super::HookId::Place),",
                "(\"BreakBlockEvent\", super::HookId::BreakBlock),",
                "(\"RenderGuiLayerEvent.Pre\", super::HookId::GuiLayerPre),",
                "(\"RenderGuiLayerEvent.Pre\", super::HookId::ModLayerPre),",
            ]
        );
    }

    #[test]
    fn families_are_the_marked_files_only() {
        let dir = std::env::temp_dir().join(format!("hz-families-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("java_listen_b.rs"),
            format!("{FAMILY_MARKER}\nfn family() {{}}\n"),
        )
        .unwrap();
        fs::write(
            dir.join("java_listen_a.rs"),
            format!("//! A.\n{FAMILY_MARKER}\n"),
        )
        .unwrap();
        fs::write(dir.join("java_other.rs"), "// java-listener-family-not\n").unwrap();
        fs::write(dir.join("notes.txt"), FAMILY_MARKER).unwrap();
        let code = families(&dir).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        let entries: Vec<_> = code.lines().filter(|l| l.starts_with("super::")).collect();
        assert_eq!(
            entries,
            [
                "super::java_listen_a::family,",
                "super::java_listen_b::family,"
            ]
        );
        syn::parse_file(&code).unwrap();
    }
}
