//! The routes of the installed mods, confirmed at boot (`routes.bin`): which vanilla methods a loaded mod's mixin changes, and for which receivers.
use crate::wire::In;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub const VERSION: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RouteId(pub u32);

/// An edge route's mixins only surround the method's body; a body route's reach inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Edge,
    Body,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Admitted {
    All,
    /// The spec that adds this family's receivers decides.
    Pending,
    Only(HashSet<Box<str>>),
    /// The mods' guards prove no receiver native passes reaches a handler.
    None,
}

/// Java's ids of the block states a route admits.
#[derive(Debug, PartialEq, Eq)]
pub struct JavaStates(Box<[u64]>);

impl JavaStates {
    pub fn from_ids(ids: &[i32]) -> Self {
        let words = ids
            .iter()
            .map(|&id| id.max(0) as usize / 64 + 1)
            .max()
            .unwrap_or(0);
        let mut bits = vec![0u64; words];
        for &id in ids.iter().filter(|&&id| id >= 0) {
            bits[id as usize / 64] |= 1 << (id % 64);
        }
        JavaStates(bits.into())
    }

    pub fn contains(&self, java: i32) -> bool {
        java >= 0
            && self
                .0
                .get(java as usize / 64)
                .is_some_and(|word| word & (1 << (java % 64)) != 0)
    }
}

#[derive(Debug)]
pub struct Route {
    pub key: Box<str>,
    pub kind: Kind,
    pub family: Box<str>,
    pub on: bool,
    pub hot: bool,
    pub admitted: Admitted,
    /// The item ids a guard admits (the call's stack's item); `None` admits any.
    pub items: Option<HashSet<Box<str>>>,
    /// The block states a guard admits (the call's argument state, or the state at its position); `None` admits any.
    pub blocks: Option<JavaStates>,
}

#[derive(Debug)]
pub struct Routes {
    routes: Vec<Route>,
    ids: HashMap<Box<str>, RouteId>,
}

impl Routes {
    pub fn read(bytes: &[u8]) -> Result<Self, String> {
        const ENDED: &str = "the file ended early";
        let mut r = In(bytes);
        let version = r.int().ok_or(ENDED)?;
        if version != VERSION as i32 {
            return Err(format!("version {version}, expected {VERSION}"));
        }
        let n = r.int().ok_or(ENDED)?;
        let mut routes = Vec::new();
        let mut ids = HashMap::new();
        for i in 0..n.max(0) {
            let key: Box<str> = r.text().ok_or(ENDED)?.into();
            let kind = match r.byte().ok_or(ENDED)? {
                0 => Kind::Edge,
                1 => Kind::Body,
                other => return Err(format!("route {key}: kind {other}")),
            };
            let family = r.text().ok_or(ENDED)?.into();
            let on = r.bool().ok_or(ENDED)?;
            let hot = r.bool().ok_or(ENDED)?;
            let admitted = match r.int().ok_or(ENDED)? {
                -1 => Admitted::All,
                -2 => Admitted::Pending,
                -3 => Admitted::None,
                count if count >= 0 => {
                    let mut names = HashSet::new();
                    for _ in 0..count {
                        names.insert(Box::from(r.text().ok_or(ENDED)?));
                    }
                    Admitted::Only(names)
                }
                other => return Err(format!("route {key}: admitted {other}")),
            };
            let items = match r.int().ok_or(ENDED)? {
                -1 => None,
                count if count >= 0 => {
                    let mut ids = HashSet::new();
                    for _ in 0..count {
                        ids.insert(Box::from(r.text().ok_or(ENDED)?));
                    }
                    Some(ids)
                }
                other => return Err(format!("route {key}: items {other}")),
            };
            let blocks = match r.int().ok_or(ENDED)? {
                -1 => None,
                count if count >= 0 => {
                    let ids = (0..count)
                        .map(|_| r.int().ok_or(ENDED))
                        .collect::<Result<Vec<_>, _>>()?;
                    Some(JavaStates::from_ids(&ids))
                }
                other => return Err(format!("route {key}: blocks {other}")),
            };
            if on {
                ids.insert(key.clone(), RouteId(i as u32));
            }
            routes.push(Route {
                key,
                kind,
                family,
                on,
                hot,
                admitted,
                items,
                blocks,
            });
        }
        Ok(Routes { routes, ids })
    }

    /// The route of `key` if it is on in this boot.
    pub fn id(&self, key: &str) -> Option<RouteId> {
        self.ids.get(key).copied()
    }

    pub fn get(&self, id: RouteId) -> &Route {
        &self.routes[id.0 as usize]
    }

