//! The rest of the entity facts: writers, capabilities, routes, NeoForge's hooks and `entity-facts.json`.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::LazyLock;

use serde_json::{Value as Json, json};
use zip::ZipArchive;

use super::bytecode::{
    CHECKCAST, GETSTATIC, INVOKEINTERFACE, INVOKEVIRTUAL, Instruction, LDC, LDC_W, instructions,
    is_aload,
};
use super::groups::{STATE_GROUPS, UNCARRIED, carried, classify};
use super::guards::{
    ENTITY_EVENT, EVENT_PREFIXES, Guard, TYPES_CLASS, listener_guards, subjects, type_ids,
};
use crate::class::{
    ClassError, ClassInfo, Event, Method, Parse, Pool, PoolEntry, is_extension, is_vanilla, params,
    parse_class, python_value,
};
use crate::hierarchy::Hierarchy;
use crate::jars::{self, Archive};
use crate::lambdas::lambda_impl;
use crate::libraries::entry_names;
use crate::replaces::{Change, MethodRow, Replaces, class_listeners, override_records};

pub const ENTITY: &str = "net/minecraft/world/entity/Entity";
pub const PLAYER: &str = "net/minecraft/world/entity/player/Player";
pub const CAPS_EVENT: &str = "net/neoforged/neoforge/capabilities/RegisterCapabilitiesEvent";
const CLIENT_ROUTES: [&str; 2] = [
    "net/minecraft/client/renderer/entity/",
    "net/minecraft/client/model/",
];
const EDGE_AT: [&str; 3] = ["HEAD", "RETURN", "TAIL"];
const CALLBACKS: [&str; 2] = [
    "org/spongepowered/asm/mixin/injection/callback/CallbackInfo",
    "org/spongepowered/asm/mixin/injection/callback/CallbackInfoReturnable",
];
const INJECT: &str = "Lorg/spongepowered/asm/mixin/injection/Inject;";
const REFLECTION: [&str; 2] = ["java/lang/reflect/", "java/lang/invoke/MethodHandles"];
const ACC_STATIC: u16 = 0x8;
const NEW: u8 = 0xbb;
const GETFIELD: u8 = 0xb4;
const INVOKESPECIAL: u8 = 0xb7;
const INVOKESTATIC: u8 = 0xb8;
const INVOKEDYNAMIC: u8 = 0xba;
const ACONST_NULL: u8 = 0x01;
const FOLLOWED: &str = "net/neoforged/";
const NOT_FOLLOWED: &str = "net/neoforged/bus/";
const HOOKS_TSV: &str = include_str!("../../data/entity_hooks.tsv");

/// Port of mod_entity_facts.HOOKS, in its order: (hook, NeoForge method key).
pub static HOOKS: LazyLock<Vec<(String, String)>> = LazyLock::new(|| {
    HOOKS_TSV
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
        .map(|(h, k)| (h.to_owned(), k.to_owned()))
        .collect()
});

fn contains(data: &[u8], needle: &[u8]) -> bool {
    data.windows(needle.len()).any(|w| w == needle)
}

fn text(e: ClassError) -> String {
    e.to_string()
}

fn index(operand: Option<i64>) -> u16 {
    operand.unwrap_or(0) as u16
}

/// Python's truthiness of an annotation value.
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

/// Port of mod_entity_facts.entity_receiver.
pub fn entity_receiver(hier: &Hierarchy, owner: &str) -> bool {
    !owner.is_empty() && hier.is_a(owner, ENTITY) && !hier.is_a(owner, PLAYER)
}

/// Port of mod_entity_facts.entity_key.
pub fn entity_key(hier: &Hierarchy, owner: &str, name: &str, desc: &str) -> Option<String> {
    let mut declarer = hier.declarer(owner, name, Some(desc));
    if !is_vanilla(&declarer) && !is_extension(&declarer) {
        if !is_vanilla(owner) {
            return None;
        }
        owner.clone_into(&mut declarer);
    }
    if hier
        .methods_of(&declarer)
        .iter()
        .any(|m| m.0 == name && m.1 == desc && m.2 & ACC_STATIC != 0)
    {
        return None;
    }
    Some(format!("{declarer}#{name}{desc}"))
}

