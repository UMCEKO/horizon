//! How each vanilla method the mods' mixins change is routed to Java: edge or body, receivers, callees (`routing.json`, `routing.tsv`, `replaces.md`).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::path::Path;
use std::rc::Rc;
use std::sync::LazyLock;

use serde::Serialize;
use serde::ser::{SerializeMap, Serializer};
use serde_json::Value as Json;

use crate::class::{
    Annotation, ClassInfo, Method, Parse, is_vanilla, params, parse_class, python_value,
};
use crate::entity_facts::bytecode::{
    CHECKCAST, DUP, GETSTATIC, GOTO, IFEQ, IFNE, INSTANCEOF, INVOKEINTERFACE, INVOKEVIRTUAL, POP,
    RETURN, instructions, is_aload, is_astore,
};
use crate::entity_facts::facts::{Archives, CALLBACKS, handler_method, library_target};
use crate::hierarchy::Hierarchy;
use crate::knowledge::{Knowledge, WEAK_STATUSES};
use crate::lambdas::Vanilla;
use crate::replaces::{Change, ROUTING_ORDER, Replaces, python_json};

const EDGE_AT: [&str; 3] = ["HEAD", "RETURN", "TAIL"];
pub const CLOSURE_CAP: usize = 2000;
pub const HOT_DEPTH: usize = 3;
const INVOKESPECIAL: u8 = 0xb7;
const CONSTRUCTORS: [&str; 2] = ["<init>", "<clinit>"];
const JAVA_BUILT: [&str; 9] = [
    "client",
    "player",
    "entity",
    "block_entity",
    "block",
    "item",
    "level",
    "menu",
    "value",
];
const ACC_PRIVATE: u16 = 0x2;
const ACC_STATIC: u16 = 0x8;
const ROOTS_TSV: &str = include_str!("../data/routing_roots.tsv");
const FAMILIES_TSV: &str = include_str!("../data/families.tsv");
const VIEW_INITS: &str = include_str!(
    "../../../mods/horizon/src/java-mods-host/plugin/dev/horizonlab/javamods/plugin/ViewInits.java"
);

pub const GUARD_FIELDS: [&str; 7] = [
    "recv",
    "recv_item",
    "arg_item",
    "arg_class",
    "arg_block",
    "block_at",
    "be_at",
];
const INDEXED: [&str; 3] = ["arg_item", "arg_class", "arg_block"];
const ITEM_TYPE: &str = "Lnet/minecraft/world/item/Item;";
const STACK_TYPE: &str = "Lnet/minecraft/world/item/ItemStack;";
const BLOCK_TYPE: &str = "Lnet/minecraft/world/level/block/Block;";
const STATE_TYPE: &str = "Lnet/minecraft/world/level/block/state/BlockState;";
const BE_TYPE: &str = "Lnet/minecraft/world/level/block/entity/BlockEntity;";
const POS_TYPE: &str = "Lnet/minecraft/core/BlockPos;";
const HOLDER_TYPE: &str = "Lnet/minecraft/core/Holder;";
const ICONST_0: u8 = 0x03;
const ICONST_1: u8 = 0x04;
const IRETURN: u8 = 0xac;
const IFNULL: u8 = 0xc6;
const IFNONNULL: u8 = 0xc7;
const LISTED: [&str; 4] = ["block", "item", "entity", "block_entity"];

/// A vanilla method: owner, name, descriptor.
pub type Key = (String, String, String);

/// The `java_bridges!` entries: owner, name, descriptor.
pub type Bridges = BTreeSet<(String, String, String)>;

fn data_rows(text: &'static str) -> impl Iterator<Item = (&'static str, &'static str)> {
    text.lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.split_once('\t'))
}

static HOT_ROOTS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    data_rows(ROOTS_TSV)
        .filter(|(k, _)| *k == "hot")
        .map(|(_, v)| v)
        .collect()
});

static VIEW_ROOTS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    data_rows(ROOTS_TSV)
        .filter(|(k, _)| *k == "view")
        .map(|(_, v)| v)
        .collect()
});

static FAMILIES: LazyLock<Vec<(&'static str, &'static str)>> =
    LazyLock::new(|| data_rows(FAMILIES_TSV).collect());

/// Port of mod_routing.desc_parts.
pub fn desc_parts(desc: &str) -> (Vec<String>, String) {
    let b = desc.as_bytes();
    let (mut out, mut i) = (Vec::new(), 1);
    while i < b.len() && b[i] != b')' {
        let start = i;
        while i < b.len() && b[i] == b'[' {
            i += 1;
        }
        i = if b.get(i) == Some(&b'L') {
            desc[i..].find(';').map_or(b.len(), |j| i + j + 1)
        } else {
            i + 1
        };
        out.push(desc[start..i.min(b.len())].to_owned());
    }
    (out, desc.get(i + 1..).unwrap_or("").to_owned())
}

/// Port of mod_routing.Graph.
pub struct Graph<'a> {
    pub vanilla: &'a Vanilla,
    pub hier: &'a Hierarchy,
    resolved: RefCell<HashMap<Key, Option<Key>>>,
}

impl<'a> Graph<'a> {
    pub fn new(vanilla: &'a Vanilla, hier: &'a Hierarchy) -> Graph<'a> {
        Graph {
            vanilla,
            hier,
            resolved: RefCell::new(HashMap::new()),
        }
    }

    /// Port of mod_routing.Graph.resolve.
    fn resolve(&self, owner: &str, name: &str, desc: &str) -> Option<Key> {
        let k = (owner.to_owned(), name.to_owned(), desc.to_owned());
        if let Some(found) = self.resolved.borrow().get(&k) {
            return found.clone();
        }
        let decl = is_vanilla(owner).then(|| self.hier.declarer(owner, name, Some(desc)));
        let found = decl
            .filter(|d| !d.is_empty() && is_vanilla(d))
            .map(|d| (d, name.to_owned(), desc.to_owned()));
        self.resolved.borrow_mut().insert(k, found.clone());
        found
    }

    /// Port of mod_routing.Graph.callees.
    pub fn callees(&self, k: &Key) -> Vec<Key> {
        let ops = self.vanilla.ops(&k.0, &k.1, &k.2);
        ops.0
            .iter()
            .filter_map(|(_, o, n, d)| self.resolve(o, n, d))
            .collect()
    }
}

/// Port of mod_routing.key_of.
pub fn key_of(k: &Key) -> String {
    format!("{}#{}{}", k.0, k.1, k.2)
}

/// Port of mod_routing.split_key.
pub fn split_key(key: &str) -> (String, String, Option<String>) {
    let (cls, rest) = key.split_once('#').unwrap_or((key, ""));
    match rest.split_once('(') {
        Some((name, desc)) => (cls.to_owned(), name.to_owned(), Some(format!("({desc}"))),
        None => (cls.to_owned(), rest.to_owned(), None),
    }
}

fn split_full(key: &str) -> Key {
    let (c, n, d) = split_key(key);
    (c, n, d.unwrap_or_default())
}

/// Port of mod_routing.hot_reach.
pub fn hot_reach(graph: &Graph) -> (HashSet<Key>, HashSet<String>) {
    let mut frontier: Vec<Key> = HOT_ROOTS.iter().map(|r| split_full(r)).collect();
    let mut seen: HashSet<Key> = frontier.iter().cloned().collect();
    for _ in 0..HOT_DEPTH {
        let mut next = Vec::new();
        for k in &frontier {
            for c in graph.callees(k) {
                if seen.insert(c.clone()) {
                    next.push(c);
                }
            }
        }
        frontier = next;
    }
    let owners = seen.iter().map(|k| k.0.clone()).collect();
    (seen, owners)
}

/// Port of mod_routing.subclass_index.
pub fn subclass_index(hier: &Hierarchy) -> HashMap<String, Vec<String>> {
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for (name, sup, ifaces) in hier.head_entries() {
        if is_vanilla(name) {
            for parent in sup.into_iter().chain(ifaces.iter().map(String::as_str)) {
                children
                    .entry(parent.to_owned())
                    .or_default()
                    .push(name.to_owned());
            }
        }
    }
    for list in children.values_mut() {
        list.sort();
    }
    children
}

/// A vanilla subclass that overrides a routed method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Overrider {
    pub class: String,
    pub super_call: bool,
}

