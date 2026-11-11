//! The state groups of the entity methods mod code calls, from `data/entity_groups.tsv` and `data/entity_writers.tsv`.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

const GROUPS_TSV: &str = include_str!("../../data/entity_groups.tsv");
const WRITERS_TSV: &str = include_str!("../../data/entity_writers.tsv");

/// The text before the unclassified methods; edit the data file named in it.
pub const UNCLASSIFIED: &str = "entity methods without a state group (add them to crates/horizon-java-analysis/data/entity_groups.tsv):";

/// The state groups, in the bit order of `entities.bin`.
pub const STATE_GROUPS: [&str; 16] = [
    "identity",
    "position",
    "motion",
    "health",
    "effects",
    "equipment",
    "fire",
    "air",
    "flags",
    "name",
    "item",
    "riding",
    "removal",
    "species",
    "attributes",
    "ai",
];

/// Groups whose methods need no state in a view.
pub const UNCARRIED: [&str; 2] = ["java", "pure"];

struct Writers {
    extra: HashSet<&'static str>,
    not: HashSet<&'static str>,
    prefixes: Vec<&'static str>,
}

/// The data rows: tab separated, a line starting with `#` is a comment.
fn rows(text: &'static str) -> impl Iterator<Item = (&'static str, &'static str)> {
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
}

/// Port of mod_entity_facts.GROUPS: a later row wins for a repeated name.
fn group_table(text: &'static str) -> HashMap<&'static str, &'static str> {
    rows(text).map(|(group, name)| (name, group)).collect()
}

static GROUPS: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| group_table(GROUPS_TSV));

static WRITERS: LazyLock<Writers> = LazyLock::new(|| {
    let mut w = Writers {
        extra: HashSet::new(),
        not: HashSet::new(),
        prefixes: Vec::new(),
    };
    for (kind, name) in rows(WRITERS_TSV) {
        match kind {
            "writer" => {
                w.extra.insert(name);
            }
            "not" => {
                w.not.insert(name);
            }
            "prefix" => w.prefixes.push(name),
            other => panic!("entity_writers.tsv: unknown row kind {other}"),
        }
    }
    w
});

/// Port of mod_entity_facts.carried.
pub fn carried(group: &str) -> Vec<&str> {
    if group == "all" {
        STATE_GROUPS.to_vec()
    } else {
        vec![group]
    }
}

/// Port of mod_entity_facts.classify.
pub fn classify(owner: &str, name: &str) -> Option<(&'static str, bool)> {
    let simple = owner.rsplit('/').next().unwrap_or(owner);
    let group = GROUPS
        .get(format!("{simple}#{name}").as_str())
        .or_else(|| GROUPS.get(name))?;
    let w = &*WRITERS;
    let writes = (w.prefixes.iter().any(|p| name.starts_with(p)) || w.extra.contains(name))
        && !w.not.contains(name);
    Some((group, writes))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTITY: &str = "net/minecraft/world/entity/Entity";
    const ITEM: &str = "net/minecraft/world/entity/item/ItemEntity";

    #[test]
    fn a_class_qualified_row_beats_the_bare_name() {
        assert_eq!(classify(ITEM, "getItem"), Some(("item", false)));
        assert_eq!(classify(ENTITY, "getItem"), None);
    }

    #[test]
    fn prefixes_and_extra_writers_write_and_not_writers_do_not() {
        assert_eq!(classify(ITEM, "setHealth"), Some(("health", true)));
        assert_eq!(classify(ENTITY, "makeStuckInBlock"), Some(("motion", true)));
        assert_eq!(classify(ENTITY, "setChanged"), Some(("pure", false)));
        assert_eq!(classify(ENTITY, "getPersistentData"), Some(("java", false)));
        assert_eq!(classify(ENTITY, "load"), Some(("all", true)));
    }

    #[test]
    fn an_unnamed_method_has_no_group() {
        assert_eq!(classify(ITEM, "nothingLikeThis"), None);
    }

    #[test]
    fn the_last_row_of_a_repeated_name_wins_and_the_comment_line_is_skipped() {
        let table = group_table("# group\tmethod; note\nair\tfoo\nfire\tfoo\nair\tbar\n");
        assert_eq!(table["foo"], "fire");
        assert_eq!(table["bar"], "air");
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn whole_entity_save_and_load_need_every_group() {
        assert_eq!(carried("all"), STATE_GROUPS);
        assert_eq!(STATE_GROUPS.len(), 16);
        assert_eq!(carried("air"), ["air"]);
    }
}