/// Port of mod_entity_facts.producers_of_call.
pub fn producers_of_call(
    pool: &Pool,
    ins: &[Instruction],
    at: usize,
) -> Result<Option<Vec<Instruction>>, ClassError> {
    let desc = pool.reference(index(ins[at].operand))?.2;
    let mut needs: Vec<bool> = vec![true; params(&desc).len() + 1];
    let mut found = Vec::new();
    let mut i = at;
    while !needs.is_empty() && i > 0 {
        i -= 1;
        let here = ins[i];
        let o = here.op;
        let (pops, pushes) = if is_aload(o) || matches!(o, GETSTATIC | LDC | LDC_W | ACONST_NULL) {
            (0, 1)
        } else if o == CHECKCAST || o == GETFIELD {
            (1, 1)
        } else if o == INVOKEDYNAMIC {
            let nat = match pool.entry(usize::from(index(here.operand)))? {
                PoolEntry::Dynamic { nat, .. } => *nat,
                other => return Err(ClassError(format!("invokedynamic constant is {other:?}"))),
            };
            (params(&pool.nat(nat)?.1).len(), 1)
        } else if matches!(
            o,
            INVOKEVIRTUAL | INVOKESTATIC | INVOKEINTERFACE | INVOKESPECIAL
        ) {
            let cdesc = pool.reference(index(here.operand))?.2;
            let pops = params(&cdesc).len() + usize::from(o != INVOKESTATIC);
            (pops, usize::from(!cdesc.ends_with(")V")))
        } else {
            return Ok(None);
        };
        if pushes == 0 {
            return Ok(None);
        }
        if needs.pop() == Some(true) {
            found.push(here);
        }
        needs.extend(std::iter::repeat_n(false, pops));
    }
    if !needs.is_empty() {
        return Ok(None);
    }
    found.reverse();
    Ok(Some(found))
}

/// Port of mod_entity_facts.capability_registrations.
pub fn capability_registrations(
    method: &Method,
    pool: &Pool,
    ids: &BTreeMap<String, String>,
) -> Result<Vec<Option<(String, String)>>, ClassError> {
    let ins = instructions(method.code.as_deref().unwrap_or_default())?;
    let mut out = Vec::new();
    for (at, i) in ins.iter().enumerate() {
        if i.op != INVOKEVIRTUAL && i.op != INVOKEINTERFACE {
            continue;
        }
        let (owner, name, _) = pool.reference(index(i.operand))?;
        if owner != CAPS_EVENT || name != "registerEntity" {
            continue;
        }
        let args = match producers_of_call(pool, &ins, at)? {
            Some(a) if a.len() >= 3 => a,
            _ => {
                out.push(None);
                continue;
            }
        };
        let (cap, kind) = (args[1], args[2]);
        if cap.op != GETSTATIC || kind.op != GETSTATIC {
            out.push(None);
            continue;
        }
        let (kowner, kfield, _) = pool.reference(index(kind.operand))?;
        let (cowner, cfield, _) = pool.reference(index(cap.operand))?;
        out.push(match ids.get(&kfield) {
            Some(id) if kowner == TYPES_CLASS => Some((id.clone(), format!("{cowner}.{cfield}"))),
            _ => None,
        });
    }
    Ok(out)
}

/// Port of mod_entity_facts.library_target: `None` stands for no descriptor.
pub fn library_target(
    hier: &Hierarchy,
    owner: &str,
    name: &str,
    desc: Option<&str>,
    handler_args: Option<&[String]>,
) -> Vec<Option<String>> {
    if let Some(d) = desc.filter(|d| !d.is_empty()) {
        return vec![Some(d.to_owned())];
    }
    let overloads: Vec<String> = hier
        .methods_of(owner)
        .iter()
        .filter(|m| m.0 == name)
        .map(|m| m.1.clone())
        .collect();
    let fitting: Vec<Option<String>> = overloads
        .iter()
        .filter(|d| handler_args.is_some_and(|a| params(d) == a))
        .map(|d| Some(d.clone()))
        .collect();
    if !fitting.is_empty() {
        fitting
    } else if !overloads.is_empty() {
        overloads.into_iter().map(Some).collect()
    } else {
        vec![None]
    }
}

/// The mods' archives and NeoForge's own, read through one borrow.
pub struct Archives<'a> {
    all: RefCell<Vec<&'a mut Archive>>,
}