struct Overriding<'g, 'a> {
    graph: &'g Graph<'a>,
    owner: &'g str,
    name: &'g str,
    desc: &'g str,
    memo: HashMap<String, bool>,
}

impl Overriding<'_, '_> {
    fn declares(&self, cls: &str) -> bool {
        self.graph
            .hier
            .methods_of(cls)
            .iter()
            .any(|m| m.0 == self.name && m.1 == self.desc && m.2 & (ACC_STATIC | ACC_PRIVATE) == 0)
    }

    fn super_calls(&mut self, cls: &str) -> bool {
        if let Some(found) = self.memo.get(cls) {
            return *found;
        }
        self.memo.insert(cls.to_owned(), false);
        let ops = self.graph.vanilla.ops(cls, self.name, self.desc);
        for (op, o, n, d) in &ops.0 {
            if *op != INVOKESPECIAL || n != self.name || d != self.desc {
                continue;
            }
            let hier = self.graph.hier;
            let decl = hier.declarer(o, self.name, Some(self.desc));
            if decl == self.owner
                || (decl != cls
                    && hier.is_a(&decl, self.owner)
                    && self.declares(&decl)
                    && self.super_calls(&decl))
            {
                self.memo.insert(cls.to_owned(), true);
                break;
            }
        }
        self.memo[cls]
    }
}

/// Port of mod_routing.overriders.
pub fn overriders(
    graph: &Graph,
    children: &HashMap<String, Vec<String>>,
    owner: &str,
    name: &str,
    desc: &str,
) -> Vec<Overrider> {
    let mut walk = Overriding {
        graph,
        owner,
        name,
        desc,
        memo: HashMap::new(),
    };
    let mut found = Vec::new();
    let mut queue: Vec<String> = children.get(owner).cloned().unwrap_or_default();
    let mut seen = HashSet::new();
    while let Some(cls) = queue.pop() {
        if !seen.insert(cls.clone()) {
            continue;
        }
        queue.extend(children.get(&cls).into_iter().flatten().cloned());
        if walk.declares(&cls) {
            let super_call = walk.super_calls(&cls);
            found.push(Overrider {
                class: cls,
                super_call,
            });
        }
    }
    found.sort_by(|a, b| a.class.cmp(&b.class));
    found
}

/// Port of mod_routing.family.
pub fn family(hier: &Hierarchy, owner: &str, is_static: bool) -> String {
    if owner.starts_with("net/minecraft/world/level/levelgen/")
        || hier.is_a(owner, "net/minecraft/server/level/ChunkMap")
    {
        return "worldgen".to_owned();
    }
    if owner.starts_with("net/minecraft/client/") {
        return "client".to_owned();
    }
    if is_static {
        return "static".to_owned();
    }
    FAMILIES
        .iter()
        .find(|(_, root)| hier.is_a(owner, root))
        .map_or("value", |(name, _)| name)
        .to_owned()
}

/// Port of mod_routing.view_classes: the `VIEWS` set of `ViewInits`, built in.
pub fn view_classes() -> BTreeSet<String> {
    view_classes_of(VIEW_INITS)
}

fn view_classes_of(text: &str) -> BTreeSet<String> {
    let mut from = 0;
    while let Some(i) = text[from..].find("VIEWS").map(|i| from + i) {
        from = i + 1;
        let rest = text[i + 5..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix("Set.of(") else {
            continue;
        };
        let Some(end) = rest.find(')') else {
            continue;
        };
        return quoted(&rest[..end]);
    }
    BTreeSet::new()
}

fn quoted(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut i = 0;
    while let Some(start) = text[i..].find('"').map(|s| i + s) {
        match text[start + 1..].find('"') {
            Some(0) => i = start + 1,
            Some(len) => {
                out.insert(text[start + 1..start + 1 + len].to_owned());
                i = start + len + 2;
            }
            None => break,
        }
    }
    out
}

/// Port of mod_routing.construction.
pub fn construction(
    owner: &str,
    name: &str,
    family: &str,
    views: &BTreeSet<String>,
) -> (&'static str, String) {
    if name == "<clinit>" {
        return (
            "java",
            "a static initialiser runs in Java when Java loads the class".to_owned(),
        );
    }
    if views.contains(owner) {
        return (
            "seam",
            "Java builds the view without its constructor; ViewInits keeps the patches and Inits.run calls them after".to_owned(),
        );
    }
    if JAVA_BUILT.contains(&family) {
        return (
            "java",
            format!(
                "Java builds its own {family} objects, so the patches run in their constructors"
            ),
        );
    }
    (
        "deferred",
        format!("native builds {family} objects and Java never does: no post-construct seam yet"),
    )
}

/// The callees of a body route.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Closure {
    pub bridged: Vec<String>,
    pub pure: Vec<String>,
    pub routed: Vec<String>,
    pub needs_bridge: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

/// Port of mod_routing.closure.
pub fn closure(
    graph: &Graph,
    root: &Key,
    routed: &BTreeSet<String>,
    bridges: &Bridges,
    native: &Knowledge,
) -> Closure {
    let mut seen: HashSet<Key> = HashSet::from([root.clone()]);
    let mut queue = VecDeque::from([root.clone()]);
    let mut walked = 0;
    let (mut bridged, mut pure, mut hit, mut needs) = (
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
        BTreeSet::new(),
    );
    let mut truncated = false;
    while !truncated && let Some(next) = queue.pop_front() {
        for c in graph.callees(&next) {
            if !seen.insert(c.clone()) {
                continue;
            }
            let key = key_of(&c);
            if routed.contains(&key) {
                hit.insert(key);
            } else if bridges.contains(&c) {
                bridged.insert(key);
            } else if VIEW_ROOTS.iter().any(|r| graph.hier.is_a(&c.0, r))
                && native.info(&format!("{}#{}", c.0, c.1)).owned
            {
                needs.insert(key);
            } else {
                walked += 1;
                if walked > CLOSURE_CAP {
                    truncated = true;
                    break;
                }
                pure.insert(key);
                queue.push_back(c);
            }
        }
    }
    Closure {
        bridged: bridged.into_iter().collect(),
        pure: pure.into_iter().collect(),
        routed: hit.into_iter().collect(),
        needs_bridge: needs.into_iter().collect(),
        truncated,
    }
}

/// The mods' archives with the handler methods and mixin classes read from them, each once.
pub struct Mods<'a> {
    archives: Archives<'a>,
    handlers: HashMap<(String, String, String), Rc<Vec<Method>>>,
    classes: HashMap<(String, String), Option<Rc<ClassInfo>>>,
}

impl<'a> Mods<'a> {
    pub fn new(archives: Archives<'a>) -> Mods<'a> {
        Mods {
            archives,
            handlers: HashMap::new(),
            classes: HashMap::new(),
        }
    }

    /// Port of mod_routing.mixin_class.
    fn mixin_class(&mut self, mod_id: &str, mixin: &str) -> Result<Option<Rc<ClassInfo>>, String> {
        let k = (mod_id.to_owned(), mixin.to_owned());
        if let Some(found) = self.classes.get(&k) {
            return Ok(found.clone());
        }
        let entry = format!("{}.class", mixin.replace('.', "/"));
        let what = Parse {
            members: true,
            annotations: false,
            code: true,
        };
        let found = match self.archives.mod_entry(mod_id, &entry)? {
            Some(data) => Some(Rc::new(
                parse_class(&data, what).map_err(|e| format!("{entry}: {e}"))?,
            )),
            None => None,
        };
        self.classes.insert(k, found.clone());
        Ok(found)
    }
}

fn handler_parts(change: &Change) -> (String, String) {
    let full = change.handler.as_deref().unwrap_or_default();
    let (mixin, handler) = full.rsplit_once('.').unwrap_or(("", full));
    (mixin.to_owned(), handler.to_owned())
}

/// Port of mod_routing.handlers.
pub fn handlers(
    mods: &mut Mods,
    change: &Change,
) -> Result<(String, String, Rc<Vec<Method>>), String> {
    let (mixin, handler) = handler_parts(change);
    let k = (change.mod_id.clone(), mixin.clone(), handler.clone());
    if let Some(found) = mods.handlers.get(&k) {
        return Ok((mixin, handler, found.clone()));
    }
    let found = Rc::new(handler_method(
        &mods.archives,
        &change.mod_id,
        &mixin,
        &handler,
    )?);
    mods.handlers.insert(k, found.clone());
    Ok((mixin, handler, found))
}

/// Port of mod_routing.callback_split.
pub fn callback_split(desc: &str) -> Option<(Vec<String>, bool)> {
    let hp = params(desc);
    let i = hp.iter().position(|p| CALLBACKS.contains(&p.as_str()))?;
    Some((hp[..i].to_vec(), hp.len() > i + 1))
}

/// Port of mod_routing.annotation: the first descriptor ending in `suffix`, with the values Python's dict keeps for it.
pub fn annotation<'m>(method: &'m Method, suffix: &str) -> Option<&'m Annotation> {
    let first = method.annos.iter().find(|a| a.desc.ends_with(suffix))?;
    method.annos.iter().rev().find(|a| a.desc == first.desc)
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