    pub fn iter(&self) -> impl Iterator<Item = (RouteId, &Route)> {
        self.routes
            .iter()
            .enumerate()
            .map(|(i, r)| (RouteId(i as u32), r))
    }
}

static ROUTES: OnceLock<Routes> = OnceLock::new();

pub(crate) fn set(routes: Routes) {
    let _ = ROUTES.set(routes);
}

/// The boot's routes; `None` before the boot finished or without Java mods.
pub fn get() -> Option<&'static Routes> {
    ROUTES.get()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::Out;

    fn route(out: &mut Out, key: &str, kind: u8, on: bool, admitted: Option<&[&str]>) {
        route_v2(out, key, kind, on, admitted, None, None);
    }

    fn route_v2(
        out: &mut Out,
        key: &str,
        kind: u8,
        on: bool,
        admitted: Option<&[&str]>,
        items: Option<&[&str]>,
        blocks: Option<&[i32]>,
    ) {
        out.text(key);
        out.0.push(kind);
        out.text("block");
        out.bool(on);
        out.bool(false);
        match admitted {
            None => out.int(-1),
            Some(names) => {
                out.int(names.len() as i32);
                names.iter().for_each(|n| out.text(n));
            }
        }
        match items {
            None => out.int(-1),
            Some(ids) => {
                out.int(ids.len() as i32);
                ids.iter().for_each(|n| out.text(n));
            }
        }
        match blocks {
            None => out.int(-1),
            Some(ids) => {
                out.int(ids.len() as i32);
                ids.iter().for_each(|&id| out.int(id));
            }
        }
    }

    fn file() -> Vec<u8> {
        let mut out = Out::default();
        out.int(2);
        out.int(3);
        route(&mut out, "a#m()V", 0, true, None);
        route(&mut out, "b#n()V", 1, false, None);
        route(
            &mut out,
            "c#o()V",
            1,
            true,
            Some(&["minecraft:chest", "x:y"]),
        );
        out.0
    }

    #[test]
    fn a_file_round_trips() {
        let routes = Routes::read(&file()).unwrap();
        let a = routes.id("a#m()V").unwrap();
        assert_eq!(routes.get(a).kind, Kind::Edge);
        assert_eq!(routes.get(a).admitted, Admitted::All);
        assert_eq!(&*routes.get(a).family, "block");
        assert_eq!(routes.iter().count(), 3);
    }

    #[test]
    fn an_off_route_has_no_id() {
        let routes = Routes::read(&file()).unwrap();
        assert_eq!(routes.id("b#n()V"), None);
        assert!(!routes.iter().nth(1).unwrap().1.on);
    }

    #[test]
    fn admitted_only_holds_the_names() {
        let routes = Routes::read(&file()).unwrap();
        let c = routes.get(routes.id("c#o()V").unwrap());
        assert_eq!(c.kind, Kind::Body);
        let Admitted::Only(names) = &c.admitted else {
            panic!("not a list")
        };
        assert!(names.contains("minecraft:chest") && names.contains("x:y") && names.len() == 2);
    }

    #[test]
    fn a_route_reads_its_item_and_block_lists() {
        let mut out = Out::default();
        out.int(2);
        out.int(2);
        route_v2(
            &mut out,
            "a#m()V",
            0,
            true,
            None,
            Some(&["minecraft:bucket"]),
            Some(&[3, 70]),
        );
        out.text("b#n()V");
        out.0.push(0);
        out.text("value");
        out.bool(true);
        out.bool(false);
        out.int(-3);
        out.int(-1);
        out.int(-1);
        let routes = Routes::read(&out.0).unwrap();
        let a = routes.get(routes.id("a#m()V").unwrap());
        let items = a.items.as_ref().unwrap();
        assert!(items.contains("minecraft:bucket") && items.len() == 1);
        let blocks = a.blocks.as_ref().unwrap();
        assert!(blocks.contains(3) && blocks.contains(70));
        assert!(
            !blocks.contains(4)
                && !blocks.contains(64)
                && !blocks.contains(1000)
                && !blocks.contains(-1)
        );
        let b = routes.get(routes.id("b#n()V").unwrap());
        assert_eq!(
            (&b.admitted, &b.items, &b.blocks),
            (&Admitted::None, &None, &None)
        );
    }

    #[test]
    fn a_version_1_file_is_refused() {
        let mut bytes = file();
        bytes[3] = 1;
        assert_eq!(Routes::read(&bytes).unwrap_err(), "version 1, expected 2");
    }

    #[test]
    fn a_short_file_is_an_error() {
        let bytes = file();
        assert!(Routes::read(&bytes[..bytes.len() - 3]).is_err());
        assert!(Routes::read(&[]).is_err());
    }

    #[test]
    fn no_boot_means_no_routes() {
        assert!(get().is_none());
    }
}