impl<'a> Archives<'a> {
    pub fn new(all: Vec<&'a mut Archive>) -> Archives<'a> {
        Archives {
            all: RefCell::new(all),
        }
    }

    fn mod_id(&self, ai: usize) -> String {
        self.all.borrow()[ai].mod_id.clone()
    }

    fn read(&self, ai: usize, entry: &str) -> Result<Vec<u8>, String> {
        jars::read_entry(&mut self.all.borrow_mut()[ai].zip, entry)
    }
}

/// Port of mod_entity_facts.handler_method.
pub fn handler_method(
    archives: &Archives,
    mod_id: &str,
    mixin: &str,
    handler: &str,
) -> Result<Vec<Method>, String> {
    let entry = format!("{}.class", mixin.replace('.', "/"));
    let count = archives.all.borrow().len();
    for ai in 0..count {
        let holds = {
            let all = archives.all.borrow();
            all[ai].mod_id == mod_id && all[ai].zip.index_for_name(&entry).is_some()
        };
        if holds {
            let what = Parse {
                members: true,
                annotations: true,
                code: false,
            };
            let info = parse_class(&archives.read(ai, &entry)?, what)
                .map_err(|e| format!("{entry}: {e}"))?;
            return Ok(info
                .methods
                .into_iter()
                .filter(|m| m.name == handler)
                .collect());
        }
    }
    Ok(Vec::new())
}

fn cancellable(method: &Method) -> bool {
    method
        .annos
        .iter()
        .rev()
        .find(|a| a.desc == INJECT)
        .and_then(|a| a.values.iter().rev().find(|(n, _)| n == "cancellable"))
        .is_some_and(|(_, v)| truthy(&python_value(v)))
}

#[derive(Default)]
struct ClassRoutes {
    edge: BTreeMap<String, Vec<Json>>,
    body: BTreeMap<String, Vec<(bool, Option<String>, Json)>>,
}

/// Port of mod_entity_facts.route_facts.
pub fn route_facts(
    archives: &Archives,
    hier: &Hierarchy,
    methods: &BTreeMap<String, MethodRow>,
) -> Result<Json, String> {
    let mut routes: BTreeMap<String, ClassRoutes> = BTreeMap::new();
    for (key, row) in methods {
        let (owner, name) = key.split_once('#').unwrap_or((key, ""));
        if !(hier.is_a(owner, ENTITY) || CLIENT_ROUTES.iter().any(|p| owner.starts_with(p)))
            || !row.vanilla
            || name == "<unknown>"
        {
            continue;
        }
        for change in &row.changes {
            if change.mechanism != "mixin" {
                continue;
            }
            route_change(archives, hier, owner, name, change, &mut routes)?;
        }
    }
    let mut out = serde_json::Map::new();
    for (cls, r) in routes {
        let conditional: serde_json::Map<String, Json> = r
            .body
            .iter()
            .filter(|(_, v)| {
                v.iter()
                    .all(|(c, side, _)| *c || side.as_deref() != Some("common"))
            })
            .map(|(k, v)| {
                (
                    k.clone(),
                    Json::Array(v.iter().map(|h| h.2.clone()).collect()),
                )
            })
            .collect();
        out.insert(
            cls,
            json!({
                "edge": r.edge,
                "body": r.body.keys().collect::<Vec<_>>(),
                "body_conditional": conditional,
            }),
        );
    }
    Ok(Json::Object(out))
}

fn route_change(
    archives: &Archives,
    hier: &Hierarchy,
    owner: &str,
    name: &str,
    change: &Change,
    routes: &mut BTreeMap<String, ClassRoutes>,
) -> Result<(), String> {
    let full = change.handler.as_deref().unwrap_or_default();
    let (mixin, handler) = full.rsplit_once('.').unwrap_or(("", full));
    let found = handler_method(archives, &change.mod_id, mixin, handler)?;
    let mut args: Option<Vec<String>> = None;
    let mut edge = change.kind.as_deref() == Some("inject")
        && !change.at.is_empty()
        && change.at.iter().all(|a| EDGE_AT.contains(&a.as_str()))
        && found.len() == 1;
    if edge {
        let hp = params(&found[0].desc);
        edge = hp.last().is_some_and(|l| CALLBACKS.contains(&l.as_str()));
        args = edge.then(|| hp[..hp.len() - 1].to_vec());
    }
    let keys: Vec<String> =
        library_target(hier, owner, name, change.desc.as_deref(), args.as_deref())
            .into_iter()
            .map(|d| format!("{owner}#{name}{}", d.unwrap_or_default()))
            .collect();
    let conditional = change.conditional.as_deref().is_some_and(|c| !c.is_empty());
    let side = &change.side;
    let slot = routes.entry(owner.to_owned()).or_default();
    if edge {
        let cancellable = cancellable(&found[0]);
        for at in &change.at {
            slot.edge.entry(keys[0].clone()).or_default().push(json!({
                "mixin": mixin,
                "handler": handler,
                "at": at,
                "cancellable": cancellable,
                "conditional": conditional,
                "side": side,
            }));
        }
    } else {
        for k in keys {
            let row = json!({"mixin": mixin, "handler": handler, "conditional": conditional, "side": side});
            slot.body
                .entry(k)
                .or_default()
                .push((conditional, side.clone(), row));
        }
    }
    Ok(())
}

/// The library jars' NeoForge classes, each from the first jar holding it, parsed once.
pub struct LibraryClasses {
    zips: Vec<RefCell<ZipArchive<File>>>,
    index: HashMap<String, (usize, String)>,
    cache: RefCell<HashMap<String, Option<Rc<ClassInfo>>>>,
}

impl LibraryClasses {
    pub fn new(jars: &[PathBuf]) -> Result<LibraryClasses, String> {
        let mut zips = Vec::new();
        let mut index = HashMap::new();
        for jar in jars {
            let file = File::open(jar).map_err(|e| format!("{}: {e}", jar.display()))?;
            let mut zip = ZipArchive::new(file).map_err(|e| format!("{}: {e}", jar.display()))?;
            for entry in entry_names(&mut zip)? {
                if let Some(name) = entry.strip_suffix(".class")
                    && name.starts_with(FOLLOWED)
                {
                    index
                        .entry(name.to_owned())
                        .or_insert_with(|| (zips.len(), entry.clone()));
                }
            }
            zips.push(RefCell::new(zip));
        }
        Ok(LibraryClasses {
            zips,
            index,
            cache: RefCell::new(HashMap::new()),
        })
    }