fn flag(a: Option<&Annotation>, name: &str) -> bool {
    a.and_then(|a| a.values.iter().rev().find(|(n, _)| n == name))
        .is_some_and(|(_, v)| truthy(&python_value(v)))
}

/// What a handler's prefix proves about the values it acts on, per guard field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardValue {
    Classes(Vec<String>),
    Indexed(Vec<(String, Vec<String>)>),
}

impl Serialize for GuardValue {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            GuardValue::Classes(classes) => classes.serialize(s),
            GuardValue::Indexed(rows) => {
                let mut m = s.serialize_map(Some(rows.len()))?;
                for (k, v) in rows {
                    m.serialize_entry(k, v)?;
                }
                m.end()
            }
        }
    }
}

/// The guards of a change or a route, in `GUARD_FIELDS` order; `None` is any.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Guards(pub [Option<GuardValue>; 7]);

impl Guards {
    pub fn get(&self, field: &str) -> Option<&GuardValue> {
        let i = GUARD_FIELDS.iter().position(|f| *f == field)?;
        self.0[i].as_ref()
    }
}

impl Serialize for Guards {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(GUARD_FIELDS.len()))?;
        for (field, value) in GUARD_FIELDS.iter().zip(&self.0) {
            m.serialize_entry(field, value)?;
        }
        m.end()
    }
}

/// One change of a route as `routing.json` writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeRow {
    #[serde(rename = "mod")]
    pub mod_id: String,
    pub mixin: String,
    pub handler: String,
    pub kind: Option<String>,
    pub at: Vec<String>,
    pub cancellable: bool,
    pub locals: bool,
    pub conditional: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lambda: Option<String>,
    pub guards: Option<Guards>,
    pub guard_stop: Option<String>,
}

/// Port of mod_routing.change_row.
pub fn change_row(
    mods: &mut Mods,
    change: &Change,
    target: Option<&str>,
) -> Result<(ChangeRow, bool), String> {
    let (mixin, handler, found) = handlers(mods, change)?;
    let kind = change.kind.as_deref().unwrap_or_default();
    let at = &change.at;
    let target_params = target.filter(|t| !t.is_empty()).map(params);
    let fits: Vec<&Method> = found
        .iter()
        .filter(|f| {
            target_params.as_ref().is_some_and(|tp| {
                callback_split(&f.desc).is_some_and(|s| s.0 == *tp || s.0.is_empty())
            })
        })
        .collect();
    let method = fits.first().copied().or_else(|| found.first());
    let inject = method
        .filter(|_| kind == "inject")
        .and_then(|m| annotation(m, "/Inject;"));
    let locals = kind == "inject"
        && fits
            .first()
            .and_then(|f| callback_split(&f.desc))
            .is_some_and(|s| s.1);
    let edge = if change.folded.is_some() {
        false
    } else {
        match kind {
            "inject" => {
                !at.is_empty()
                    && at.iter().all(|a| EDGE_AT.contains(&a.as_str()))
                    && !fits.is_empty()
                    && !locals
            }
            "modify_return_value" => at.iter().all(|a| a.split('(').next() == Some("RETURN")),
            "wrap_method" => true,
            "modify_variable" => {
                at.len() == 1
                    && at[0] == "HEAD"
                    && method.is_some_and(|m| flag(annotation(m, "/ModifyVariable;"), "argsOnly"))
            }
            _ => false,
        }
    };
    let row = ChangeRow {
        mod_id: change.mod_id.clone(),
        mixin,
        handler,
        kind: change.kind.clone(),
        at: at.clone(),
        cancellable: flag(inject, "cancellable"),
        locals,
        conditional: change.conditional.as_deref().is_some_and(|c| !c.is_empty()),
        lambda: change.folded.clone(),
        guards: None,
        guard_stop: None,
    };
    Ok((row, edge))
}

/// Port of mod_routing.targets_of.
pub fn targets_of(
    hier: &Hierarchy,
    mods: &mut Mods,
    owner: &str,
    name: &str,
    change: &Change,
) -> Result<Vec<Option<String>>, String> {
    let found = if change.kind.as_deref() == Some("inject") {
        handlers(mods, change)?.2
    } else {
        Rc::new(Vec::new())
    };
    let split = if found.len() == 1 {
        callback_split(&found[0].desc)
    } else {
        None
    };
    Ok(library_target(
        hier,
        owner,
        name,
        change.desc.as_deref(),
        split.as_ref().map(|s| s.0.as_slice()),
    ))
}

/// A value the guard walk follows.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Val {
    This,
    Arg(usize),
    Stack(Box<Val>),
    Item(Box<Val>),
    Holder(Box<Val>),
    Block(Box<Val>),
    BlockAt,
    StateAt(usize, usize),
    BeAt(usize, usize),
    Inst(Box<Val>, String),
    Guards(Vec<Found>),
    Int(u8),
}

