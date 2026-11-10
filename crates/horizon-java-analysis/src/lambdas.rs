//! Lambdas: the event a listener lambda handles, the method it runs, and vanilla's own lambdas folded into their enclosing methods.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::rc::Rc;

use zip::ZipArchive;

use crate::class::{self, ClassInfo, Event, Parse, PoolEntry};
use crate::hierarchy::Hierarchy;
use crate::libraries::entry_names;

const LAMBDA_FACTORY: &str = "java/lang/invoke/LambdaMetafactory";
const RETURNS: std::ops::RangeInclusive<u8> = 0xac..=0xb1;
const INVOKESTATIC: u8 = 0xb8;
const INVOKEVIRTUAL: u8 = 0xb6;
const REF_INVOKE_STATIC: u8 = 6;

/// A method a lambda or method reference runs: class, name, descriptor, static.
pub type Impl = (String, String, String, bool);

/// A call of a method body: opcode, owner, name, descriptor.
pub type Call = (u8, String, String, String);

/// Port of mod_replaces.lambda_event.
pub fn lambda_event(info: &ClassInfo, bootstrap: usize) -> Option<String> {
    let pool = &info.pool;
    let args = &info.bootstraps.get(bootstrap)?.1;
    if args.len() < 3 {
        return None;
    }
    let PoolEntry::MethodType(desc) = pool.entry(usize::from(args[2])).ok()? else {
        return None;
    };
    let instantiated = class::params(&pool.utf(*desc).ok()?);
    if let Some(first) = instantiated.first()
        && first != "java/lang/Object"
        && !first.starts_with('[')
    {
        return Some(first.clone());
    }
    if let PoolEntry::MethodHandle { reference, .. } = pool.entry(usize::from(args[1])).ok()? {
        let target = class::params(&pool.reference(*reference).ok()?.2);
        if let Some(last) = target.last()
            && last.len() > 1
            && !last.starts_with('[')
        {
            return Some(last.clone());
        }
    }
    None
}

/// Port of mod_replaces.lambda_impl.
pub fn lambda_impl(info: &ClassInfo, bootstrap: usize) -> Option<Impl> {
    let args = &info.bootstraps.get(bootstrap)?.1;
    let PoolEntry::MethodHandle { kind, reference } =
        info.pool.entry(usize::from(*args.get(1)?)).ok()?
    else {
        return None;
    };
    let (owner, name, desc) = info.pool.reference(*reference).ok()?;
    Some((owner, name, desc, *kind == REF_INVOKE_STATIC))
}

/// Port of mod_routing.lambda_target.
pub fn lambda_target(info: &ClassInfo, bootstrap: usize) -> Option<Impl> {
    let (handle, _) = info.bootstraps.get(bootstrap)?;
    let PoolEntry::MethodHandle { reference, .. } = info.pool.entry(usize::from(*handle)).ok()?
    else {
        return None;
    };
    if info.pool.reference(*reference).ok()?.0 != LAMBDA_FACTORY {
        return None;
    }
    lambda_impl(info, bootstrap)
}

type Methods = HashMap<(String, String), usize>;

/// Port of mod_routing.Vanilla.
pub struct Vanilla {
    zips: Vec<RefCell<ZipArchive<File>>>,
    index: HashMap<String, (usize, String)>,
    cache: RefCell<HashMap<String, Option<Rc<ClassInfo>>>>,
    methods: RefCell<HashMap<String, Rc<Methods>>>,
    code: RefCell<HashMap<(String, String, String), Rc<(Vec<Call>, usize)>>>,
}

impl Vanilla {
    /// Port of mod_routing.Vanilla.__init__.
    pub fn new(jars: &[PathBuf]) -> Result<Vanilla, String> {
        let mut zips = Vec::new();
        let mut index: HashMap<String, (usize, String)> = HashMap::new();
        for jar in jars {
            let file = File::open(jar).map_err(|e| format!("{}: {e}", jar.display()))?;
            let mut zip = ZipArchive::new(file).map_err(|e| format!("{}: {e}", jar.display()))?;
            for entry in entry_names(&mut zip)? {
                if let Some(name) = entry.strip_suffix(".class")
                    && class::is_vanilla(name)
                {
                    index
                        .entry(name.to_owned())
                        .or_insert_with(|| (zips.len(), entry.clone()));
                }
            }
            zips.push(RefCell::new(zip));
        }
        Ok(Vanilla {
            zips,
            index,
            cache: RefCell::new(HashMap::new()),
            methods: RefCell::new(HashMap::new()),
            code: RefCell::new(HashMap::new()),
        })
    }

