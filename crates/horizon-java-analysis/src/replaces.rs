//! Every change the mods make to vanilla methods and every event they listen to, routed, with the per-mod summary: `replaces.json`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::Path;
use std::time::Instant;

use serde::ser::{Serialize, SerializeMap, Serializer};
use serde_json::Value as Json;

use crate::class::{self, ClassInfo, Event, Parse, is_vanilla, params, parse_class, python_value};
use crate::hierarchy::Hierarchy;
use crate::jars::{self, Archive};
use crate::knowledge::{self, Knowledge, NativeInfo, WEAK_STATUSES};
use crate::lambdas::{Impl, Vanilla, fold_lambdas, lambda_event, lambda_impl};
use crate::libraries::{entry_names, library_jars, load_library_index, spi_names};
use crate::mixins::{self, MixinEntry};

const ROUTING_ORDER: [&str; 6] = [
    "java",
    "java-own-type",
    "undecided",
    "java-already",
    "native-hook",
    "inert",
];
const MIXIN: &str = "Lorg/spongepowered/asm/mixin/Mixin;";
const SUBSCRIBE: &str = "Lnet/neoforged/bus/api/SubscribeEvent;";
const EVENT_BUS: &str = "net/neoforged/bus/api/IEventBus";
const EVENT_PRIORITY: &str = "net/neoforged/bus/api/EventPriority";
const ACC_PRIVATE: u16 = 0x2;
const ACC_STATIC: u16 = 0x8;
const ACC_ABSTRACT: u16 = 0x400;

/// One change to a vanilla method; the mechanism decides which fields it writes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Change {
    pub mod_id: String,
    pub mechanism: String,
    pub routing: String,
    pub note: Option<String>,
    pub kind: Option<String>,
    pub handler: Option<String>,
    pub desc: Option<String>,
    pub at: Vec<String>,
    pub side: Option<String>,
    pub conditional: Option<String>,
    pub folded: Option<String>,
    pub classes: Vec<String>,
    pub classes_n: usize,
    pub is_abstract: bool,
    pub extension: bool,
    pub scope: Option<String>,
}

impl Serialize for Change {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("mod", &self.mod_id)?;
        m.serialize_entry("mechanism", &self.mechanism)?;
        if matches!(self.mechanism.as_str(), "mixin" | "access") {
            m.serialize_entry("kind", &self.kind)?;
            m.serialize_entry("handler", &self.handler)?;
            m.serialize_entry("desc", &self.desc)?;
            m.serialize_entry("at", &self.at)?;
            m.serialize_entry("side", &self.side)?;
            m.serialize_entry("conditional", &self.conditional)?;
            if self.folded.is_some() {
                m.serialize_entry("folded", &self.folded)?;
            }
        } else {
            m.serialize_entry("desc", &self.desc)?;
            m.serialize_entry("classes", &self.classes)?;
            m.serialize_entry("classes_n", &self.classes_n)?;
            m.serialize_entry("abstract", &self.is_abstract)?;
            m.serialize_entry("extension", &self.extension)?;
            m.serialize_entry("scope", &self.scope)?;
        }
        m.serialize_entry("routing", &self.routing)?;
        if self.note.is_some() {
            m.serialize_entry("note", &self.note)?;
        }
        m.end()
    }
}

/// The changes to one vanilla method.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MethodRow {
    pub changes: Vec<Change>,
    pub vanilla: bool,
    pub native: NativeInfo,
    pub routing: String,
    pub mods: Vec<String>,
}

/// One listener of an event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listener {
    pub mod_id: String,
    pub via: String,
    pub class: String,
    pub method: String,
    pub priority: String,
    pub receive_canceled: bool,
    pub bus: Option<String>,
}

impl Serialize for Listener {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(8))?;
        m.serialize_entry("mod", &self.mod_id)?;
        m.serialize_entry("mechanism", "listener")?;
        m.serialize_entry("via", &self.via)?;
        m.serialize_entry("class", &self.class)?;
        m.serialize_entry("method", &self.method)?;
        m.serialize_entry("priority", &self.priority)?;
        m.serialize_entry("receive_canceled", &self.receive_canceled)?;
        m.serialize_entry("bus", &self.bus)?;
        m.end()
    }
}