/// A proven guard: field, argument index, class.
type Found = (&'static str, Option<usize>, String);

/// Port of mod_routing.guard_of.
fn guard_of(value: &Val, cls: &str) -> Option<Found> {
    let index = |v: &Val| match v {
        Val::Arg(i) => Some(*i),
        _ => None,
    };
    match value {
        Val::This => Some(("recv", None, cls.to_owned())),
        Val::Arg(i) => Some(("arg_class", Some(*i), cls.to_owned())),
        Val::Item(base) if **base == Val::This => Some(("recv_item", None, cls.to_owned())),
        Val::Item(base) => Some(("arg_item", index(base), cls.to_owned())),
        Val::Block(recv) => Some(("arg_block", index(recv), cls.to_owned())),
        Val::BlockAt => Some(("block_at", None, cls.to_owned())),
        Val::BeAt(..) => Some(("be_at", None, cls.to_owned())),
        _ => None,
    }
}

type Step = (Option<Val>, Option<String>);

/// Port of mod_routing.invoked.
fn invoked(
    stack: &mut Vec<Option<Val>>,
    (owner, name, desc): (&str, &str, &str),
    info: &ClassInfo,
    depth: usize,
) -> Result<Step, String> {
    let (args, ret) = desc_parts(desc);
    if args.len() == 1
        && args[0] == POS_TYPE
        && ((name == "getBlockState" && ret == STATE_TYPE)
            || (name == "getBlockEntity" && ret == BE_TYPE))
    {
        let pos = stack.pop().flatten();
        let level = stack.pop().flatten();
        if let (Some(Val::Arg(l)), Some(Val::Arg(p))) = (level, pos) {
            let v = if ret == STATE_TYPE {
                Val::StateAt(l, p)
            } else {
                Val::BeAt(l, p)
            };
            return Ok((Some(v), None));
        }
        return Ok((
            None,
            Some(format!("{name} on an unknown level or position")),
        ));
    }
    if !args.is_empty() {
        return Ok((None, Some(format!("a call of {owner}.{name}{desc}"))));
    }
    let Some(recv) = stack.pop().flatten() else {
        return Ok((
            None,
            Some(format!(
                "a call of {owner}.{name}{desc} on an unknown value"
            )),
        ));
    };
    let base = match &recv {
        Val::Stack(b) | Val::Holder(b) => (**b).clone(),
        other => other.clone(),
    };
    let this_or_arg = matches!(recv, Val::This | Val::Arg(_));
    let or_stack = this_or_arg || matches!(recv, Val::Stack(_));
    if name == "getItem" && ret == STACK_TYPE && this_or_arg {
        return Ok((Some(Val::Stack(Box::new(recv))), None));
    }
    if name == "getItem" && ret == ITEM_TYPE && or_stack {
        return Ok((Some(Val::Item(Box::new(base))), None));
    }
    if name == "typeHolder" && ret == HOLDER_TYPE && or_stack {
        return Ok((Some(Val::Holder(Box::new(base))), None));
    }
    if name == "value" && ret == "Ljava/lang/Object;" && matches!(recv, Val::Holder(_)) {
        return Ok((Some(Val::Item(Box::new(base))), None));
    }
    if name == "getBlock" && ret == BLOCK_TYPE && matches!(recv, Val::Arg(_) | Val::StateAt(..)) {
        let v = if matches!(recv, Val::Arg(_)) {
            Val::Block(Box::new(recv))
        } else {
            Val::BlockAt
        };
        return Ok((Some(v), None));
    }
    if owner == info.name && recv == Val::This && desc == "()Z" && depth == 0 {
        let helper = info.methods.iter().find(|m| {
            m.name == name && m.desc == desc && m.access & ACC_PRIVATE != 0 && m.code.is_some()
        });
        let Some(helper) = helper else {
            return Ok((None, Some(format!("{name}{desc} is no private helper"))));
        };
        let (found, why) = walk_guard(
            info,
            helper,
            HashMap::from([(0, Some(Val::This))]),
            depth + 1,
        )?;
        if !found.is_empty() && why.is_none() {
            return Ok((Some(Val::Guards(found)), None));
        }
        let why = why.unwrap_or_else(|| "proves nothing".to_owned());
        return Ok((None, Some(format!("helper {name}: {why}"))));
    }
    Ok((None, Some(format!("a call of {owner}.{name}{desc}"))))
}

fn text(e: crate::class::ClassError) -> String {
    e.to_string()
}

fn pool_index(operand: Option<i64>) -> u16 {
    operand.and_then(|o| u16::try_from(o).ok()).unwrap_or(0)
}

/// Port of mod_routing.walk_guard.
fn walk_guard(
    info: &ClassInfo,
    method: &Method,
    locals: HashMap<i64, Option<Val>>,
    depth: usize,
) -> Result<(Vec<Found>, Option<String>), String> {
    let pool = &info.pool;
    let ins = instructions(method.code.as_deref().unwrap_or_default()).map_err(text)?;
    let at: HashMap<i64, usize> = ins
        .iter()
        .enumerate()
        .map(|(n, i)| (i64::try_from(i.offset).unwrap_or(i64::MAX), n))
        .collect();
    let mut stack: Vec<Option<Val>> = Vec::new();
    let mut local = locals;
    let mut found: Vec<Found> = Vec::new();
    let exits = |n: Option<usize>| -> bool {
        let mut n = n;
        if let Some(i) = n
            && i < ins.len()
            && ins[i].op == GOTO
        {
            n = ins[i].operand.and_then(|o| at.get(&o).copied());
        }
        let Some(n) = n.filter(|n| *n < ins.len()) else {
            return false;
        };
        if depth == 0 {
            return ins[n].op == RETURN;
        }
        ins[n].op == ICONST_0 && n + 1 < ins.len() && ins[n + 1].op == IRETURN
    };
    let mut n = 0;
    while n < ins.len() {
        let (offset, op, operand) = (ins[n].offset, ins[n].op, ins[n].operand);
        let place = format!("{}@{offset}", method.name);
        n += 1;
        if is_aload(op) {
            stack.push(operand.and_then(|o| local.get(&o).cloned()).flatten());
        } else if is_astore(op) {
            let v = stack.pop().flatten();
            local.insert(operand.unwrap_or(-1), v);
        } else if op == DUP && !stack.is_empty() {
            let top = stack[stack.len() - 1].clone();
            stack.push(top);
        } else if op == POP && !stack.is_empty() {
            stack.pop();
        } else if op == CHECKCAST && !stack.is_empty() {
            continue;
        } else if op == ICONST_0 || op == ICONST_1 {
            stack.push(Some(Val::Int(op - ICONST_0)));
        } else if op == INSTANCEOF {
            let value = stack.pop().flatten();
            let cls = pool
                .cls(pool_index(operand))
                .map_err(text)?
                .unwrap_or_default();
            match value {
                Some(v) if guard_of(&v, &cls).is_some() => {
                    stack.push(Some(Val::Inst(Box::new(v), cls)));
                }
                _ => {
                    return Ok((
                        found,
                        Some(format!("{place}: instanceof on a value it does not follow")),
                    ));
                }
            }
        } else if op == INVOKEVIRTUAL || op == INVOKEINTERFACE || op == INVOKESPECIAL {
            let (owner, name, desc) = pool.reference(pool_index(operand)).map_err(text)?;
            let (value, why) = invoked(&mut stack, (&owner, &name, &desc), info, depth)?;
            match value {
                Some(v) => stack.push(Some(v)),
                None => {
                    return Ok((found, Some(format!("{place}: {}", why.unwrap_or_default()))));
                }
            }
        } else if op == IFNULL || op == IFNONNULL || op == IFEQ || op == IFNE {
            let value = stack.pop().flatten();
            let target = operand.and_then(|o| at.get(&o).copied());
            let at_offset = i64::try_from(offset).unwrap_or(i64::MAX);
            if target.is_none() || operand.is_none_or(|o| o <= at_offset) {
                return Ok((found, Some(format!("{place}: a backward branch"))));
            }
            let held = if exits(target) {
                op == IFEQ
            } else if exits(Some(n)) {
                n = target.unwrap_or(n);
                op == IFNE
            } else {
                return Ok((found, Some(format!("{place}: a branch that does not exit"))));
            };
            if op == IFNULL || op == IFNONNULL {
                continue;
            }
            match value {
                Some(Val::Inst(v, c)) => {
                    if held {
                        found.extend(guard_of(&v, &c));
                    }
                }
                Some(Val::Guards(g)) => {
                    if held {
                        found.extend(g);
                    }
                }
                _ => {
                    return Ok((
                        found,
                        Some(format!("{place}: a branch on a value it does not follow")),
                    ));
                }
            }
        } else if depth > 0 && op == IRETURN {
            return Ok(match stack.pop().flatten() {
                Some(Val::Int(1)) => (found, None),
                Some(Val::Inst(v, c)) => {
                    found.extend(guard_of(&v, &c));
                    (found, None)
                }
                _ => (
                    Vec::new(),
                    Some(format!("{place}: returns a value it does not follow")),
                ),
            });
        } else if depth > 0 && op == GOTO {
            let target = operand.and_then(|o| at.get(&o).copied());
            let returns_true = stack.last() == Some(&Some(Val::Int(1)));
            if target.is_none_or(|t| ins[t].op != IRETURN) || !returns_true {
                return Ok((
                    Vec::new(),
                    Some(format!("{place}: a jump it does not follow")),
                ));
            }
            return Ok((found, None));
        } else if op == GETSTATIC {
            let (owner, name, _) = pool.reference(pool_index(operand)).map_err(text)?;
            return Ok((found, Some(format!("{place}: getstatic {owner}.{name}"))));
        } else {
            return Ok((found, Some(format!("{place}: opcode 0x{op:02x}"))));
        }
    }
    Ok((found, Some(format!("{}: the end of the code", method.name))))
}

/// Port of mod_routing.change_guards.
pub fn change_guards(
    mods: &mut Mods,
    change: &Change,
    target: Option<&str>,
) -> Result<(Option<Guards>, Option<String>), String> {
    let (mixin, handler) = handler_parts(change);
    let kind = change.kind.as_deref().unwrap_or_default();
    let Some(target) = target.filter(|t| !t.is_empty() && kind == "inject") else {
        let shown = change.kind.as_deref().unwrap_or("None");
        return Ok((None, Some(format!("a {shown} handler"))));
    };
    let info = mods.mixin_class(&change.mod_id, &mixin)?;
    let target_params = params(target);
    let fits: Vec<&Method> = info
        .iter()
        .flat_map(|i| i.methods.iter())
        .filter(|m| {
            m.name == handler
                && m.code.is_some()
                && callback_split(&m.desc).is_some_and(|s| s.0 == target_params || s.0.is_empty())
        })
        .collect();
    let (Some(info), [method]) = (info.as_deref(), fits.as_slice()) else {
        return Ok((None, Some("no single handler method".to_owned())));
    };
    let mut slot = 0;
    let mut locals = HashMap::new();
    if method.access & ACC_STATIC == 0 {
        locals.insert(0, Some(Val::This));
        slot = 1;
    }
    for (i, t) in callback_split(&method.desc)
        .map(|s| s.0)
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        locals.insert(slot, Some(Val::Arg(i)));
        slot += if t == "J" || t == "D" { 2 } else { 1 };
    }
    let (found, why) = walk_guard(info, method, locals, 0)?;
    if found.is_empty() {
        return Ok((None, why));
    }
    let mut out = Guards::default();
    for (field, index, cls) in found {
        let i = GUARD_FIELDS
            .iter()
            .position(|f| *f == field)
            .unwrap_or_default();
        if INDEXED.contains(&field) {
            let slot = out.0[i].get_or_insert_with(|| GuardValue::Indexed(Vec::new()));
            if let GuardValue::Indexed(rows) = slot {
                let key = index.map_or_else(|| "None".to_owned(), |n| n.to_string());
                let pos = match rows.iter().position(|(k, _)| *k == key) {
                    Some(p) => p,
                    None => {
                        rows.push((key, Vec::new()));
                        rows.len() - 1
                    }
                };
                if !rows[pos].1.contains(&cls) {
                    rows[pos].1.push(cls);
                }
            }
        } else if out.0[i].is_none() {
            out.0[i] = Some(GuardValue::Classes(vec![cls]));
        }
    }
    Ok((Some(out), why))
}

