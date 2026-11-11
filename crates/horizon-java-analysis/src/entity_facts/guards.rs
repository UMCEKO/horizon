//! What a listener puts on its entity before doing anything else: side, required and excluded types, and the entity type ids.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::rc::Rc;

use serde::Serialize;

use super::bytecode::{
    CHECKCAST, DUP, GETSTATIC, IF_ACMPNE, IFEQ, IFNE, INSTANCEOF, INVOKEINTERFACE, INVOKEVIRTUAL,
    Instruction, LDC, LDC_W, POP, PUTSTATIC, clinit, instructions, is_aload, is_astore, is_exit,
    read_library_class, slot_of_last,
};
use crate::class::{ClassError, ClassInfo, Pool, PoolEntry, params};
use crate::lambdas::Impl;

pub const TYPES_CLASS: &str = "net/minecraft/world/entity/EntityTypes";
pub const EVENT_PREFIXES: [&str; 3] = [
    "net/neoforged/neoforge/event/tick/Entity",
    "net/neoforged/neoforge/client/event/RenderLiving",
    "net/neoforged/neoforge/client/event/RenderNameTag",
];
pub const ENTITY_EVENT: &str = "net/neoforged/neoforge/event/entity/EntityEvent";
pub const TYPE_IDS_CLASS: &str = "net/minecraft/world/entity/EntityTypeIds";

/// The guards of one listener: where it runs and which entities it can see.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Guard {
    pub side: String,
    pub require: Vec<String>,
    pub exclude: Vec<String>,
    pub types: Option<Vec<String>>,
}

impl Guard {
    /// Port of mod_entity_facts.ALL_GUARD.
    pub fn all() -> Guard {
        Guard {
            side: "both".to_owned(),
            require: Vec::new(),
            exclude: Vec::new(),
            types: None,
        }
    }
}

/// What the symbolic walk knows about a stack or local value.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Val {
    Event,
    Entity,
    Level,
    ClientSide,
    Type,
    Instance(String),
    TypeId(String),
}

fn string_constant(pool: &Pool, operand: i64) -> Option<String> {
    let index = usize::try_from(operand).ok()?;
    match pool.entry(index).ok()? {
        PoolEntry::String(n) => pool.utf(*n).ok(),
        _ => None,
    }
}

/// Port of mod_entity_facts.type_ids.
pub fn type_ids(jars: &[PathBuf]) -> Result<BTreeMap<String, String>, String> {
    let holder = read_library_class(jars, TYPES_CLASS)?;
    let ids = read_library_class(jars, TYPE_IDS_CLASS)?;
    let Some(holder) = holder else {
        return Ok(BTreeMap::new());
    };
    let e = |e: ClassError| e.to_string();
    let mut names: HashMap<String, String> = HashMap::new();
    if let Some(ids) = ids {
        let (pool, ins) = clinit(&ids).map_err(e)?;
        let mut last: Option<String> = None;
        for i in &ins {
            let operand = i.operand.unwrap_or(0);
            if matches!(i.op, LDC | LDC_W) && string_constant(&pool, operand).is_some() {
                last = string_constant(&pool, operand);
            } else if i.op == PUTSTATIC
                && let Some(name) = last.take()
            {
                names.insert(pool.reference(operand as u16).map_err(e)?.1, name);
            }
        }
    }
    let (pool, ins) = clinit(&holder).map_err(e)?;
    let mut found = BTreeMap::new();
    let (mut key, mut string): (Option<String>, Option<String>) = (None, None);
    for i in &ins {
        let operand = i.operand.unwrap_or(0);
        if i.op == GETSTATIC && pool.reference(operand as u16).map_err(e)?.0 == TYPE_IDS_CLASS {
            key = Some(pool.reference(operand as u16).map_err(e)?.1);
        } else if matches!(i.op, LDC | LDC_W) && string_constant(&pool, operand).is_some() {
            string = string_constant(&pool, operand);
        } else if i.op == PUTSTATIC {
            let (owner, field, _) = pool.reference(operand as u16).map_err(e)?;
            if owner == TYPES_CLASS {
                let name = match &key {
                    Some(k) => names.get(k).cloned(),
                    None => string.clone(),
                };
                if let Some(name) = name.filter(|n| !n.is_empty()) {
                    let id = if name.contains(':') {
                        name
                    } else {
                        format!("minecraft:{name}")
                    };
                    found.insert(field, id);
                }
                key = None;
                string = None;
            }
        }
    }
    Ok(found)
}