/// The listeners of one event.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct EventRow {
    pub changes: Vec<Listener>,
    pub cancellable: Option<bool>,
    pub host_mentions: bool,
    pub routing: String,
    pub bus: Vec<String>,
    pub mods: Vec<String>,
}

/// A mod class the analysis read: the archive it is in, its entry and name, and whether it may listen to events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModClass {
    pub archive: usize,
    pub entry: String,
    pub name: String,
    pub marked: bool,
    pub listens: bool,
}

/// The data files one mod overrides of one kind.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DataCount {
    pub files: usize,
    pub samples: Vec<String>,
}

fn in_routing_order<S: Serializer>(rows: &[(String, usize)], s: S) -> Result<S::Ok, S::Error> {
    let mut m = s.serialize_map(Some(rows.len()))?;
    for (k, v) in rows {
        m.serialize_entry(k, v)?;
    }
    m.end()
}

/// The per-mod summary.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ModSummary {
    pub jars: Vec<String>,
    pub can_analyse_ahead: bool,
    pub opaque: Vec<String>,
    pub counts: BTreeMap<String, usize>,
    #[serde(serialize_with = "in_routing_order")]
    pub routing: Vec<(String, usize)>,
    pub unresolved_listeners: usize,
}

/// The whole result, with what SPEC-9 and SPEC-11 read on top of it.
pub struct Replaces {
    pub mods: BTreeMap<String, ModSummary>,
    pub methods: BTreeMap<String, MethodRow>,
    pub events: BTreeMap<String, EventRow>,
    pub data: BTreeMap<String, BTreeMap<String, DataCount>>,
    pub totals: BTreeMap<String, usize>,
    pub archives: Vec<Archive>,
    pub classes: Vec<ModClass>,
    pub entries: Vec<MixinEntry>,
    pub hierarchy: Hierarchy,
    pub mixin_classes: usize,
    pub warnings: Vec<String>,
}

/// An event listener a class declares or registers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundListener {
    pub event: Option<String>,
    pub via: String,
    pub method: String,
    pub priority: String,
    pub receive_canceled: bool,
    pub explicit_bus: Option<String>,
    pub implementation: Option<Impl>,
}

fn truthy(v: &Json) -> bool {
    match v {
        Json::Null => false,
        Json::Bool(b) => *b,
        Json::Number(n) => n.as_f64() != Some(0.0),
        Json::String(s) => !s.is_empty(),
        Json::Array(a) => !a.is_empty(),
        Json::Object(o) => !o.is_empty(),
    }
}

fn as_text(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The last annotation of a descriptor, as Python's dict keeps it.
fn anno_of<'a>(annos: &'a [class::Annotation], desc: &str) -> Option<&'a class::Annotation> {
    annos.iter().rev().find(|a| a.desc == desc)
}

fn anno_value(a: &class::Annotation, name: &str) -> Option<Json> {
    a.values
        .iter()
        .rev()
        .find(|(n, _)| n == name)
        .map(|(_, v)| python_value(v))
}