fn classes_of(v: &GuardValue) -> &[String] {
    match v {
        GuardValue::Classes(c) => c,
        GuardValue::Indexed(_) => &[],
    }
}

fn indexed_of(v: &GuardValue) -> &[(String, Vec<String>)] {
    match v {
        GuardValue::Indexed(rows) => rows,
        GuardValue::Classes(_) => &[],
    }
}

/// Port of mod_routing.route_guards.
pub fn route_guards(guards: &[Option<Guards>]) -> Option<Guards> {
    if guards.is_empty() || guards.iter().any(Option::is_none) {
        return None;
    }
    let guards: Vec<&Guards> = guards.iter().flatten().collect();
    let mut out = Guards::default();
    for (i, field) in GUARD_FIELDS.iter().enumerate() {
        let values: Option<Vec<&GuardValue>> = guards.iter().map(|g| g.0[i].as_ref()).collect();
        let Some(values) = values else {
            continue;
        };
        if INDEXED.contains(field) {
            let mut keys: Option<BTreeSet<&String>> = None;
            for v in &values {
                let these: BTreeSet<&String> = indexed_of(v).iter().map(|(k, _)| k).collect();
                keys = Some(match keys {
                    None => these,
                    Some(k) => k.intersection(&these).copied().collect(),
                });
            }
            let rows: Vec<(String, Vec<String>)> = keys
                .unwrap_or_default()
                .into_iter()
                .map(|k| {
                    let all: BTreeSet<&String> = values
                        .iter()
                        .flat_map(|v| indexed_of(v).iter().filter(|(kk, _)| kk == k))
                        .flat_map(|(_, c)| c)
                        .collect();
                    (k.clone(), all.into_iter().cloned().collect())
                })
                .collect();
            if !rows.is_empty() {
                out.0[i] = Some(GuardValue::Indexed(rows));
            }
        } else {
            let all: BTreeSet<&String> = values.iter().flat_map(|v| classes_of(v)).collect();
            out.0[i] = Some(GuardValue::Classes(all.into_iter().cloned().collect()));
        }
    }
    out.0.iter().any(Option::is_some).then_some(out)
}

/// Port of mod_routing.inert_by_guard.
pub fn inert_by_guard(
    family: &str,
    guards: Option<&Guards>,
    mod_class: &dyn Fn(&str) -> bool,
) -> bool {
    let Some(guards) = guards else {
        return false;
    };
    let mod_only = |classes: &[String]| !classes.is_empty() && classes.iter().all(|c| mod_class(c));
    if !LISTED.contains(&family) && guards.get("recv").is_some_and(|v| mod_only(classes_of(v))) {
        return true;
    }
    guards
        .get("arg_class")
        .is_some_and(|v| indexed_of(v).iter().any(|(_, c)| mod_only(c)))
}

/// Who runs a constructor route's patches, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Construct {
    pub by: String,
    pub reason: String,
}

/// One route of `routing.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Route {
    pub kind: String,
    #[serde(rename = "static")]
    pub is_static: bool,
    pub family: String,
    pub args: Vec<String>,
    pub ret: String,
    pub changes: Vec<ChangeRow>,
    pub returns: usize,
    pub tail: bool,
    pub overriders: Vec<Overrider>,
    pub guards: Option<Guards>,
    pub closure: Closure,
    pub hot: bool,
    pub side: String,
    pub inert: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub construct: Option<Construct>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
}

/// A key whose verdict the verdict map has yet to give.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Undecided {
    pub status: String,
    pub mods: Vec<String>,
}

/// The routing report: `routing.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub routes: BTreeMap<String, Route>,
    pub undecided: BTreeMap<String, Undecided>,
    pub lambdas: BTreeMap<String, String>,
}

impl Serialize for Report {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut m = s.serialize_map(Some(4))?;
        m.serialize_entry("format", &1)?;
        m.serialize_entry("routes", &self.routes)?;
        m.serialize_entry("undecided", &self.undecided)?;
        m.serialize_entry("lambdas", &self.lambdas)?;
        m.end()
    }
}

