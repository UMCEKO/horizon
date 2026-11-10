//! Superclasses and interfaces of every class the libraries and the mods define.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use crate::libraries::LibClass;

const CANCELLABLE: &str = "net/neoforged/bus/api/ICancellableEvent";
const MOD_BUS_EVENT: &str = "net/neoforged/fml/event/IModBusEvent";

/// What the hierarchy says about an event class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventFacts {
    pub cancellable: Option<bool>,
    pub bus: Option<&'static str>,
}

/// Port of mod_replaces.Hierarchy.
pub struct Hierarchy {
    pub libs: BTreeMap<String, LibClass>,
    pub jars: Vec<PathBuf>,
    heads: HashMap<String, (Option<String>, Vec<String>)>,
    memo: RefCell<HashMap<String, Vec<String>>>,
}

impl Hierarchy {
    /// Port of mod_replaces.Hierarchy.__init__.
    pub fn new(libs: BTreeMap<String, LibClass>) -> Hierarchy {
        let heads = libs
            .iter()
            .map(|(n, c)| (n.clone(), (c.sup.clone(), c.ifaces.clone())))
            .collect();
        Hierarchy {
            libs,
            jars: Vec::new(),
            heads,
            memo: RefCell::new(HashMap::new()),
        }
    }

    /// Port of mod_replaces.Hierarchy.add.
    pub fn add(&mut self, name: &str, sup: Option<String>, ifaces: Vec<String>) {
        self.heads.insert(name.to_owned(), (sup, ifaces));
    }

    /// Port of mod_replaces.Hierarchy.ancestors.
    pub fn ancestors(&self, name: &str) -> Vec<String> {
        if let Some(found) = self.memo.borrow().get(name) {
            return found.clone();
        }
        let mut seen: Vec<String> = Vec::new();
        let mut known: HashSet<String> = HashSet::new();
        let mut queue = std::collections::VecDeque::from([name.to_owned()]);
        while let Some(next) = queue.pop_front() {
            let Some((sup, ifaces)) = self.heads.get(&next) else {
                continue;
            };
            for parent in sup.iter().chain(ifaces) {
                if known.insert(parent.clone()) {
                    seen.push(parent.clone());
                    queue.push_back(parent.clone());
                }
            }
        }
        self.memo.borrow_mut().insert(name.to_owned(), seen.clone());
        seen
    }

    /// Port of mod_replaces.Hierarchy.is_a.
    pub fn is_a(&self, name: &str, target: &str) -> bool {
        name == target || self.ancestors(name).iter().any(|a| a == target)
    }

    /// Port of mod_replaces.Hierarchy.methods_of.
    pub fn methods_of(&self, cls: &str) -> &[(String, String, u16)] {
        self.libs.get(cls).map_or(&[], |c| &c.methods)
    }

    /// Port of mod_replaces.Hierarchy.vanilla_ancestors.
    pub fn vanilla_ancestors(&self, name: &str) -> Vec<String> {
        self.ancestors(name)
            .into_iter()
            .filter(|a| !self.methods_of(a).is_empty())
            .collect()
    }

    /// Port of mod_replaces.Hierarchy.declarer.
    pub fn declarer(&self, cls: &str, name: &str, desc: Option<&str>) -> String {
        let chain = std::iter::once(cls.to_owned()).chain(self.ancestors(cls));
        for c in chain {
            if self
                .methods_of(&c)
                .iter()
                .any(|m| m.0 == name && desc.is_none_or(|d| m.1 == d))
            {
                return c;
            }
        }
        cls.to_owned()
    }

    /// Port of mod_replaces.Hierarchy.event_facts.
    pub fn event_facts(&self, event: &str) -> EventFacts {
        if !self.heads.contains_key(event) {
            return EventFacts {
                cancellable: None,
                bus: None,
            };
        }
        EventFacts {
            cancellable: Some(self.is_a(event, CANCELLABLE)),
            bus: Some(if self.is_a(event, MOD_BUS_EVENT) {
                "mod"
            } else {
                "game"
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(sup: Option<&str>, ifaces: &[&str], methods: &[(&str, &str)]) -> LibClass {
        LibClass {
            sup: sup.map(str::to_owned),
            ifaces: ifaces.iter().map(|s| (*s).to_owned()).collect(),
            access: 1,
            methods: methods
                .iter()
                .map(|(n, d)| ((*n).to_owned(), (*d).to_owned(), 1))
                .collect(),
        }
    }

    fn sample() -> Hierarchy {
        let mut libs = BTreeMap::new();
        libs.insert("C".to_owned(), class(Some("B"), &[], &[]));
        libs.insert("B".to_owned(), class(Some("A"), &["I"], &[("m", "()V")]));
        libs.insert("I".to_owned(), class(None, &["J"], &[("m", "(I)V")]));
        Hierarchy::new(libs)
    }

    #[test]
    fn ancestors_are_breadth_first_with_the_super_before_interfaces() {
        let h = sample();
        assert_eq!(h.ancestors("C"), ["B", "A", "I", "J"]);
        assert!(h.is_a("C", "J"));
        assert!(h.is_a("C", "C"));
        assert!(!h.is_a("J", "C"));
    }

    #[test]
    fn declarer_finds_the_nearest_declaring_class() {
        let h = sample();
        assert_eq!(h.declarer("C", "m", None), "B");
        assert_eq!(h.declarer("C", "m", Some("(I)V")), "I");
        assert_eq!(h.declarer("C", "x", None), "C");
        assert_eq!(h.vanilla_ancestors("C"), ["B", "I"]);
    }

    #[test]
    fn added_classes_extend_the_chain_and_event_facts_follow() {
        let mut h = sample();
        h.add("Ev", Some("Base".to_owned()), vec![]);
        h.add("Base", None, vec![CANCELLABLE.to_owned()]);
        let facts = h.event_facts("Ev");
        assert_eq!(facts.cancellable, Some(true));
        assert_eq!(facts.bus, Some("game"));
        assert_eq!(h.event_facts("Nope").cancellable, None);
    }
}