/// Port of mod_replaces.class_listeners.
pub fn class_listeners(info: &ClassInfo, hier: &Hierarchy) -> Vec<FoundListener> {
    let mut found = Vec::new();
    let subscriber = info
        .annos
        .iter()
        .find(|a| a.desc.ends_with("/EventBusSubscriber;"))
        .and_then(|first| anno_of(&info.annos, &first.desc));
    let explicit = subscriber
        .and_then(|s| anno_value(s, "bus"))
        .filter(truthy)
        .map(|v| as_text(&v).to_lowercase());
    for m in &info.methods {
        if let Some(sub) = anno_of(&m.annos, SUBSCRIBE)
            && let Some(event) = params(&m.desc).into_iter().next()
        {
            let via = if subscriber.is_some() {
                "subscriber-class"
            } else {
                "subscribe"
            };
            found.push(FoundListener {
                event: Some(event),
                via: via.to_owned(),
                method: m.name.clone(),
                priority: anno_value(sub, "priority")
                    .map_or_else(|| "NORMAL".to_owned(), |v| as_text(&v)),
                receive_canceled: anno_value(sub, "receiveCanceled").is_some_and(|v| truthy(&v)),
                explicit_bus: explicit.clone(),
                implementation: Some((
                    info.name.clone(),
                    m.name.clone(),
                    m.desc.clone(),
                    m.access & ACC_STATIC != 0,
                )),
            });
        }
        let (mut indy, mut cls, mut priority, mut implementation): (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<Impl>,
        ) = (None, None, None, None);
        for ev in &m.events {
            match ev {
                Event::Indy {
                    bootstrap, desc, ..
                } if desc.ends_with(")Ljava/util/function/Consumer;") => {
                    indy = lambda_event(info, usize::from(*bootstrap));
                    implementation = lambda_impl(info, usize::from(*bootstrap));
                }
                Event::Class(name) => cls = Some(name.clone()),
                Event::Static { owner, name } if owner == EVENT_PRIORITY => {
                    priority = Some(name.clone());
                }
                Event::Call { owner, name, desc }
                    if (name == "addListener" || name == "addGenericListener")
                        && !owner.is_empty()
                        && hier.is_a(owner, EVENT_BUS) =>
                {
                    let event = if desc.contains("Ljava/lang/Class;")
                        && name == "addListener"
                        && cls.is_some()
                    {
                        cls.take()
                    } else {
                        indy.take()
                    };
                    let priority_name = if desc.contains("EventPriority;") {
                        priority.take()
                    } else {
                        None
                    };
                    found.push(FoundListener {
                        event,
                        via: "addListener".to_owned(),
                        method: m.name.clone(),
                        priority: priority_name.unwrap_or_else(|| "NORMAL".to_owned()),
                        receive_canceled: false,
                        explicit_bus: None,
                        implementation: implementation.take(),
                    });
                    (indy, cls, priority, implementation) = (None, None, None, None);
                }
                _ => {}
            }
        }
    }
    found
}

/// Port of mod_replaces.split_target.
fn split_target(target: &str) -> (String, Option<String>) {
    let rest = match target.strip_prefix('L').and_then(|t| t.split_once(';')) {
        Some((owner, rest)) if !owner.is_empty() => rest,
        _ => target,
    };
    match rest.split_once('(') {
        Some((name, tail)) => (name.to_owned(), Some(format!("({tail}"))),
        None => (rest.to_owned(), None),
    }
}

/// Port of mod_replaces.mixin_records.
fn mixin_records(
    entries: &[MixinEntry],
    plugins: &BTreeMap<(String, String), String>,
    hier: &Hierarchy,
) -> Vec<(String, bool, Change)> {
    let mut out = Vec::new();
    for e in entries {
        let conditional = plugins.get(&(e.mod_id.clone(), e.config.clone()));
        for dotted in &e.target_class {
            let cls = dotted.replace('.', "/");
            let targets: Vec<(String, Option<String>)> = if e.access {
                vec![(e.handler.clone(), None)]
            } else if e.target_method.is_empty() {
                vec![("<unknown>".to_owned(), None)]
            } else {
                e.target_method.iter().map(|t| split_target(t)).collect()
            };
            for (name, desc) in targets {
                let owner = if e.access {
                    cls.clone()
                } else {
                    hier.declarer(&cls, &name, desc.as_deref())
                };
                let record = Change {
                    mod_id: e.mod_id.clone(),
                    mechanism: if e.access { "access" } else { "mixin" }.to_owned(),
                    kind: Some(e.kind.clone()),
                    handler: Some(format!("{}.{}", e.mixin, e.handler)),
                    desc,
                    at: e.at.clone(),
                    side: Some(e.side.clone()),
                    conditional: conditional.map(|p| format!("config plugin {p}")),
                    ..Change::default()
                };
                out.push((format!("{owner}#{name}"), is_vanilla(&owner), record));
            }
        }
    }
    out
}