/// Port of mod_routing.routing.
pub fn routing(
    r: &mut Replaces,
    vanilla: &Vanilla,
    knowledge: &Knowledge,
    bridges: &Bridges,
    views: &BTreeSet<String>,
) -> Result<Report, String> {
    let Replaces {
        methods,
        archives,
        hierarchy: hier,
        ..
    } = r;
    let hier: &Hierarchy = hier;
    let graph = Graph::new(vanilla, hier);
    let mut mods = Mods::new(Archives::new(archives.iter_mut().collect()));
    let mut grouped: Vec<(String, Vec<(&Change, Option<String>)>)> = Vec::new();
    let mut group_of: HashMap<String, usize> = HashMap::new();
    let mut undecided: BTreeMap<String, Undecided> = BTreeMap::new();
    let mut lambdas: BTreeMap<String, String> = BTreeMap::new();
    let mut status_of: HashMap<String, String> = HashMap::new();
    for (key, row) in methods.iter() {
        let (owner, name) = key.split_once('#').unwrap_or((key, ""));
        for change in &row.changes {
            if !row.vanilla
                || change.mechanism != "mixin"
                || !(change.routing == "java" || change.routing == "undecided")
                || name == "<unknown>"
            {
                continue;
            }
            for desc in targets_of(hier, &mut mods, owner, name, change)? {
                let full = format!("{owner}#{name}{}", desc.as_deref().unwrap_or_default());
                if let Some(folded) = &change.folded {
                    lambdas.insert(format!("{owner}#{folded}"), full.clone());
                }
                if change.routing == "undecided" {
                    let entry = undecided.entry(full).or_insert_with(|| Undecided {
                        status: row.native.status.clone(),
                        mods: Vec::new(),
                    });
                    if !entry.mods.contains(&change.mod_id) {
                        entry.mods.push(change.mod_id.clone());
                        entry.mods.sort();
                    }
                } else {
                    let next = grouped.len();
                    let i = *group_of.entry(full.clone()).or_insert(next);
                    if i == next {
                        grouped.push((full.clone(), Vec::new()));
                    }
                    grouped[i].1.push((change, desc));
                    status_of.insert(full, row.native.status.clone());
                }
            }
        }
    }
    let children = subclass_index(hier);
    let (reach, reach_owners) = hot_reach(&graph);
    let mut routes: BTreeMap<String, Route> = BTreeMap::new();
    for (full, items) in &grouped {
        let (owner, name, desc) = split_key(full);
        let mut rows = Vec::new();
        for (change, _) in items {
            rows.push(change_row(&mut mods, change, desc.as_deref())?);
        }
        let kind = if rows.iter().all(|(_, edge)| *edge) {
            "edge"
        } else {
            "body"
        };
        let mut changes: Vec<ChangeRow> = rows.into_iter().map(|(row, _)| row).collect();
        for (row, (change, _)) in changes.iter_mut().zip(items) {
            (row.guards, row.guard_stop) = change_guards(&mut mods, change, desc.as_deref())?;
        }
        let sides: BTreeSet<Option<&str>> = items.iter().map(|(c, _)| c.side.as_deref()).collect();
        let side = match sides.iter().next() {
            Some(Some(s)) if sides.len() == 1 && (*s == "client" || *s == "server") => *s,
            _ => "common",
        };
        let access = hier
            .methods_of(&owner)
            .iter()
            .find(|m| m.0 == name && Some(&m.1) == desc.as_ref())
            .map_or(0, |m| m.2);
        let is_static = access & ACC_STATIC != 0;
        let (args, ret) = match &desc {
            Some(d) if !d.is_empty() => desc_parts(d),
            _ => (Vec::new(), "V".to_owned()),
        };
        let d = desc.clone().unwrap_or_default();
        let k: Key = (owner.clone(), name.clone(), d.clone());
        let built = CONSTRUCTORS.contains(&name.as_str());
        let fam = family(hier, &owner, is_static);
        let guards = route_guards(&changes.iter().map(|c| c.guards.clone()).collect::<Vec<_>>());
        let has_desc = desc.as_deref().is_some_and(|d| !d.is_empty());
        let mut route = Route {
            kind: kind.to_owned(),
            is_static,
            family: fam.clone(),
            args,
            ret,
            returns: vanilla.ops(&owner, &name, &d).1,
            tail: changes.iter().any(|c| c.at.iter().any(|a| a == "TAIL")),
            overriders: if has_desc && !built {
                overriders(&graph, &children, &owner, &name, &d)
            } else {
                Vec::new()
            },
            changes,
            inert: inert_by_guard(&fam, guards.as_ref(), &|c| hier.mod_class(c)),
            guards,
            closure: Closure::default(),
            hot: (desc.is_some() && reach.contains(&k))
                || (kind == "edge" && reach_owners.contains(&owner)),
            side: side.to_owned(),
            construct: None,
            verdict: None,
        };
        if built {
            let (by, reason) = construction(&owner, &name, &fam, views);
            route.construct = Some(Construct {
                by: by.to_owned(),
                reason,
            });
            route.inert = by != "deferred";
        }
        if let Some(verdict) = knowledge.verdict_of(full)
            && status_of
                .get(full)
                .is_some_and(|s| WEAK_STATUSES.contains(&s.as_str()))
        {
            route.verdict = Some(verdict.to_owned());
        }
        routes.insert(full.clone(), route);
    }
    let routed: BTreeSet<String> = routes.keys().cloned().collect();
    for (full, route) in &mut routes {
        let (owner, name, desc) = split_key(full);
        if let Some(desc) = desc.filter(|d| !d.is_empty())
            && route.kind == "body"
            && route.construct.is_none()
        {
            route.closure = closure(&graph, &(owner, name, desc), &routed, bridges, knowledge);
        }
    }
    Ok(Report {
        routes,
        undecided,
        lambdas,
    })
}

/// Port of mod_routing.tsv.
pub fn tsv(report: &Report) -> String {
    let lines: Vec<String> = report
        .routes
        .iter()
        .map(|(key, r)| {
            let (owner, name, desc) = split_key(key);
            [
                owner,
                name,
                desc.unwrap_or_default(),
                r.kind.clone(),
                u8::from(r.tail).to_string(),
                r.returns.to_string(),
            ]
            .join("\t")
        })
        .collect();
    let mut out = lines.join("\n");
    if !lines.is_empty() {
        out.push('\n');
    }
    out
}

/// Port of mod_routing.routes_tsv.
pub fn routes_tsv(report: &Report, label: &str) -> String {
    let mut lines: Vec<String> = report
        .routes
        .iter()
        .map(|(key, r)| {
            let mods: BTreeSet<&str> = r.changes.iter().map(|c| c.mod_id.as_str()).collect();
            let all_conditional = r.changes.iter().all(|c| c.conditional);
            [
                key.clone(),
                r.kind.clone(),
                r.family.clone(),
                u8::from(r.tail).to_string(),
                r.returns.to_string(),
                mods.into_iter().collect::<Vec<_>>().join(","),
                u8::from(all_conditional).to_string(),
            ]
            .join("\t")
        })
        .collect();
    lines.sort();
    let mut out = format!("# routes of {label}, mod_routing format 1\n");
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// The counts `summary` gives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub routes: usize,
    #[serde(serialize_with = "in_order")]
    pub by_kind: Vec<(String, usize)>,
    pub by_family: BTreeMap<String, usize>,
    pub undecided_keys: usize,
    pub lambdas: usize,
    pub hot: Vec<String>,
    pub needs_bridge: BTreeMap<String, Vec<String>>,
}

fn in_order<S: Serializer>(rows: &[(String, usize)], s: S) -> Result<S::Ok, S::Error> {
    let mut m = s.serialize_map(Some(rows.len()))?;
    for (k, v) in rows {
        m.serialize_entry(k, v)?;
    }
    m.end()
}

/// Port of mod_routing.summary.
pub fn summary(report: &Report) -> Summary {
    let routes = &report.routes;
    let mut by_kind: Vec<(String, usize)> = Vec::new();
    let mut by_family: BTreeMap<String, usize> = BTreeMap::new();
    for r in routes.values() {
        match by_kind.iter_mut().find(|(k, _)| *k == r.kind) {
            Some(slot) => slot.1 += 1,
            None => by_kind.push((r.kind.clone(), 1)),
        }
        *by_family
            .entry(format!("{}/{}", r.kind, r.family))
            .or_insert(0) += 1;
    }
    Summary {
        routes: routes.len(),
        by_kind,
        by_family,
        undecided_keys: report.undecided.len(),
        lambdas: report.lambdas.len(),
        hot: routes
            .iter()
            .filter(|(_, r)| r.hot)
            .map(|(k, _)| k.clone())
            .collect(),
        needs_bridge: routes
            .iter()
            .filter(|(_, r)| !r.closure.needs_bridge.is_empty())
            .map(|(k, r)| (k.clone(), r.closure.needs_bridge.clone()))
            .collect(),
    }
}

/// Port of mod_replaces.write_routing.
pub fn write_routing(out: &Path, report: &Report) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    for (name, text) in [
        ("routing.json", python_json(report)?),
        ("routing.tsv", tsv(report)),
    ] {
        let path = out.join(name);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

/// What covers each route in the tree: the gated keys, the deferrals and the verdicts (`tools/java_routing.py`'s `scan_tree`, `read_deferred`, `read_verdicts`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gates {
    pub gated: BTreeSet<String>,
    pub deferred: Vec<(String, String)>,
    pub verdicts: Vec<(String, String)>,
}

/// Port of java_routing.constructor.
fn constructor(key: &str) -> bool {
    let name = key.split_once('#').map_or("", |(_, n)| n);
    name.starts_with("<init>(") || name.starts_with("<clinit>(")
}

/// Port of java_routing.PARTIAL: `^seam: \S+:\d+`.
fn partial(reason: &str) -> bool {
    let Some(rest) = reason.strip_prefix("seam: ") else {
        return false;
    };
    let word: Vec<char> = rest.chars().take_while(|c| !c.is_whitespace()).collect();
    (1..word.len()).any(|j| word[j] == ':' && word.get(j + 1).is_some_and(char::is_ascii_digit))
}

/// Port of java_routing.owner_group.
fn owner_group(reason: &str) -> String {
    if let Some(rest) = reason.strip_prefix("owner: W") {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if !digits.is_empty() {
            return format!("W{digits}");
        }
    }
    if reason.starts_with("spec: w12-coverage ") {
        return "w12-coverage".to_owned();
    }
    reason.split(':').next().unwrap_or_default().to_owned()
}

/// Port of java_routing.group_order.
fn group_order(name: &str) -> (u8, u128, String) {
    match name.strip_prefix('W') {
        Some(d) if !d.is_empty() && d.chars().all(|c| c.is_ascii_digit()) => {
            (0, d.parse().unwrap_or(u128::MAX), String::new())
        }
        _ => (1, 0, name.to_owned()),
    }
}