    /// Port of hook_closure.parsed: a class that fails to parse is None.
    fn parsed(&self, cls: &str) -> Option<Rc<ClassInfo>> {
        if let Some(found) = self.cache.borrow().get(cls) {
            return found.clone();
        }
        let parsed = self.index.get(cls).and_then(|(zip, entry)| {
            let mut data = Vec::new();
            self.zips[*zip]
                .borrow_mut()
                .by_name(entry)
                .ok()?
                .read_to_end(&mut data)
                .ok()?;
            let what = Parse {
                members: true,
                annotations: false,
                code: true,
            };
            parse_class(&data, what).ok().map(Rc::new)
        });
        self.cache
            .borrow_mut()
            .insert(cls.to_owned(), parsed.clone());
        parsed
    }
}

struct HookWalk<'a> {
    hier: &'a Hierarchy,
    lib: &'a LibraryClasses,
    events: &'a HashSet<String>,
    unclassified: &'a mut BTreeMap<String, String>,
    seen: HashSet<(String, String, String)>,
    reads: BTreeSet<&'static str>,
    writes: BTreeSet<(String, String, String)>,
    posted: BTreeSet<String>,
}

impl HookWalk<'_> {
    fn find(&self, owner: &str, name: &str, desc: &str) -> Option<(Rc<ClassInfo>, usize)> {
        let chain = std::iter::once(owner.to_owned()).chain(self.hier.ancestors(owner));
        for cls in chain {
            if !cls.starts_with(FOLLOWED) || cls.starts_with(NOT_FOLLOWED) {
                continue;
            }
            let Some(info) = self.lib.parsed(&cls) else {
                continue;
            };
            if let Some(i) = info
                .methods
                .iter()
                .position(|m| m.name == name && m.desc == desc && m.code.is_some())
            {
                return Some((info, i));
            }
        }
        None
    }

    fn visit(&mut self, owner: &str, name: &str, desc: &str) -> Result<bool, String> {
        if !self
            .seen
            .insert((owner.to_owned(), name.to_owned(), desc.to_owned()))
        {
            return Ok(false);
        }
        let Some((info, at)) = self.find(owner, name, desc) else {
            return Ok(false);
        };
        let pool = &info.pool;
        let code = info.methods[at].code.as_deref().unwrap_or_default();
        for i in instructions(code).map_err(text)? {
            let op = i.op;
            if op == NEW {
                if let Some(cls) = pool.cls(index(i.operand)).map_err(text)?
                    && self.events.contains(&cls)
                {
                    self.posted.insert(cls);
                }
            } else if op == INVOKEDYNAMIC {
                let bootstrap = match pool.entry(usize::from(index(i.operand))).map_err(text)? {
                    PoolEntry::Dynamic { bootstrap, .. } => usize::from(*bootstrap),
                    _ => continue,
                };
                if let Some((c, n, d, _)) = lambda_impl(&info, bootstrap) {
                    self.visit(&c, &n, &d)?;
                }
            } else if matches!(
                op,
                INVOKEVIRTUAL | INVOKESPECIAL | INVOKESTATIC | INVOKEINTERFACE
            ) {
                let (callee, cname, cdesc) = pool.reference(index(i.operand)).map_err(text)?;
                if entity_receiver(self.hier, &callee) {
                    let Some(ekey) = entity_key(self.hier, &callee, &cname, &cdesc) else {
                        continue;
                    };
                    if cname.starts_with('<') {
                        continue;
                    }
                    let declarer = ekey.split('#').next().unwrap_or_default();
                    match classify(declarer, &cname) {
                        None => {
                            self.unclassified
                                .insert(format!("{declarer}#{cname}"), "neoforge".to_owned());
                        }
                        Some((group, write)) => {
                            if !UNCARRIED.contains(&group) {
                                self.reads.extend(carried(group));
                            }
                            if write {
                                self.writes.insert((
                                    ekey.clone(),
                                    callee.clone(),
                                    group.to_owned(),
                                ));
                            }
                        }
                    }
                } else if callee.starts_with(FOLLOWED) && !callee.starts_with(NOT_FOLLOWED) {
                    self.visit(&callee, &cname, &cdesc)?;
                }
            }
        }
        Ok(true)
    }
}