/// Port of mod_replaces.override_records, in the order the class declares them: (key, descriptor) to (extension, abstract).
pub fn override_records(
    info: &ClassInfo,
    _mod_id: &str,
    hier: &Hierarchy,
) -> Vec<((String, String), (bool, bool))> {
    if info.annos.iter().any(|a| a.desc == MIXIN) {
        return Vec::new();
    }
    let supers = hier.vanilla_ancestors(&info.name);
    let mut found = Vec::new();
    for m in &info.methods {
        if m.name == "<init>" || m.name == "<clinit>" || m.access & (ACC_STATIC | ACC_PRIVATE) != 0
        {
            continue;
        }
        for ancestor in &supers {
            let hit = hier
                .methods_of(ancestor)
                .iter()
                .find(|v| v.0 == m.name && v.1 == m.desc);
            if let Some(hit) = hit {
                if hit.2 & (ACC_STATIC | ACC_PRIVATE) == 0 {
                    found.push((
                        (format!("{ancestor}#{}", m.name), m.desc.clone()),
                        (
                            ancestor.starts_with("net/neoforged/"),
                            hit.2 & ACC_ABSTRACT != 0,
                        ),
                    ));
                }
                break;
            }
        }
    }
    found
}

fn contains(data: &[u8], needle: &[u8]) -> bool {
    data.windows(needle.len()).any(|w| w == needle)
}

#[derive(Default)]
struct ModRow {
    jars: Vec<String>,
    opaque: Vec<String>,
    counts: BTreeMap<String, usize>,
    unresolved: usize,
    keys: BTreeMap<&'static str, BTreeSet<String>>,
}

#[derive(Default)]
struct Ordered<K: std::hash::Hash + Eq + Clone, V> {
    index: HashMap<K, usize>,
    items: Vec<(K, V)>,
}

impl<K: std::hash::Hash + Eq + Clone, V: Default> Ordered<K, V> {
    fn entry(&mut self, key: K) -> &mut V {
        let next = self.items.len();
        let i = *self.index.entry(key.clone()).or_insert(next);
        if i == next {
            self.items.push((key, V::default()));
        }
        &mut self.items[i].1
    }
}

type ListenerKey = (String, String, String, String, String, String, bool);

#[derive(Default)]
struct OverrideItem {
    classes: BTreeSet<String>,
    extension: bool,
    is_abstract: bool,
}

fn bump(counts: &mut BTreeMap<String, usize>, key: &str) {
    *counts.entry(key.to_owned()).or_insert(0) += 1;
}

/// Port of mod_replaces.analyse without its entity facts and routing: SPEC-9 and SPEC-11 run on the result.
pub fn analyse(
    run: &Path,
    knowledge: &Knowledge,
    cache: Option<&Path>,
    log: &mut dyn FnMut(&str),
) -> Result<Replaces, String> {
    analyse_with(run, knowledge, cache, None, log)
}