/// Port of java_routing.m2: the line of one pack.
fn m2(name: &str, keys: &[&String], gates: &Gates) -> String {
    let mut reasons: HashMap<&str, Vec<&str>> = HashMap::new();
    for (key, reason) in &gates.deferred {
        if !partial(reason) {
            reasons.entry(key).or_default().push(reason);
        }
    }
    let verdict_keys: HashSet<&str> = gates.verdicts.iter().map(|(k, _)| k.as_str()).collect();
    let (mut n_gated, mut n_verdict, mut n_open) = (0, 0, 0);
    let mut owned: Vec<(String, usize)> = Vec::new();
    let keys: Vec<&&String> = keys.iter().filter(|k| !constructor(k)).collect();
    for key in &keys {
        if gates.gated.contains(key.as_str()) {
            n_gated += 1;
        } else if verdict_keys.contains(key.as_str()) {
            n_verdict += 1;
        } else if let Some(first) = reasons.get(key.as_str()).and_then(|r| r.first()) {
            let group = owner_group(first);
            match owned.iter_mut().find(|(g, _)| *g == group) {
                Some(slot) => slot.1 += 1,
                None => owned.push((group, 1)),
            }
        } else {
            n_open += 1;
        }
    }
    owned.sort_by_key(|(g, _)| group_order(g));
    let groups: Vec<String> = owned.iter().map(|(g, n)| format!("{g} {n}")).collect();
    let total: usize = owned.iter().map(|(_, n)| n).sum();
    let owned_text = if groups.is_empty() {
        format!("owned {total}")
    } else {
        format!("owned {total} ({})", groups.join(", "))
    };
    format!(
        "M2 {name}: {} routes; covered {} ({n_gated} gated + {n_verdict} verdict); {owned_text}; open {n_open}",
        keys.len(),
        n_gated + n_verdict
    )
}

/// Port of mod_replaces.table.
pub fn table(headers: &[&str], rows: &[Vec<String>]) -> Vec<String> {
    let mut lines = vec![
        format!("| {} |", headers.join(" | ")),
        format!("|{}", "---|".repeat(headers.len())),
    ];
    lines.extend(rows.iter().map(|r| format!("| {} |", r.join(" | "))));
    lines
}

/// Port of mod_replaces.short.
pub fn short(key: &str) -> String {
    let (cls, name) = key.split_once('#').unwrap_or((key, ""));
    format!("{}#{name}", cls.rsplit('/').next().unwrap_or(cls))
}

/// Port of java_routing.status_of.
fn status_of(
    key: &str,
    gates: &Gates,
    by_key: &HashMap<&str, Vec<&str>>,
    verdict_of: &HashMap<&str, &str>,
    inert: bool,
    construct: Option<&Construct>,
) -> String {
    if let Some(c) = construct {
        return format!("constructor ({})", c.by);
    }
    if inert {
        return "inert (guard)".to_owned();
    }
    if gates.gated.contains(key) {
        return "gated".to_owned();
    }
    if let Some(verdict) = verdict_of.get(key) {
        return format!("verdict ({verdict})");
    }
    if let Some(reasons) = by_key.get(key) {
        return format!("deferred ({})", reasons[0]);
    }
    "UNGATED".to_owned()
}

fn simple(cls: &str) -> &str {
    cls.rsplit('/').next().unwrap_or(cls)
}

/// Port of java_routing.guards_text.
pub fn guards_text(guards: Option<&Guards>) -> String {
    let Some(guards) = guards else {
        return "all".to_owned();
    };
    let mut parts = Vec::new();
    for (field, value) in GUARD_FIELDS.iter().zip(&guards.0) {
        match value {
            Some(GuardValue::Indexed(rows)) => {
                for (i, classes) in rows {
                    let names: Vec<&str> = classes.iter().map(|c| simple(c)).collect();
                    parts.push(format!("{field}[{i}]: {}", names.join(", ")));
                }
            }
            Some(GuardValue::Classes(classes)) if !classes.is_empty() => {
                let names: Vec<&str> = classes.iter().map(|c| simple(c)).collect();
                parts.push(format!("{field}: {}", names.join(", ")));
            }
            _ => {}
        }
    }
    parts.join("; ")
}

/// Port of java_routing.report_table, with its default pack name.
pub fn report_table(report: &Report, gates: &Gates) -> Vec<String> {
    let mut by_key: HashMap<&str, Vec<&str>> = HashMap::new();
    for (key, reason) in &gates.deferred {
        by_key.entry(key).or_default().push(reason);
    }
    let verdict_of: HashMap<&str, &str> = gates
        .verdicts
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let rows: Vec<[String; 7]> = report
        .routes
        .iter()
        .map(|(key, r)| {
            let mods: BTreeSet<&str> = r.changes.iter().map(|c| c.mod_id.as_str()).collect();
            let need: Vec<&str> = r
                .closure
                .needs_bridge
                .iter()
                .map(|n| n.rsplit('#').next().unwrap_or(n))
                .collect();
            [
                key.replace("net/minecraft/", ""),
                r.kind.clone(),
                r.family.clone(),
                mods.into_iter().collect::<Vec<_>>().join(", "),
                status_of(
                    key,
                    gates,
                    &by_key,
                    &verdict_of,
                    r.inert,
                    r.construct.as_ref(),
                ),
                guards_text(r.guards.as_ref()),
                need.join(", "),
            ]
        })
        .collect();
    let mut lines = vec![
        "| key | kind | family | mods | status | guards | needs_bridge |".to_owned(),
        "|---|---|---|---|---|---|---|".to_owned(),
    ];
    for row in &rows {
        let cells: Vec<String> = row
            .iter()
            .enumerate()
            .map(|(i, c)| if i == 0 { format!("`{c}`") } else { c.clone() })
            .collect();
        lines.push(format!("| {} |", cells.join(" | ")));
    }
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &rows {
        *counts
            .entry(row[4].split(' ').next().unwrap_or_default())
            .or_insert(0) += 1;
    }
    let covered: usize = counts
        .iter()
        .filter(|(k, _)| **k != "UNGATED")
        .map(|(_, n)| n)
        .sum();
    let listed: Vec<String> = counts.iter().map(|(k, n)| format!("{n} {k}")).collect();
    let mut out = vec![
        format!(
            "{} routes, {covered} covered: {}",
            rows.len(),
            listed.join(", ")
        ),
        String::new(),
    ];
    out.extend(lines);
    let live: Vec<&String> = report
        .routes
        .iter()
        .filter(|(_, r)| !r.inert && r.construct.is_none())
        .map(|(k, _)| k)
        .collect();
    out.extend([String::new(), m2("install", &live, gates)]);
    out
}

fn python_dumps_counts(totals: &BTreeMap<String, usize>) -> String {
    let items: Vec<String> = totals
        .iter()
        .map(|(k, v)| {
            let mut key = String::new();
            crate::pytext::dump_str(k, &mut key);
            format!("{key}: {v}")
        })
        .collect();
    format!("{{{}}}", items.join(", "))
}

const TOP: usize = 40;

