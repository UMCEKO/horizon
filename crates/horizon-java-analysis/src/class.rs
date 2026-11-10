//! A JVM class-file reader: members, annotations and the calls, field reads and lambdas of each method's code.

use serde_json::{Map, Value as Json, json};

/// What went wrong reading a class file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassError(pub String);

impl std::fmt::Display for ClassError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ClassError {}

fn err<T>(message: impl Into<String>) -> Result<T, ClassError> {
    Err(ClassError(message.into()))
}

fn u1(d: &[u8], p: usize) -> Result<u8, ClassError> {
    match d.get(p) {
        Some(b) => Ok(*b),
        None => err(format!("read past the end at {p}")),
    }
}

/// Port of mod_replaces.u2.
pub fn u2(d: &[u8], p: usize) -> Result<u16, ClassError> {
    Ok((u16::from(u1(d, p)?) << 8) | u16::from(u1(d, p + 1)?))
}

/// Port of mod_replaces.u4.
pub fn u4(d: &[u8], p: usize) -> Result<u32, ClassError> {
    Ok((u32::from(u2(d, p)?) << 16) | u32::from(u2(d, p + 2)?))
}

fn i4(d: &[u8], p: usize) -> Result<i32, ClassError> {
    Ok(u4(d, p)? as i32)
}

fn u8_at(d: &[u8], p: usize) -> Result<u64, ClassError> {
    Ok((u64::from(u4(d, p)?) << 32) | u64::from(u4(d, p + 4)?))
}

#[derive(Debug, Clone, PartialEq)]
pub enum PoolEntry {
    Empty,
    Utf8(String),
    Integer(i32),
    Float(f32),
    Long(i64),
    Double(f64),
    Class(u16),
    String(u16),
    MethodType(u16),
    Module(u16),
    Package(u16),
    Ref { tag: u8, class: u16, nat: u16 },
    NameAndType { name: u16, desc: u16 },
    Dynamic { tag: u8, bootstrap: u16, nat: u16 },
    MethodHandle { kind: u8, reference: u16 },
}

/// The constant pool, by index (index 0 and the second slot of a long or double are `Empty`).
#[derive(Debug, Clone, PartialEq)]
pub struct Pool {
    entries: Vec<PoolEntry>,
    end: usize,
}

impl Pool {
    /// Port of mod_replaces.Pool.__init__.
    pub fn new(data: &[u8]) -> Result<Pool, ClassError> {
        let count = usize::from(u2(data, 8)?);
        let mut entries = vec![PoolEntry::Empty; count];
        let mut pos = 10;
        let mut i = 1;
        while i < count {
            let tag = u1(data, pos)?;
            match tag {
                1 => {
                    let n = usize::from(u2(data, pos + 1)?);
                    let Some(bytes) = data.get(pos + 3..pos + 3 + n) else {
                        return err("utf8 constant past the end");
                    };
                    entries[i] = PoolEntry::Utf8(String::from_utf8_lossy(bytes).into_owned());
                    pos += 3 + n;
                }
                3 => {
                    entries[i] = PoolEntry::Integer(i4(data, pos + 1)?);
                    pos += 5;
                }
                4 => {
                    entries[i] = PoolEntry::Float(f32::from_bits(u4(data, pos + 1)?));
                    pos += 5;
                }
                5 | 6 => {
                    let bits = u8_at(data, pos + 1)?;
                    entries[i] = if tag == 5 {
                        PoolEntry::Long(bits as i64)
                    } else {
                        PoolEntry::Double(f64::from_bits(bits))
                    };
                    pos += 9;
                    i += 1;
                }
                7 | 8 | 16 | 19 | 20 => {
                    let v = u2(data, pos + 1)?;
                    entries[i] = match tag {
                        7 => PoolEntry::Class(v),
                        8 => PoolEntry::String(v),
                        16 => PoolEntry::MethodType(v),
                        19 => PoolEntry::Module(v),
                        _ => PoolEntry::Package(v),
                    };
                    pos += 3;
                }
                9..=12 | 17 | 18 => {
                    let (a, b) = (u2(data, pos + 1)?, u2(data, pos + 3)?);
                    entries[i] = match tag {
                        12 => PoolEntry::NameAndType { name: a, desc: b },
                        17 | 18 => PoolEntry::Dynamic {
                            tag,
                            bootstrap: a,
                            nat: b,
                        },
                        _ => PoolEntry::Ref {
                            tag,
                            class: a,
                            nat: b,
                        },
                    };
                    pos += 5;
                }
                15 => {
                    entries[i] = PoolEntry::MethodHandle {
                        kind: u1(data, pos + 1)?,
                        reference: u2(data, pos + 2)?,
                    };
                    pos += 4;
                }
                _ => return err(format!("constant pool tag {tag}")),
            }
            i += 1;
        }
        Ok(Pool { entries, end: pos })
    }