/// `analyse` with the mixin entries given instead of scanned.
pub fn analyse_with(
    run: &Path,
    knowledge: &Knowledge,
    cache: Option<&Path>,
    given: Option<Vec<MixinEntry>>,
    log: &mut dyn FnMut(&str),
) -> Result<Replaces, String> {
    let started = Instant::now();
    let library = library_jars(&run.join("libraries"))?;
    let mut hier = Hierarchy::new(load_library_index(&library, cache)?);
    hier.jars = library.clone();
    let spis = spi_names(&library)?;
    log(&format!(
        "libraries: {} classes, {:.1}s",
        hier.libs.len(),
        started.elapsed().as_secs_f64()
    ));

    let mut archives: Vec<Archive> = Vec::new();
    let mut classes: Vec<ModClass> = Vec::new();
    for jar in jars::mod_jars(run) {
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
                let listens =
                    contains(&data, b"addListener") || contains(&data, b"addGenericListener");
                classes.push(ModClass {
                    archive: index,
                    entry,
                    name: head.name,
                    marked: listens || contains(&data, b"SubscribeEvent"),
                    listens,
                });
            }
        }
    }
    log(&format!(
        "scanned {} mod classes in {} archives, {:.1}s",
        classes.len(),
        archives.len(),
        started.elapsed().as_secs_f64()
    ));

    let mut mods: BTreeMap<String, ModRow> = BTreeMap::new();
    let mut data_total: BTreeMap<String, BTreeMap<String, DataCount>> = BTreeMap::new();
    let mut plugins: BTreeMap<(String, String), String> = BTreeMap::new();
    for archive in &mut archives {
        let facts =
            jars::jar_facts(archive, &spis).map_err(|e| format!("{}: {e}", archive.label))?;
        let row = mods.entry(archive.mod_id.clone()).or_default();
        row.jars.push(archive.label.clone());
        row.opaque.extend(facts.opaque);
        if facts.at_lines > 0 {
            *row.counts
                .entry("access_transformer".to_owned())
                .or_insert(0) += facts.at_lines;
        }
        for (config, plugin) in facts.plugins {
            plugins.insert((archive.mod_id.clone(), config), plugin);
        }
        for (kind, mut files) in facts.data {
            *row.counts.entry("data".to_owned()).or_insert(0) += files.len();
            let slot = data_total
                .entry(kind)
                .or_default()
                .entry(archive.mod_id.clone())
                .or_insert(DataCount {
                    files: 0,
                    samples: Vec::new(),
                });
            slot.files += files.len();
            files.sort();
            files.truncate(3);
            slot.samples = files;
        }
    }

    let (entries, mixin_classes, warnings) = match given {
        Some(entries) => (entries, 0, Vec::new()),
        None => {
            let mut warnings = Vec::new();
            let (entries, count) = mixins::scan_install(run, &mut warnings)?;
            (entries, count, warnings)
        }
    };
    log(&format!(
        "mixin handlers: {}, {:.1}s",
        entries.len(),
        started.elapsed().as_secs_f64()
    ));

    let mut methods: BTreeMap<String, Vec<Change>> = BTreeMap::new();
    let mut events: BTreeMap<String, EventRow> = BTreeMap::new();
    let mut vanilla_of: HashMap<String, bool> = HashMap::new();
    let vanilla_classes = Vanilla::new(&hier.jars)?;
    for (key, vanilla, record) in fold_lambdas(
        mixin_records(&entries, &plugins, &hier),
        &hier,
        &vanilla_classes,
    ) {
        bump(
            &mut mods.entry(record.mod_id.clone()).or_default().counts,
            &record.mechanism,
        );
        vanilla_of.insert(key.clone(), vanilla);
        methods.entry(key).or_default().push(record);
    }

    let mut overrides: Ordered<(String, String, String), OverrideItem> = Ordered::default();
    let mut listeners: Ordered<ListenerKey, BTreeSet<Option<String>>> = Ordered::default();
    for class in &classes {
        let mod_id = archives[class.archive].mod_id.clone();
        if !class.marked && hier.vanilla_ancestors(&class.name).is_empty() {
            continue;
        }
        let data = jars::read_entry(&mut archives[class.archive].zip, &class.entry)?;
        let Ok(info) = parse_class(
            &data,
            Parse {
                members: true,
                annotations: true,
                code: class.listens,
            },
        ) else {
            continue;
        };
        let dotted = class.name.replace('/', ".");
        for ((key, desc), (extension, is_abstract)) in override_records(&info, &mod_id, &hier) {
            let item = overrides.entry((mod_id.clone(), key, desc));
            item.classes.insert(dotted.clone());
            item.extension = extension;
            item.is_abstract = is_abstract;
        }
        for found in class_listeners(&info, &hier) {
            let Some(event) = found.event else {
                mods.entry(mod_id.clone()).or_default().unresolved += 1;
                continue;
            };
            let facts = hier.event_facts(&event);
            listeners
                .entry((
                    mod_id.clone(),
                    event,
                    found.via,
                    dotted.clone(),
                    found.method,
                    found.priority,
                    found.receive_canceled,
                ))
                .insert(found.explicit_bus.or_else(|| facts.bus.map(str::to_owned)));
        }
    }
    for ((mod_id, key, desc), item) in overrides.items {
        let classes_list: Vec<String> = item.classes.into_iter().collect();
        let mechanism = if item.is_abstract {
            "implement"
        } else {
            "override"
        };
        methods.entry(key.clone()).or_default().push(Change {
            mod_id: mod_id.clone(),
            mechanism: mechanism.to_owned(),
            desc: Some(desc),
            classes: classes_list.iter().take(10).cloned().collect(),
            classes_n: classes_list.len(),
            is_abstract: item.is_abstract,
            extension: item.extension,
            scope: Some("own-type".to_owned()),
            ..Change::default()
        });
        vanilla_of.insert(key, true);
        bump(&mut mods.entry(mod_id).or_default().counts, mechanism);
    }
    for ((mod_id, event, via, class, method, priority, receive_canceled), buses) in listeners.items
    {
        let facts = hier.event_facts(&event);
        let bus = buses.into_iter().flatten().find(|b| !b.is_empty());
        let simple = event.rsplit('/').next().unwrap_or(&event);
        let row = events.entry(event.clone()).or_default();
        row.cancellable = facts.cancellable;
        row.host_mentions = knowledge
            .host_words
            .contains(simple.split('$').next().unwrap_or(simple));
        row.changes.push(Listener {
            mod_id: mod_id.clone(),
            via,
            class,
            method,
            priority,
            receive_canceled,
            bus,
        });
        bump(&mut mods.entry(mod_id).or_default().counts, "listener");
    }

    let mut totals: BTreeMap<String, usize> = BTreeMap::new();
    let mut rows: BTreeMap<String, MethodRow> = BTreeMap::new();
    for (key, mut changes) in methods {
        let vanilla = vanilla_of.get(&key).copied().unwrap_or(true);
        let native = if vanilla {
            knowledge.info(&key)
        } else {
            NativeInfo {
                status: "not-vanilla".to_owned(),
                owned: false,
                fns: Vec::new(),
                note: String::new(),
            }
        };
        for change in &mut changes {
            change.routing =
                knowledge::route(&change.mechanism, &native, &key, knowledge).to_owned();
            if change.mechanism == "mixin"
                && change.routing == "java-already"
                && WEAK_STATUSES.contains(&native.status.as_str())
                && knowledge.verdict_of(&key) == Some("missing")
            {
                change.note = Some("native lacks it".to_owned());
            }
            bump(&mut totals, &change.mechanism);
        }
        let routing = knowledge::top_routing(
            &changes
                .iter()
                .map(|c| c.routing.as_str())
                .collect::<Vec<_>>(),
        );
        let involved: BTreeSet<String> = changes.iter().map(|c| c.mod_id.clone()).collect();
        bump(&mut totals, &format!("routing:{routing}"));
        for c in &changes {
            let slot = &mut mods.entry(c.mod_id.clone()).or_default().keys;
            slot.entry(
                ROUTING_ORDER[ROUTING_ORDER
                    .iter()
                    .position(|r| *r == c.routing)
                    .unwrap_or(0)],
            )
            .or_default()
            .insert(key.clone());
        }
        rows.insert(
            key,
            MethodRow {
                changes,
                vanilla,
                native,
                routing: routing.to_owned(),
                mods: involved.into_iter().collect(),
            },
        );
    }
    for (event, row) in &mut events {
        "native-hook".clone_into(&mut row.routing);
        row.bus = row
            .changes
            .iter()
            .filter_map(|c| c.bus.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let involved: BTreeSet<String> = row.changes.iter().map(|c| c.mod_id.clone()).collect();
        row.mods = involved.into_iter().collect();
        *totals.entry("listener".to_owned()).or_insert(0) += row.changes.len();
        bump(&mut totals, "routing:native-hook");
        for c in &row.changes {
            mods.entry(c.mod_id.clone())
                .or_default()
                .keys
                .entry("native-hook")
                .or_default()
                .insert(format!("event:{event}"));
        }
    }
    for counted in ["data", "access_transformer"] {
        let sum = mods
            .values()
            .map(|r| r.counts.get(counted).copied().unwrap_or(0))
            .sum();
        totals.insert(counted.to_owned(), sum);
    }

    let summary = mods
        .into_iter()
        .map(|(mod_id, row)| {
            let routing = ROUTING_ORDER
                .iter()
                .filter_map(|r| {
                    row.keys
                        .get(r)
                        .filter(|k| !k.is_empty())
                        .map(|k| ((*r).to_owned(), k.len()))
                })
                .collect();
            let opaque: BTreeSet<String> = row.opaque.iter().cloned().collect();
            let summary = ModSummary {
                jars: row.jars,
                can_analyse_ahead: row.opaque.is_empty(),
                opaque: opaque.into_iter().collect(),
                counts: row.counts,
                routing,
                unresolved_listeners: row.unresolved,
            };
            (mod_id, summary)
        })
        .collect();
    Ok(Replaces {
        mods: summary,
        methods: rows,
        events,
        data: data_total,
        totals,
        archives,
        classes,
        entries,
        hierarchy: hier,
        mixin_classes,
        warnings,
    })
}