/// Port of mod_replaces.markdown: `replaces.md`.
pub fn markdown(r: &Replaces, install: &Path, report: &Report, gates: &Gates) -> String {
    let mut lines = vec![
        format!(
            "# Vanilla behaviour the mods of {} change",
            install
                .components()
                .collect::<std::path::PathBuf>()
                .display()
        ),
        String::new(),
        format!("Totals: {}", python_dumps_counts(&r.totals)),
        String::new(),
        "## Per mod".to_owned(),
        String::new(),
    ];
    let counted = [
        "mixin",
        "access",
        "override",
        "implement",
        "listener",
        "data",
        "access_transformer",
    ];
    let rows: Vec<Vec<String>> = r
        .mods
        .iter()
        .map(|(m, s)| {
            let mut row = vec![m.clone()];
            row.extend(
                counted
                    .iter()
                    .map(|k| s.counts.get(*k).copied().unwrap_or(0).to_string()),
            );
            row.extend(ROUTING_ORDER.iter().map(|o| {
                s.routing
                    .iter()
                    .find(|(k, _)| k == o)
                    .map_or(0, |(_, n)| *n)
                    .to_string()
            }));
            row.push(if s.can_analyse_ahead { "yes" } else { "no" }.to_owned());
            row
        })
        .collect();
    let mut headers = vec![
        "mod",
        "mixin",
        "access",
        "override",
        "implement",
        "listener",
        "data files",
        "AT lines",
    ];
    headers.extend(ROUTING_ORDER);
    headers.push("analysable");
    lines.extend(table(&headers, &rows));
    let opaque: Vec<String> = r
        .mods
        .iter()
        .filter(|(_, s)| !s.opaque.is_empty())
        .map(|(m, s)| format!("- {m}: {}", s.opaque.join("; ")))
        .collect();
    if !opaque.is_empty() {
        lines.extend([
            String::new(),
            "## Can't be analysed ahead".to_owned(),
            String::new(),
        ]);
        lines.extend(opaque);
    }
    let mut ranked: Vec<(&String, &crate::replaces::MethodRow)> = r
        .methods
        .iter()
        .filter(|(_, v)| v.vanilla && v.routing != "inert")
        .collect();
    ranked.sort_by(|a, b| b.1.mods.len().cmp(&a.1.mods.len()).then(a.0.cmp(b.0)));
    ranked.truncate(TOP);
    lines.extend([
        String::new(),
        format!("## Top {TOP} vanilla methods by number of mods that change them"),
        String::new(),
    ]);
    let rows: Vec<Vec<String>> = ranked
        .iter()
        .map(|(k, v)| {
            let mechanisms: BTreeSet<&str> =
                v.changes.iter().map(|c| c.mechanism.as_str()).collect();
            vec![
                format!("`{}`", short(k)),
                v.mods.len().to_string(),
                mechanisms.into_iter().collect::<Vec<_>>().join(", "),
                v.routing.clone(),
                v.native.status.clone(),
            ]
        })
        .collect();
    lines.extend(table(
        &["method", "mods", "mechanisms", "routing", "native"],
        &rows,
    ));
    let routes = &report.routes;
    let edge = routes.values().filter(|r| r.kind == "edge").count();
    let body = routes.values().filter(|r| r.kind == "body").count();
    let hot = routes.values().filter(|r| r.hot).count();
    lines.extend([
        String::new(),
        "## Routes".to_owned(),
        String::new(),
        format!(
            "{} routes: {edge} edge, {body} body; {} undecided; {} folded lambdas; {hot} hot",
            routes.len(),
            report.undecided.len(),
            report.lambdas.len()
        ),
    ]);
    let need: Vec<Vec<String>> = routes
        .iter()
        .filter(|(_, r)| !r.closure.needs_bridge.is_empty())
        .map(|(k, r)| {
            let names: Vec<String> = r
                .closure
                .needs_bridge
                .iter()
                .map(|n| format!("`{}`", short(n)))
                .collect();
            vec![format!("`{}`", short(k)), names.join(", ")]
        })
        .collect();
    if !need.is_empty() {
        lines.extend([
            String::new(),
            "Body routes whose vanilla callees native owns and no bridge covers:".to_owned(),
            String::new(),
        ]);
        lines.extend(table(&["route", "needs a bridge"], &need));
    }
    lines.extend([
        String::new(),
        "Gates (tools/java_routing.py):".to_owned(),
        String::new(),
    ]);
    lines.extend(report_table(report, gates));
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Writes `replaces.md` beside `replaces.json`.
pub fn write_markdown(
    out: &Path,
    r: &Replaces,
    install: &Path,
    report: &Report,
    gates: &Gates,
) -> Result<(), String> {
    let path = out.join("replaces.md");
    std::fs::write(&path, markdown(r, install, report, gates))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads `owner \t name \t desc` lines: the bridges the game compiles in.
pub fn read_bridges(text: &str) -> Bridges {
    text.lines()
        .filter_map(|l| {
            let mut parts = l.split('\t');
            let (o, n, d) = (parts.next()?, parts.next()?, parts.next()?);
            Some((o.to_owned(), n.to_owned(), d.to_owned()))
        })
        .collect()
}

fn pairs(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let (k, r) = l.split_once('\t').unwrap_or((l, ""));
            (k.to_owned(), r.to_owned())
        })
        .collect()
}

/// Reads a gated key per line, `key \t reason` deferral lines and `key \t verdict` verdict lines.
pub fn read_gates(gated: &str, deferred: &str, verdicts: &str) -> Gates {
    Gates {
        gated: gated
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect(),
        deferred: pairs(deferred),
        verdicts: pairs(verdicts),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptors_split_into_parameters_and_return() {
        assert_eq!(
            desc_parts("(I[[JLjava/lang/String;[Lx/Y;)Z"),
            (
                vec![
                    "I".to_owned(),
                    "[[J".to_owned(),
                    "Ljava/lang/String;".to_owned(),
                    "[Lx/Y;".to_owned()
                ],
                "Z".to_owned()
            )
        );
        assert_eq!(desc_parts("()V"), (Vec::new(), "V".to_owned()));
    }

    #[test]
    fn keys_split_with_and_without_a_descriptor() {
        assert_eq!(
            split_key("a/B#m(I)V"),
            ("a/B".to_owned(), "m".to_owned(), Some("(I)V".to_owned()))
        );
        assert_eq!(split_key("a/B#m"), ("a/B".to_owned(), "m".to_owned(), None));
        assert_eq!(short("net/minecraft/world/A#b()V"), "A#b()V");
    }

    #[test]
    fn the_view_list_is_read_from_view_inits() {
        let views = view_classes();
        assert!(views.contains("net/minecraft/server/level/ChunkMap"));
        assert!(views.contains("net/minecraft/client/Minecraft"));
        let text = "x VIEWS; static final Set<String> VIEWS = Set.of(\"a/B\", \"\", \"c/D\");";
        assert_eq!(
            view_classes_of(text),
            BTreeSet::from(["a/B".to_owned(), "c/D".to_owned()])
        );
    }

    fn route(kind: &str, tail: bool, returns: usize) -> Route {
        Route {
            kind: kind.to_owned(),
            is_static: false,
            family: "value".to_owned(),
            args: Vec::new(),
            ret: "V".to_owned(),
            changes: Vec::new(),
            returns,
            tail,
            overriders: Vec::new(),
            guards: None,
            closure: Closure::default(),
            hot: false,
            side: "common".to_owned(),
            inert: false,
            construct: None,
            verdict: None,
        }
    }

    #[test]
    fn tsv_of_two_routes_is_two_lines_with_a_final_newline() {
        let mut report = Report::default();
        report
            .routes
            .insert("a/B#m(I)V".to_owned(), route("edge", true, 2));
        report
            .routes
            .insert("a/C#n()Z".to_owned(), route("body", false, 1));
        assert_eq!(
            tsv(&report),
            "a/B\tm\t(I)V\tedge\t1\t2\na/C\tn\t()Z\tbody\t0\t1\n"
        );
        assert_eq!(tsv(&Report::default()), "");
    }

    #[test]
    fn route_guards_union_classes_and_intersect_argument_indexes() {
        let one = Guards([
            Some(GuardValue::Classes(vec!["b".to_owned()])),
            None,
            Some(GuardValue::Indexed(vec![
                ("1".to_owned(), vec!["x".to_owned()]),
                ("0".to_owned(), vec!["y".to_owned()]),
            ])),
            None,
            None,
            None,
            None,
        ]);
        let two = Guards([
            Some(GuardValue::Classes(vec!["a".to_owned()])),
            None,
            Some(GuardValue::Indexed(vec![(
                "1".to_owned(),
                vec!["w".to_owned()],
            )])),
            None,
            None,
            None,
            None,
        ]);
        let both = route_guards(&[Some(one.clone()), Some(two)]).unwrap();
        assert_eq!(
            both.get("recv"),
            Some(&GuardValue::Classes(vec!["a".to_owned(), "b".to_owned()]))
        );
        assert_eq!(
            both.get("arg_item"),
            Some(&GuardValue::Indexed(vec![(
                "1".to_owned(),
                vec!["w".to_owned(), "x".to_owned()]
            )]))
        );
        assert_eq!(route_guards(&[Some(one), None]), None);
        assert_eq!(guards_text(Some(&both)), "recv: a, b; arg_item[1]: w, x");
        assert_eq!(guards_text(None), "all");
    }
}