/// Port of mod_entity_facts.hook_closure: `events` is the set of event classes a `new` may name.
pub fn hook_closure(
    hier: &Hierarchy,
    lib: &LibraryClasses,
    key: &str,
    events: &HashSet<String>,
    unclassified: &mut BTreeMap<String, String>,
) -> Result<Option<Json>, String> {
    let mut walk = HookWalk {
        hier,
        lib,
        events,
        unclassified,
        seen: HashSet::new(),
        reads: BTreeSet::new(),
        writes: BTreeSet::new(),
        posted: BTreeSet::new(),
    };
    let (owner, rest) = key.split_once('#').unwrap_or((key, ""));
    let (name, tail) = rest.split_once('(').unwrap_or((rest, ""));
    if !walk.visit(owner, name, &format!("({tail}"))? {
        return Ok(None);
    }
    let mut reads: Vec<&str> = walk.reads.into_iter().collect();
    reads.sort_by_key(|g| STATE_GROUPS.iter().position(|s| s == g));
    let writes: Vec<Json> = walk
        .writes
        .into_iter()
        .map(|(k, r, g)| json!({"key": k, "receiver": r, "group": g}))
        .collect();
    Ok(Some(
        json!({"reads": reads, "writes": writes, "events": walk.posted}),
    ))
}

/// Port of mod_entity_facts.loader_archive: NeoForge's jar and its entries that register listeners.
pub fn loader_archive(
    jars: &[PathBuf],
) -> Result<Option<(Archive, Vec<(String, Vec<u8>)>)>, String> {
    for jar in jars {
        let file = jar
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !(file.starts_with("neoforge-") && file.ends_with("-universal.jar")) {
            continue;
        }
        let bytes = std::fs::read(jar).map_err(|e| format!("{}: {e}", jar.display()))?;
        let mut zip =
            ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("{}: {e}", jar.display()))?;
        let mut found = Vec::new();
        for entry in entry_names(&mut zip)? {
            if entry.ends_with(".class") && !entry.starts_with("META-INF/") {
                let data = jars::read_entry(&mut zip, &entry)?;
                if contains(&data, b"SubscribeEvent") || contains(&data, b"addListener") {
                    found.push((entry, data));
                }
            }
        }
        let archive = Archive {
            mod_id: "neoforge".to_owned(),
            label: "neoforge".to_owned(),
            zip,
            toml: toml::Table::new(),
        };
        return Ok(Some((archive, found)));
    }
    Ok(None)
}