#[derive(serde::Serialize)]
struct Doc<'a> {
    format: u32,
    install: String,
    mods: &'a BTreeMap<String, ModSummary>,
    methods: &'a BTreeMap<String, MethodRow>,
    events: &'a BTreeMap<String, EventRow>,
    data: &'a BTreeMap<String, BTreeMap<String, DataCount>>,
    totals: &'a BTreeMap<String, usize>,
    seconds: f64,
}

/// The result as Python's `json.dumps(result, indent=1)` writes it.
pub fn replaces_json(r: &Replaces, install: &Path, seconds: f64) -> Result<String, String> {
    let doc = Doc {
        format: 1,
        install: install.display().to_string(),
        mods: &r.mods,
        methods: &r.methods,
        events: &r.events,
        data: &r.data,
        totals: &r.totals,
        seconds: (seconds * 10.0).round() / 10.0,
    };
    python_json(&doc)
}

/// A value as Python's `json.dumps(value, indent=1)` writes it, with a final newline.
pub fn python_json<T: Serialize>(doc: &T) -> Result<String, String> {
    let mut bytes = Vec::new();
    let format = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut ser = serde_json::Serializer::with_formatter(&mut bytes, format);
    doc.serialize(&mut ser).map_err(|e| e.to_string())?;
    let text = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    let mut out = String::with_capacity(text.len() + 1);
    for c in text.chars() {
        if (' '..='~').contains(&c) || c == '\n' {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                let _ = write!(out, "\\u{unit:04x}");
            }
        }
    }
    out.push('\n');
    Ok(out)
}

/// Writes `out/replaces.json`.
pub fn write_replaces(
    out: &Path,
    r: &Replaces,
    install: &Path,
    seconds: f64,
) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join("replaces.json");
    std::fs::write(&path, replaces_json(r, install, seconds)?)
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_descriptor_in_a_target_is_split_off() {
        assert_eq!(
            split_target("Lnet/minecraft/world/Base;must()V"),
            ("must".to_owned(), Some("()V".to_owned()))
        );
        assert_eq!(split_target("tick"), ("tick".to_owned(), None));
        assert_eq!(
            split_target("tick(I)V"),
            ("tick".to_owned(), Some("(I)V".to_owned()))
        );
        assert_eq!(split_target("Lx;"), (String::new(), None));
    }
}