    /// Port of mod_routing.Vanilla.info.
    pub fn info(&self, name: &str) -> Option<Rc<ClassInfo>> {
        if let Some(found) = self.cache.borrow().get(name) {
            return found.clone();
        }
        let parsed = self.index.get(name).and_then(|(zip, entry)| {
            let mut zip = self.zips[*zip].borrow_mut();
            let mut data = Vec::new();
            zip.by_name(entry).ok()?.read_to_end(&mut data).ok()?;
            let what = Parse {
                members: true,
                annotations: true,
                code: true,
            };
            class::parse_class(&data, what).ok().map(Rc::new)
        });
        self.cache
            .borrow_mut()
            .insert(name.to_owned(), parsed.clone());
        parsed
    }

    /// Port of mod_routing.Vanilla.method (the index into the class's methods; the last declaration wins).
    pub fn method(&self, cls: &str, name: &str, desc: &str) -> Option<usize> {
        let cached = self.methods.borrow().get(cls).cloned();
        let table = cached.unwrap_or_else(|| {
            let mut table = Methods::new();
            if let Some(info) = self.info(cls) {
                for (i, m) in info.methods.iter().enumerate() {
                    table.insert((m.name.clone(), m.desc.clone()), i);
                }
            }
            let table = Rc::new(table);
            self.methods
                .borrow_mut()
                .insert(cls.to_owned(), table.clone());
            table
        });
        table.get(&(name.to_owned(), desc.to_owned())).copied()
    }

    /// Port of mod_routing.Vanilla.ops.
    pub fn ops(&self, cls: &str, name: &str, desc: &str) -> Rc<(Vec<Call>, usize)> {
        let key = (cls.to_owned(), name.to_owned(), desc.to_owned());
        if let Some(found) = self.code.borrow().get(&key) {
            return found.clone();
        }
        let (mut calls, mut returns) = (Vec::new(), 0);
        if let (Some(info), Some(i)) = (self.info(cls), self.method(cls, name, desc))
            && let Some(code) = &info.methods[i].code
        {
            let mut p = 0;
            while p < code.len() {
                let op = code[p];
                if (0xb6..=0xb9).contains(&op) {
                    let reference = class::u2(code, p + 1)
                        .ok()
                        .and_then(|r| info.pool.reference(r).ok());
                    if let Some((owner, name, desc)) = reference {
                        calls.push((op, owner, name, desc));
                    }
                } else if op == 0xba {
                    let entry = class::u2(code, p + 1)
                        .ok()
                        .and_then(|r| info.pool.entry(usize::from(r)).ok());
                    if let Some(PoolEntry::Dynamic { bootstrap, .. }) = entry
                        && let Some((owner, name, desc, is_static)) =
                            lambda_target(&info, usize::from(*bootstrap))
                    {
                        let op = if is_static {
                            INVOKESTATIC
                        } else {
                            INVOKEVIRTUAL
                        };
                        calls.push((op, owner, name, desc));
                    }
                } else if RETURNS.contains(&op) {
                    returns += 1;
                }
                match class::op_length(code, p) {
                    Ok(n) => p += n,
                    Err(_) => break,
                }
            }
        }
        let out = Rc::new((calls, returns));
        self.code.borrow_mut().insert(key, out.clone());
        out
    }
}

/// Port of mod_routing.enclosing.
pub fn enclosing(
    vanilla: &Vanilla,
    cls: &str,
    name: &str,
    desc: Option<&str>,
) -> Option<(String, String)> {
    let info = vanilla.info(cls);
    let mut seen: HashSet<(String, Option<String>)> = HashSet::new();
    let (mut name, mut desc) = (name.to_owned(), desc.map(str::to_owned));
    let mut found = None;
    while name.starts_with("lambda$")
        && !seen.contains(&(name.clone(), desc.clone()))
        && let Some(info) = &info
    {
        seen.insert((name.clone(), desc.clone()));
        let hit = info.methods.iter().find(|m| {
            m.events.iter().any(|ev| {
                let Event::Indy { bootstrap, .. } = ev else {
                    return false;
                };
                lambda_target(info, usize::from(*bootstrap)).is_some_and(|imp| {
                    imp.0 == cls && imp.1 == name && desc.as_ref().is_none_or(|d| *d == imp.2)
                })
            })
        })?;
        found = Some((hit.name.clone(), hit.desc.clone()));
        (name, desc) = (hit.name.clone(), Some(hit.desc.clone()));
    }
    found
}