/// Port of mod_entity_facts.event_classes.
pub fn event_classes(hier: &Hierarchy) -> Vec<String> {
    hier.heads()
        .filter(|c| {
            !is_vanilla(c)
                && (EVENT_PREFIXES.iter().any(|p| c.starts_with(p)) || hier.is_a(c, ENTITY_EVENT))
        })
        .map(str::to_owned)
        .collect()
}

/// One listener of an entity event.
#[derive(Debug, Clone)]
struct EventListener {
    mod_id: String,
    class: String,
    method: String,
    guards: Guard,
}

type Events = (Vec<(String, Vec<EventListener>)>, HashMap<String, usize>);

fn event_slot<'e>(events: &'e mut Events, event: &str) -> &'e mut Vec<EventListener> {
    let next = events.0.len();
    let i = *events.1.entry(event.to_owned()).or_insert(next);
    if i == next {
        events.0.push((event.to_owned(), Vec::new()));
    }
    &mut events.0[i].1
}

/// Port of mod_entity_facts.inherit.
fn inherit(events: &mut Events, hier: &Hierarchy, candidates: &[String]) {
    let snapshot: Vec<String> = events.0.iter().map(|(e, _)| e.clone()).collect();
    for event in snapshot {
        let listeners = event_slot(events, &event).clone();
        for below in candidates {
            if *below != event && hier.is_a(below, &event) {
                event_slot(events, below).extend(listeners.iter().cloned());
            }
        }
    }
}

/// Port of mod_entity_facts.analyse: entity-facts.json for the classes and mixin entries of SPEC-8's result.
pub fn analyse(r: &mut Replaces) -> Result<Json, String> {
    analyse_hooks(r, &HOOKS)
}

