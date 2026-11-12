//! Entity facts from the jars of an install: bytecode helpers, entity type ids, listener guards, state groups and `entity-facts.json`.

pub mod bytecode;
pub mod facts;
pub mod groups;
pub mod guards;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use serde_json::{Map, Value as Json, json};

use crate::class::{ClassInfo, Parse, parse_class};
use crate::hierarchy::Hierarchy;
use crate::jars::{self, Archive};
use crate::libraries::{entry_names, library_jars, load_library_index};
use crate::replaces::class_listeners;
use guards::{ENTITY_EVENT, EVENT_PREFIXES, listener_guards, type_ids};

fn contains(data: &[u8], needle: &[u8]) -> bool {
    data.windows(needle.len()).any(|w| w == needle)
}

/// The entity type ids and the guards of each entity-event listener of the mods, keyed `mod|class|method|event`.
pub fn dump_guards(install: &Path) -> Result<Json, String> {
    let library = library_jars(&install.join("libraries"))?;
    let mut hier = Hierarchy::new(load_library_index(&library, None)?);
    hier.jars = library;
    let mut archives: Vec<Archive> = Vec::new();
    let mut classes: Vec<(usize, String, String, bool)> = Vec::new();
    for jar in jars::mod_jars(install) {
        for archive in jars::walk(&jar)? {
            archives.push(archive);
            let index = archives.len() - 1;
            let zip = &mut archives[index].zip;
            for entry in entry_names(zip)? {
                if !entry.ends_with(".class")
                    || entry.starts_with("META-INF/")
                    || entry == "module-info.class"
                {
                    continue;
                }
                let data = jars::read_entry(zip, &entry)?;
                let Ok(head) = parse_class(
                    &data,
                    Parse {
                        members: false,
                        annotations: false,
                        code: false,
                    },
                ) else {
                    continue;
                };
                hier.add(&head.name, head.sup.clone(), head.ifaces.clone());
                let marked = contains(&data, b"addListener")
                    || contains(&data, b"addGenericListener")
                    || contains(&data, b"SubscribeEvent");
                classes.push((index, entry, head.name, marked));
            }
        }
    }
    let ids = type_ids(&hier.jars)?;
    let by_name: HashMap<&str, (usize, &str)> = classes
        .iter()
        .map(|(a, entry, name, _)| (name.as_str(), (*a, entry.as_str())))
        .collect();
    let archives = RefCell::new(archives);
    let cache: RefCell<HashMap<String, Option<Rc<ClassInfo>>>> = RefCell::new(HashMap::new());
    let infos = |name: &str| -> Option<Rc<ClassInfo>> {
        if let Some(found) = cache.borrow().get(name) {
            return found.clone();
        }
        let parsed = by_name.get(name).and_then(|(a, entry)| {
            let data = jars::read_entry(&mut archives.borrow_mut()[*a].zip, entry).ok()?;
            parse_class(&data, Parse::all()).ok().map(Rc::new)
        });
        cache.borrow_mut().insert(name.to_owned(), parsed.clone());
        parsed
    };
    let mut listeners = Map::new();
    for (a, _, name, marked) in &classes {
        if !marked {
            continue;
        }
        let Some(info) = infos(name) else {
            continue;
        };
        let mod_id = archives.borrow()[*a].mod_id.clone();
        for found in class_listeners(&info, &hier) {
            let Some(event) = found.event else {
                continue;
            };
            if !(EVENT_PREFIXES.iter().any(|p| event.starts_with(p))
                || hier.is_a(&event, ENTITY_EVENT))
            {
                continue;
            }
            let guard = listener_guards(&infos, found.implementation.as_ref(), &ids)
                .map_err(|e| e.to_string())?;
            listeners.insert(
                format!(
                    "{mod_id}|{}|{}|{event}",
                    name.replace('/', "."),
                    found.method
                ),
                json!(guard),
            );
        }
    }
    Ok(json!({"type_ids": ids, "listeners": listeners}))
}
