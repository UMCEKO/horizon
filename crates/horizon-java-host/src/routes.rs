//! The routes of the installed mods, confirmed at boot (`routes.bin`): which vanilla methods a loaded mod's mixin changes, and for which receivers.
use crate::wire::In;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub const VERSION: u32 = 1;

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
}

#[derive(Debug)]
pub struct Route {
    pub key: Box<str>,
    pub kind: Kind,
    pub family: Box<str>,
    pub on: bool,
    pub hot: bool,
    pub admitted: Admitted,
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
                count if count >= 0 => {
                    let mut names = HashSet::new();
                    for _ in 0..count {
                        names.insert(Box::from(r.text().ok_or(ENDED)?));
                    }
                    Admitted::Only(names)
                }
                other => return Err(format!("route {key}: admitted {other}")),
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
    }

    fn file() -> Vec<u8> {
        let mut out = Out::default();
        out.int(1);
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