    /// The entry at an index.
    pub fn entry(&self, i: usize) -> Result<&PoolEntry, ClassError> {
        match self.entries.get(i) {
            Some(e) => Ok(e),
            None => err(format!("constant pool index {i} out of range")),
        }
    }

    /// Port of mod_replaces.Pool.utf.
    pub fn utf(&self, i: u16) -> Result<String, ClassError> {
        match self.entry(usize::from(i))? {
            PoolEntry::Utf8(s) => Ok(s.clone()),
            other => err(format!("constant {i} is not utf8: {other:?}")),
        }
    }

    /// Port of mod_replaces.Pool.cls.
    pub fn cls(&self, i: u16) -> Result<Option<String>, ClassError> {
        if i == 0 {
            return Ok(None);
        }
        match self.entry(usize::from(i))? {
            PoolEntry::Class(n)
            | PoolEntry::String(n)
            | PoolEntry::MethodType(n)
            | PoolEntry::Module(n)
            | PoolEntry::Package(n) => Ok(Some(self.utf(*n)?)),
            other => err(format!("constant {i} names no class: {other:?}")),
        }
    }

    /// Port of mod_replaces.Pool.nat.
    pub fn nat(&self, i: u16) -> Result<(String, String), ClassError> {
        match self.entry(usize::from(i))? {
            PoolEntry::NameAndType { name, desc } => Ok((self.utf(*name)?, self.utf(*desc)?)),
            other => err(format!("constant {i} is not a name and type: {other:?}")),
        }
    }