/// Port of mod_entity_facts.walk_guards.
pub fn walk_guards(
    code: &[u8],
    pool: &Pool,
    event_slot: i64,
    ids: &BTreeMap<String, String>,
) -> Result<Guard, ClassError> {
    let ins = instructions(code)?;
    let by_offset: HashMap<usize, Instruction> = ins.iter().map(|i| (i.offset, *i)).collect();
    let mut side = "both";
    let mut require: BTreeSet<String> = BTreeSet::new();
    let mut exclude: BTreeSet<String> = BTreeSet::new();
    let mut types: Option<Vec<String>> = None;
    let mut stack: Vec<Option<Val>> = Vec::new();
    let mut local: HashMap<i64, Option<Val>> = HashMap::from([(event_slot, Some(Val::Event))]);
    for i in &ins {
        let op = i.op;
        let operand = i.operand.unwrap_or(0);
        let index = operand as u16;
        if is_aload(op) {
            stack.push(local.get(&operand).cloned().flatten());
        } else if is_astore(op) {
            let value = stack.pop().flatten();
            local.insert(operand, value);
        } else if op == DUP {
            let Some(top) = stack.last().cloned() else {
                break;
            };
            stack.push(top);
        } else if op == POP {
            if stack.pop().is_none() {
                break;
            }
        } else if op == CHECKCAST {
            if stack.is_empty() {
                break;
            }
        } else if op == INSTANCEOF {
            if stack.pop().flatten() != Some(Val::Entity) {
                break;
            }
            stack.push(Some(Val::Instance(pool.cls(index)?.unwrap_or_default())));
        } else if op == GETSTATIC {
            let (owner, name, _) = pool.reference(index)?;
            match ids.get(&name) {
                Some(id) if owner == TYPES_CLASS => stack.push(Some(Val::TypeId(id.clone()))),
                _ => break,
            }
        } else if op == INVOKEVIRTUAL || op == INVOKEINTERFACE {
            let (_, name, desc) = pool.reference(index)?;
            let receiver = stack.pop().flatten();
            let no_args = desc.starts_with("()");
            if name == "getEntity" && no_args && receiver == Some(Val::Event) {
                stack.push(Some(Val::Entity));
            } else if (name == "level" && no_args && receiver == Some(Val::Entity))
                || (name == "getLevel" && no_args && receiver == Some(Val::Event))
            {
                stack.push(Some(Val::Level));
            } else if name == "isClientSide" && desc == "()Z" && receiver == Some(Val::Level) {
                stack.push(Some(Val::ClientSide));
            } else if name == "getType" && no_args && receiver == Some(Val::Entity) {
                stack.push(Some(Val::Type));
            } else {
                break;
            }
        } else if op == IFEQ || op == IFNE {
            let value = stack.pop().flatten();
            if !is_exit(&by_offset, operand) {
                break;
            }
            match value {
                Some(Val::ClientSide) => side = if op == IFNE { "server" } else { "client" },
                Some(Val::Instance(class)) => {
                    if op == IFEQ {
                        &mut require
                    } else {
                        &mut exclude
                    }
                    .insert(class);
                }
                _ => break,
            }
        } else if op == IF_ACMPNE {
            let pair = [stack.pop().flatten(), stack.pop().flatten()];
            let known: Vec<&String> = pair
                .iter()
                .filter_map(|v| match v {
                    Some(Val::TypeId(id)) => Some(id),
                    _ => None,
                })
                .collect();
            if !is_exit(&by_offset, operand) || known.len() != 1 || !pair.contains(&Some(Val::Type))
            {
                break;
            }
            let id = known[0].clone();
            types = Some(match types {
                None => vec![id],
                Some(t) => t.into_iter().filter(|t| *t == id).collect(),
            });
        } else {
            break;
        }
    }
    Ok(Guard {
        side: side.to_owned(),
        require: require.into_iter().collect(),
        exclude: exclude.into_iter().collect(),
        types,
    })
}

/// Port of mod_entity_facts.listener_guards.
pub fn listener_guards(
    infos: &dyn Fn(&str) -> Option<Rc<ClassInfo>>,
    implementation: Option<&Impl>,
    ids: &BTreeMap<String, String>,
) -> Result<Option<Guard>, ClassError> {
    let Some((class, name, desc, is_static)) = implementation else {
        return Ok(None);
    };
    let Some(info) = infos(class) else {
        return Ok(None);
    };
    let method = info
        .methods
        .iter()
        .find(|m| m.name == *name && m.desc == *desc && m.code.is_some());
    let Some(code) = method.and_then(|m| m.code.as_deref()) else {
        return Ok(None);
    };
    if params(desc).is_empty() {
        return Ok(None);
    }
    walk_guards(code, &info.pool, slot_of_last(desc, *is_static), ids).map(Some)
}

/// Port of mod_entity_facts.subjects: `None` is `all`, else the distinct guards in listener order.
pub fn subjects(listeners: &[Guard]) -> Option<Vec<Guard>> {
    if listeners.iter().any(|g| *g == Guard::all()) {
        return None;
    }
    let mut unique: Vec<Guard> = Vec::new();
    for g in listeners {
        if !unique.contains(g) {
            unique.push(g.clone());
        }
    }
    Some(unique)
}