/// `analyse` with the given HOOKS lines in place of the data file's.
pub fn analyse_hooks(r: &mut Replaces, hooks_table: &[(String, String)]) -> Result<Json, String> {
    let hier = &r.hierarchy;
    let ids = type_ids(&hier.jars)?;
    let mods_n = r.archives.len();
    let loader = loader_archive(&hier.jars)?;
    let mut classes: Vec<(usize, String, String, bool)> = r
        .classes
        .iter()
        .map(|c| (c.archive, c.entry.clone(), c.name.clone(), c.marked))
        .collect();
    let mut loader_zip = None;
    if let Some((archive, found)) = loader {
        for (entry, data) in found {
            let head = parse_class(
                &data,
                Parse {
                    members: false,
                    annotations: false,
                    code: false,
                },
            )
            .map_err(|e| format!("{entry}: {e}"))?;
            classes.push((mods_n, entry, head.name, true));
        }
        loader_zip = Some(archive);
    }
    let mut all: Vec<&mut Archive> = r.archives.iter_mut().collect();
    all.extend(loader_zip.as_mut());
    let archives = Archives::new(all);
    let by_name: HashMap<&str, (usize, &str)> = classes
        .iter()
        .map(|(a, entry, name, _)| (name.as_str(), (*a, entry.as_str())))
        .collect();
    let cache: RefCell<HashMap<String, Option<Rc<ClassInfo>>>> = RefCell::new(HashMap::new());
    let infos = |name: &str| -> Option<Rc<ClassInfo>> {
        if let Some(found) = cache.borrow().get(name) {
            return found.clone();
        }
        let parsed = by_name.get(name).and_then(|(a, entry)| {
            let data = archives.read(*a, entry).ok()?;
            parse_class(&data, Parse::all()).ok().map(Rc::new)
        });
        cache.borrow_mut().insert(name.to_owned(), parsed.clone());
        parsed
    };

    let mut events: Events = (Vec::new(), HashMap::new());
    let mut unresolved: BTreeMap<String, usize> = BTreeMap::new();
    let mut reads: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
    let mut writers: BTreeMap<(String, String, String), BTreeSet<String>> = BTreeMap::new();
    let mut unclassified: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut caps: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut caps_unresolved: BTreeMap<String, usize> = BTreeMap::new();
    let mut reflective: BTreeMap<String, usize> = BTreeMap::new();
    let mut overrides: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (ai, entry, name, marked) in &classes {
        let mod_id = archives.mod_id(*ai);
        let raw = archives.read(*ai, entry)?;
        let listening_only = *ai >= mods_n;
        let entity_class = !hier.vanilla_ancestors(name).is_empty() && hier.is_a(name, ENTITY);
        let names_caps = contains(&raw, CAPS_EVENT.as_bytes());
        if !(*marked
            || entity_class
            || contains(&raw, b"net/minecraft/world/entity/")
            || names_caps)
        {
            continue;
        }
        let Some(info) = infos(name) else {
            continue;
        };
        if *marked {
            for found in class_listeners(&info, hier) {
                let Some(event) = found.event else {
                    continue;
                };
                if !(EVENT_PREFIXES.iter().any(|p| event.starts_with(p))
                    || hier.is_a(&event, ENTITY_EVENT))
                {
                    continue;
                }
                let guards = match listener_guards(&infos, found.implementation.as_ref(), &ids)
                    .map_err(text)?
                {
                    Some(g) => g,
                    None => {
                        *unresolved.entry(mod_id.clone()).or_insert(0) += 1;
                        Guard::all()
                    }
                };
                event_slot(&mut events, &event).push(EventListener {
                    mod_id: mod_id.clone(),
                    class: name.replace('/', "."),
                    method: found.method,
                    guards,
                });
            }
        }
        if listening_only {
            continue;
        }
        let mut touches_entity = false;
        for m in &info.methods {
            for ev in &m.events {
                if let Event::Call { owner, .. } | Event::Static { owner, .. } = ev
                    && !owner.is_empty()
                    && hier.is_a(owner, ENTITY)
                {
                    touches_entity = true;
                }
                let Event::Call {
                    owner,
                    name: mname,
                    desc,
                } = ev
                else {
                    continue;
                };
                if !entity_receiver(hier, owner) {
                    continue;
                }
                let Some(key) = entity_key(hier, owner, mname, desc) else {
                    continue;
                };
                if mname.starts_with('<') {
                    continue;
                }
                let declarer = key.split('#').next().unwrap_or_default();
                match classify(declarer, mname) {
                    None => {
                        unclassified
                            .entry(format!("{declarer}#{mname}"))
                            .or_default()
                            .insert(mod_id.clone());
                    }
                    Some((group, write)) => {
                        if !UNCARRIED.contains(&group) {
                            reads
                                .entry(owner.clone())
                                .or_default()
                                .extend(carried(group));
                        }
                        if write {
                            writers
                                .entry((key.clone(), owner.clone(), group.to_owned()))
                                .or_default()
                                .insert(mod_id.clone());
                        }
                    }
                }
            }
            if m.code.is_some() && names_caps {
                for reg in capability_registrations(m, &info.pool, &ids).map_err(text)? {
                    match reg {
                        None => *caps_unresolved.entry(mod_id.clone()).or_insert(0) += 1,
                        Some((t, c)) => {
                            caps.entry(t).or_default().insert(c);
                        }
                    }
                }
            }
        }
        if touches_entity
            && info.methods.iter().flat_map(|m| &m.events).any(|ev| {
                matches!(ev, Event::Call { owner, .. } if REFLECTION.iter().any(|p| owner.starts_with(p)))
            })
        {
            *reflective.entry(mod_id.clone()).or_insert(0) += 1;
        }
        if !hier.vanilla_ancestors(name).is_empty() {
            for ((key, desc), _) in override_records(&info, &mod_id, hier) {
                let owner = key.split('#').next().unwrap_or_default();
                if hier.is_a(owner, ENTITY) || CLIENT_ROUTES.iter().any(|p| owner.starts_with(p)) {
                    overrides
                        .entry(name.clone())
                        .or_default()
                        .insert(format!("{key}{desc}"));
                }
            }
        }
    }
    let candidates = event_classes(hier);
    let candidate_set: HashSet<String> = candidates.iter().cloned().collect();
    let lib = LibraryClasses::new(&hier.jars)?;
    let mut hooks = serde_json::Map::new();
    let mut hooks_missing = Vec::new();
    for (hook, line) in hooks_table {
        let mut found = BTreeMap::new();
        let closure = hook_closure(hier, &lib, line, &candidate_set, &mut found)?;
        for (k, who) in found {
            unclassified.entry(k).or_default().insert(who);
        }
        let Some(closure) = closure else {
            hooks_missing.push(line.clone());
            continue;
        };
        for w in closure["writes"].as_array().into_iter().flatten() {
            let field = |f: &str| w[f].as_str().unwrap_or_default().to_owned();
            writers
                .entry((field("key"), field("receiver"), field("group")))
                .or_default()
                .insert("neoforge".to_owned());
        }
        hooks.insert(hook.clone(), closure);
    }
    inherit(&mut events, hier, &candidates);
    let mut event_rows = BTreeMap::new();
    for (event, mut listeners) in events.0 {
        let guards: Vec<Guard> = listeners.iter().map(|l| l.guards.clone()).collect();
        let subjects = match subjects(&guards) {
            None => json!("all"),
            Some(unique) => json!(unique),
        };
        listeners.sort_by(|a, b| {
            (&a.mod_id, &a.class, &a.method).cmp(&(&b.mod_id, &b.class, &b.method))
        });
        let rows: Vec<Json> = listeners
            .into_iter()
            .map(|l| json!({"mod": l.mod_id, "class": l.class, "method": l.method, "guards": l.guards}))
            .collect();
        event_rows.insert(event, json!({"listeners": rows, "subjects": subjects}));
    }
    for (m, row) in &r.mods {
        if row.unresolved_listeners > 0 {
            *unresolved.entry(m.clone()).or_insert(0) += row.unresolved_listeners;
        }
    }
    let mut supers = BTreeMap::new();
    for c in &candidates {
        let above: Vec<String> = hier
            .ancestors(c)
            .into_iter()
            .filter(|a| candidate_set.contains(a))
            .collect();
        if !above.is_empty() {
            supers.insert(c.clone(), above);
        }
    }
    let writes: Vec<Json> = writers
        .into_iter()
        .map(|((k, rcv, g), m)| json!({"key": k, "receiver": rcv, "group": g, "mods": m}))
        .collect();
    let routes = route_facts(&archives, hier, &r.methods)?;
    let mut opaque: Vec<String> = r
        .mods
        .iter()
        .flat_map(|(m, row)| row.opaque.iter().map(move |why| format!("{m}: {why}")))
        .collect();
    opaque.sort();
    let unclassified: Vec<String> = unclassified
        .into_iter()
        .map(|(k, m)| format!("{k} ({})", m.into_iter().collect::<Vec<_>>().join(", ")))
        .collect();
    Ok(json!({
        "format": 2,
        "event_root": ENTITY_EVENT,
        "event_prefixes": EVENT_PREFIXES,
        "event_supers": supers,
        "events": event_rows,
        "reads": reads,
        "writes": writes,
        "hooks": hooks,
        "hooks_missing": hooks_missing,
        "routes": routes,
        "overrides": overrides,
        "caps": caps,
        "caps_unresolved": caps_unresolved,
        "opaque": opaque,
        "unclassified": unclassified,
        "unresolved_listeners": unresolved,
        "reflective_entity_access": reflective,
    }))
}

/// The unclassified entity methods of a facts value, as the tool's error text; None when there are none.
pub fn unclassified_error(facts: &Json) -> Option<String> {
    let names: Vec<&str> = facts["unclassified"]
        .as_array()?
        .iter()
        .filter_map(Json::as_str)
        .collect();
    (!names.is_empty())
        .then(|| format!("{}\n  {}", super::groups::UNCLASSIFIED, names.join("\n  ")))
}

/// Writes what the boot reads: `entity-facts.json` (routing.json and routing.tsv come with SPEC-9).
pub fn write_boot_files(out: &Path, facts: &Json) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let path = out.join("entity-facts.json");
    std::fs::write(&path, crate::replaces::python_json(facts)?)
        .map_err(|e| format!("{}: {e}", path.display()))
}