/// The fields of a change record `fold_lambdas` reads and writes; the rest of the record is SPEC-8's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Change {
    pub desc: Option<String>,
    pub folded: Option<String>,
}

/// Port of mod_routing.fold_lambdas.
pub fn fold_lambdas(
    records: Vec<(String, bool, Change)>,
    hier: &Hierarchy,
    vanilla: &Vanilla,
) -> Vec<(String, bool, Change)> {
    records
        .into_iter()
        .map(|(key, is_vanilla, record)| {
            let (owner, name) = key.split_once('#').unwrap_or((&key, ""));
            if is_vanilla && name.starts_with("lambda$") {
                let lam_desc = record.desc.clone().filter(|d| !d.is_empty()).or_else(|| {
                    hier.methods_of(owner)
                        .iter()
                        .find(|m| m.0 == name)
                        .map(|m| m.1.clone())
                });
                if let Some((hit_name, hit_desc)) =
                    enclosing(vanilla, owner, name, lam_desc.as_deref())
                {
                    let folded = format!("{name}{}", lam_desc.as_deref().unwrap_or(""));
                    let record = Change {
                        desc: Some(hit_desc),
                        folded: Some(folded),
                    };
                    return (format!("{owner}#{hit_name}"), is_vanilla, record);
                }
            }
            (key, is_vanilla, record)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::path::Path;
    use std::process::Command;

    fn compile(dir: &Path) -> Option<PathBuf> {
        let home = std::env::var_os("JAVA_HOME")?;
        let javac = Path::new(&home).join("bin").join("javac");
        if !javac.is_file() {
            return None;
        }
        let src = dir.join("V.java");
        std::fs::write(
            &src,
            "package net.minecraft.x; class V { void outer() { Runnable r = () -> { Runnable s = () -> inner(); }; } void inner() {} }",
        )
        .unwrap();
        let out = dir.join("classes");
        let status = Command::new(javac)
            .args(["-proc:none", "-nowarn", "-d"])
            .arg(&out)
            .arg(&src)
            .status()
            .unwrap();
        assert!(status.success());
        let jar = dir.join("v.jar");
        let mut zip = zip::ZipWriter::new(File::create(&jar).unwrap());
        zip.start_file(
            "net/minecraft/x/V.class",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&std::fs::read(out.join("net/minecraft/x/V.class")).unwrap())
            .unwrap();
        zip.finish().unwrap();
        Some(jar)
    }

    #[test]
    fn nested_lambdas_fold_into_the_method_that_holds_them() {
        let dir = tempfile::tempdir().unwrap();
        let Some(jar) = compile(dir.path()) else {
            eprintln!("skipped: no JAVA_HOME");
            return;
        };
        let vanilla = Vanilla::new(&[jar]).unwrap();
        let info = vanilla.info("net/minecraft/x/V").unwrap();
        let lambdas: Vec<&str> = info
            .methods
            .iter()
            .filter(|m| m.name.starts_with("lambda$"))
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(lambdas.len(), 2, "{lambdas:?}");
        for name in lambdas {
            assert_eq!(
                enclosing(&vanilla, "net/minecraft/x/V", name, None),
                Some(("outer".to_owned(), "()V".to_owned())),
                "{name}"
            );
        }
        assert_eq!(
            enclosing(&vanilla, "net/minecraft/x/V", "outer", None),
            None
        );
        let folded = fold_lambdas(
            vec![(
                "net/minecraft/x/V#lambda$outer$1".to_owned(),
                true,
                Change::default(),
            )],
            &Hierarchy::new(Default::default()),
            &vanilla,
        );
        assert_eq!(folded[0].0, "net/minecraft/x/V#outer");
        assert_eq!(folded[0].2.desc.as_deref(), Some("()V"));
        assert_eq!(folded[0].2.folded.as_deref(), Some("lambda$outer$1"));
    }
}