    /// Port of mod_replaces.Pool.ref.
    pub fn reference(&self, i: u16) -> Result<(String, String, String), ClassError> {
        match self.entry(usize::from(i))? {
            PoolEntry::Ref { class, nat, .. } => {
                let owner = self.cls(*class)?.unwrap_or_default();
                let (name, desc) = self.nat(*nat)?;
                Ok((owner, name, desc))
            }
            other => err(format!("constant {i} is not a reference: {other:?}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    Str(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Str(String),
    Enum { desc: String, name: String },
    Class(String),
    Const { tag: char, value: Const },
    Anno(Annotation),
    Array(Vec<Value>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Annotation {
    pub desc: String,
    pub values: Vec<(String, Value)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Static {
        owner: String,
        name: String,
    },
    Class(String),
    Call {
        owner: String,
        name: String,
        desc: String,
    },
    Indy {
        bootstrap: u16,
        name: String,
        desc: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    pub access: u16,
    pub name: String,
    pub desc: String,
    pub annos: Vec<Annotation>,
    pub invisible: Vec<Annotation>,
    pub events: Vec<Event>,
    pub code: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassInfo {
    pub name: String,
    pub sup: Option<String>,
    pub ifaces: Vec<String>,
    pub access: u16,
    pub methods: Vec<Method>,
    pub annos: Vec<Annotation>,
    pub invisible: Vec<Annotation>,
    pub bootstraps: Vec<(u16, Vec<u16>)>,
    pub pool: Pool,
}

/// What `parse_class` reads beyond the class header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parse {
    pub members: bool,
    pub annotations: bool,
    pub code: bool,
}

impl Parse {
    pub fn all() -> Parse {
        Parse {
            members: true,
            annotations: true,
            code: true,
        }
    }
}

fn constant(pool: &Pool, tag: char, i: u16) -> Result<Const, ClassError> {
    Ok(match pool.entry(usize::from(i))? {
        PoolEntry::Integer(v) => Const::Int(*v),
        PoolEntry::Long(v) => Const::Long(*v),
        PoolEntry::Float(v) => Const::Float(*v),
        PoolEntry::Double(v) => Const::Double(*v),
        PoolEntry::Utf8(s) => Const::Str(s.clone()),
        other => return err(format!("element constant '{tag}' is {other:?}")),
    })
}

/// Port of mod_replaces.element.
fn element(d: &[u8], p: usize, pool: &Pool) -> Result<(Value, usize), ClassError> {
    let tag = char::from(u1(d, p)?);
    match tag {
        's' => Ok((Value::Str(pool.utf(u2(d, p + 1)?)?), p + 3)),
        'e' => Ok((
            Value::Enum {
                desc: pool.utf(u2(d, p + 1)?)?,
                name: pool.utf(u2(d, p + 3)?)?,
            },
            p + 5,
        )),
        'c' => Ok((Value::Class(pool.utf(u2(d, p + 1)?)?), p + 3)),
        '@' => {
            let (anno, next) = annotation(d, p + 1, pool)?;
            Ok((Value::Anno(anno), next))
        }
        '[' => {
            let count = u2(d, p + 1)?;
            let mut items = Vec::new();
            let mut p = p + 3;
            for _ in 0..count {
                let (v, next) = element(d, p, pool)?;
                items.push(v);
                p = next;
            }
            Ok((Value::Array(items), p))
        }
        'B' | 'C' | 'D' | 'F' | 'I' | 'J' | 'S' | 'Z' => Ok((
            Value::Const {
                tag,
                value: constant(pool, tag, u2(d, p + 1)?)?,
            },
            p + 3,
        )),
        other => err(format!("annotation element tag {other:?}")),
    }
}

/// Port of mod_replaces.annotation.
fn annotation(d: &[u8], p: usize, pool: &Pool) -> Result<(Annotation, usize), ClassError> {
    let desc = pool.utf(u2(d, p)?)?;
    let count = u2(d, p + 2)?;
    let mut p = p + 4;
    let mut values = Vec::new();
    for _ in 0..count {
        let name = pool.utf(u2(d, p)?)?;
        let (v, next) = element(d, p + 2, pool)?;
        values.push((name, v));
        p = next;
    }
    Ok((Annotation { desc, values }, p))
}

/// Port of mod_replaces.read_annotations.
fn read_annotations(d: &[u8], p: usize, pool: &Pool) -> Result<Vec<Annotation>, ClassError> {
    let count = u2(d, p)?;
    let mut p = p + 2;
    let mut found = Vec::new();
    for _ in 0..count {
        let (a, next) = annotation(d, p, pool)?;
        found.push(a);
        p = next;
    }
    Ok(found)
}

/// Port of mod_replaces.LENGTHS (the fixed instruction lengths; 1 for every opcode not listed).
fn fixed_length(op: u8) -> usize {
    match op {
        0x10 | 0x12 | 0x15..=0x19 | 0x36..=0x3a | 0xa9 | 0xbc => 2,
        0x11
        | 0x13
        | 0x14
        | 0x84
        | 0x99..=0xa8
        | 0xb2..=0xb8
        | 0xbb
        | 0xbd
        | 0xc0
        | 0xc1
        | 0xc6
        | 0xc7 => 3,
        0xc5 => 4,
        0xb9 | 0xba | 0xc8 | 0xc9 => 5,
        _ => 1,
    }
}

/// Port of mod_replaces.op_length.
pub fn op_length(code: &[u8], p: usize) -> Result<usize, ClassError> {
    let op = u1(code, p)?;
    let length = match op {
        0xaa | 0xab => {
            let pad = 1 + (4 - (p + 1) % 4) % 4;
            if op == 0xaa {
                let (low, high) = (
                    i64::from(i4(code, p + pad + 4)?),
                    i64::from(i4(code, p + pad + 8)?),
                );
                pad as i64 + 12 + 4 * (high - low + 1)
            } else {
                pad as i64 + 8 + 8 * i64::from(u4(code, p + pad + 4)?)
            }
        }
        0xc4 => {
            if u1(code, p + 1)? == 0x84 {
                6
            } else {
                4
            }
        }
        _ => fixed_length(op) as i64,
    };
    match usize::try_from(length) {
        Ok(n) if n > 0 => Ok(n),
        _ => err(format!("instruction at {p} has length {length}")),
    }
}

/// Port of mod_replaces.scan_code.
fn scan_code(code: &[u8], pool: &Pool) -> Result<Vec<Event>, ClassError> {
    let mut events = Vec::new();
    let mut p = 0;
    while p < code.len() {
        let op = code[p];
        match op {
            0x12 | 0x13 => {
                let index = if op == 0x12 {
                    usize::from(u1(code, p + 1)?)
                } else {
                    usize::from(u2(code, p + 1)?)
                };
                if let PoolEntry::Class(n) = pool.entry(index)? {
                    events.push(Event::Class(pool.utf(*n)?));
                }
            }
            0xb2 => {
                let (owner, name, _) = pool.reference(u2(code, p + 1)?)?;
                events.push(Event::Static { owner, name });
            }
            0xb6..=0xb9 => {
                let (owner, name, desc) = pool.reference(u2(code, p + 1)?)?;
                events.push(Event::Call { owner, name, desc });
            }
            0xba => match pool.entry(usize::from(u2(code, p + 1)?))? {
                PoolEntry::Dynamic { bootstrap, nat, .. } => {
                    let (name, desc) = pool.nat(*nat)?;
                    events.push(Event::Indy {
                        bootstrap: *bootstrap,
                        name,
                        desc,
                    });
                }
                other => return err(format!("invokedynamic constant is {other:?}")),
            },
            _ => {}
        }
        p += op_length(code, p)?;
    }
    Ok(events)
}

/// Port of mod_replaces.parse_class.
pub fn parse_class(data: &[u8], what: Parse) -> Result<ClassInfo, ClassError> {
    let pool = Pool::new(data)?;
    let mut p = pool.end;
    let access = u2(data, p)?;
    let this = u2(data, p + 2)?;
    let sup = u2(data, p + 4)?;
    let iface_count = usize::from(u2(data, p + 6)?);
    let mut ifaces = Vec::with_capacity(iface_count);
    for i in 0..iface_count {
        ifaces.push(pool.cls(u2(data, p + 8 + 2 * i)?)?.unwrap_or_default());
    }
    let Some(name) = pool.cls(this)? else {
        return err("class has no name");
    };
    let mut info = ClassInfo {
        name,
        sup: pool.cls(sup)?,
        ifaces,
        access,
        methods: Vec::new(),
        annos: Vec::new(),
        invisible: Vec::new(),
        bootstraps: Vec::new(),
        pool: pool.clone(),
    };
    if !what.members {
        return Ok(info);
    }
    p += 8 + 2 * iface_count;
    let fields = u2(data, p)?;
    p += 2;
    for _ in 0..fields {
        let attrs = u2(data, p + 6)?;
        p += 8;
        for _ in 0..attrs {
            p += 6 + u4(data, p + 2)? as usize;
        }
    }
    let count = u2(data, p)?;
    p += 2;
    for _ in 0..count {
        let mut m = Method {
            access: u2(data, p)?,
            name: pool.utf(u2(data, p + 2)?)?,
            desc: pool.utf(u2(data, p + 4)?)?,
            annos: Vec::new(),
            invisible: Vec::new(),
            events: Vec::new(),
            code: None,
        };
        let attrs = u2(data, p + 6)?;
        p += 8;
        for _ in 0..attrs {
            let kind = pool.utf(u2(data, p)?)?;
            let size = u4(data, p + 2)? as usize;
            if kind == "RuntimeVisibleAnnotations" && what.annotations {
                m.annos = read_annotations(data, p + 6, &pool)?;
            } else if kind == "RuntimeInvisibleAnnotations" && what.annotations {
                m.invisible = read_annotations(data, p + 6, &pool)?;
            } else if kind == "Code" && what.code {
                let length = u4(data, p + 10)? as usize;
                let Some(code) = data.get(p + 14..p + 14 + length) else {
                    return err("code past the end");
                };
                m.events = scan_code(code, &pool)?;
                m.code = Some(code.to_vec());
            }
            p += 6 + size;
        }
        info.methods.push(m);
    }
    let attrs = u2(data, p)?;
    p += 2;
    for _ in 0..attrs {
        let kind = pool.utf(u2(data, p)?)?;
        let size = u4(data, p + 2)? as usize;
        if kind == "RuntimeVisibleAnnotations" && what.annotations {
            info.annos = read_annotations(data, p + 6, &pool)?;
        } else if kind == "RuntimeInvisibleAnnotations" && what.annotations {
            info.invisible = read_annotations(data, p + 6, &pool)?;
        } else if kind == "BootstrapMethods" && what.code {
            let mut q = p + 8;
            for _ in 0..u2(data, p + 6)? {
                let n = usize::from(u2(data, q + 2)?);
                let mut args = Vec::with_capacity(n);
                for i in 0..n {
                    args.push(u2(data, q + 4 + 2 * i)?);
                }
                info.bootstraps.push((u2(data, q)?, args));
                q += 4 + 2 * n;
            }
        }
        p += 6 + size;
    }
    Ok(info)
}

/// Port of mod_replaces.params.
pub fn params(desc: &str) -> Vec<String> {
    let bytes = desc.as_bytes();
    let mut out = Vec::new();
    let mut i = 1;
    while i < bytes.len() && bytes[i] != b')' {
        let start = i;
        while i < bytes.len() && bytes[i] == b'[' {
            i += 1;
        }
        i = if bytes.get(i) == Some(&b'L') {
            bytes[i..]
                .iter()
                .position(|b| *b == b';')
                .map_or(bytes.len(), |n| i + n + 1)
        } else {
            i + 1
        };
        let t = &bytes[start..i.min(bytes.len())];
        let text = if t.first() == Some(&b'L') && t.len() >= 2 {
            &t[1..t.len() - 1]
        } else {
            t
        };
        out.push(String::from_utf8_lossy(text).into_owned());
    }
    out
}

/// Port of mod_replaces.is_extension.
pub fn is_extension(name: &str) -> bool {
    name.starts_with("net/neoforged/neoforge/")
        && name.contains("/extensions/I")
        && name.ends_with("Extension")
}

/// Port of mod_replaces.is_vanilla.
pub fn is_vanilla(name: &str) -> bool {
    name.starts_with("net/minecraft/") || name.starts_with("com/mojang/")
}

/// What Python's `element` returns for a value (enums as the constant's name, classes as their descriptor).
pub fn python_value(v: &Value) -> Json {
    match v {
        Value::Str(s) | Value::Class(s) => json!(s),
        Value::Enum { name, .. } => json!(name),
        Value::Const {
            tag: 'Z',
            value: Const::Int(i),
        } => json!(i),
        Value::Const { .. } => Json::Null,
        Value::Anno(a) => json!([a.desc, anno_values(a)]),
        Value::Array(items) => Json::Array(items.iter().map(python_value).collect()),
    }
}

/// An annotation's values as Python's dict: the last value of a repeated key.
fn anno_values(a: &Annotation) -> Json {
    let mut map = Map::new();
    for (name, v) in &a.values {
        map.insert(name.clone(), python_value(v));
    }
    Json::Object(map)
}

/// Python's `{descriptor: {name: value}}` for a list of annotations.
pub fn python_annotations(annos: &[Annotation]) -> Json {
    let mut map = Map::new();
    for a in annos {
        map.insert(a.desc.clone(), anno_values(a));
    }
    Json::Object(map)
}

fn python_event(e: &Event) -> Json {
    match e {
        Event::Static { owner, name } => json!(["static", owner, name]),
        Event::Class(name) => json!(["class", name]),
        Event::Call { owner, name, desc } => json!(["call", owner, name, desc]),
        Event::Indy {
            bootstrap,
            name,
            desc,
        } => json!(["indy", bootstrap, name, desc]),
    }
}

/// The dump shape of Python's parse_class result (pool and code left out).
pub fn dump(info: &ClassInfo) -> Json {
    let methods: Vec<Json> = info
        .methods
        .iter()
        .map(|m| {
            json!({
                "name": m.name,
                "desc": m.desc,
                "access": m.access,
                "annos": python_annotations(&m.annos),
                "events": m.events.iter().map(python_event).collect::<Vec<_>>(),
            })
        })
        .collect();
    json!({
        "name": info.name,
        "super": info.sup,
        "ifaces": info.ifaces,
        "access": info.access,
        "methods": methods,
        "annos": python_annotations(&info.annos),
        "bootstraps": info.bootstraps.iter().map(|(i, args)| json!([i, args])).collect::<Vec<_>>(),
    })
}
