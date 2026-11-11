// game-module-cfg: feature java-mods
//! W12: a vanilla method a loaded mod's mixin patches runs as Java's merged method; its body stays native unless the patch is inside it.

/// Who a routed method's receiver is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum Recv {
    Player,
    Static,
}

#[cfg(feature = "java-mods")]
macro_rules! route_gate {
    ($game:expr, $key:literal, $recv:expr, ($($arg:expr),* $(,)?)) => {
        {
            static GATE: $crate::game::java_routing::Gate = $crate::game::java_routing::Gate::new($key);
            if let Some(id) = GATE.admitted(&$recv) {
                // Written before the call, so an argument may read the game.
                #[allow(unused_mut)]
                let mut args = horizon_java_host::wire::Out::default();
                $( $crate::game::java_routing::RouteArg::put(&$arg, &mut args); )*
                if let Some(value) = $game.route_call(id, $recv, |out| out.0.extend_from_slice(&args.0)) {
                    return value;
                }
            }
        }
    };
}

/// Marks the native site that serves a GUI key; expands to nothing.
#[allow(unused_macros)]
macro_rules! gui_route {
    ($key:literal) => {};
}

#[allow(unused_imports)]
pub(in crate::game) use {gui_route, route_gate as route};

/// A routed `Component` argument: its saved form.
pub(in crate::game) struct ComponentArg<'a>(pub &'a horizon_core::nbt::Tag);

/// A routed `Player` argument: the acting player, as a call fills its view.
pub(in crate::game) struct PlayerArg(pub Option<Vec<u8>>);

#[cfg(feature = "java-mods")]
pub(in crate::game) use gate::{Gate, RouteArg};

#[cfg(feature = "java-mods")]
impl crate::game::Game<'_> {
    /// The acting player as a routed `Player` argument.
    pub(in crate::game) fn java_player_arg(&mut self) -> PlayerArg {
        PlayerArg(self.java_use_request())
    }
}

#[cfg(feature = "java-mods")]
mod gate {
    use super::Recv;
    use crate::game::Game;
    use horizon_core::position::BlockPos;
    use horizon_java_host::routes::{self, Admitted, RouteId, Routes};
    use horizon_java_host::wire::{In, Out};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// One `route!` site: 0 unresolved, 1 off, `n + 2` is `RouteId(n)`.
    pub struct Gate {
        key: &'static str,
        state: AtomicU32,
    }

    impl Gate {
        pub const fn new(key: &'static str) -> Self {
            Gate {
                key,
                state: AtomicU32::new(0),
            }
        }

        pub fn admitted(&self, _recv: &Recv) -> Option<RouteId> {
            match self.state.load(Ordering::Relaxed) {
                0 => {
                    let routes = routes::get()?;
                    self.resolve_with(routes)
                }
                1 => None,
                n => Some(RouteId(n - 2)),
            }
        }

        /// Looks the key up and caches the answer; a family whose receivers aren't added yet never crosses.
        pub(super) fn resolve_with(&self, routes: &Routes) -> Option<RouteId> {
            let found = routes
                .id(self.key)
                .filter(|id| !matches!(routes.get(*id).admitted, Admitted::Pending));
            self.state
                .store(found.map_or(1, |id| id.0 + 2), Ordering::Relaxed);
            found
        }
    }

    pub trait RouteArg {
        fn put(&self, out: &mut Out);
    }

    impl RouteArg for i32 {
        fn put(&self, out: &mut Out) {
            out.int(*self);
        }
    }
    impl RouteArg for i64 {
        fn put(&self, out: &mut Out) {
            out.long(*self);
        }
    }
    impl RouteArg for f32 {
        fn put(&self, out: &mut Out) {
            out.float(*self);
        }
    }
    impl RouteArg for f64 {
        fn put(&self, out: &mut Out) {
            out.double(*self);
        }
    }
    impl RouteArg for bool {
        fn put(&self, out: &mut Out) {
            out.bool(*self);
        }
    }
    impl RouteArg for BlockPos {
        fn put(&self, out: &mut Out) {
            out.long(self.pack());
        }
    }
    impl RouteArg for &str {
        fn put(&self, out: &mut Out) {
            out.text(self);
        }
    }
    impl RouteArg for String {
        fn put(&self, out: &mut Out) {
            out.text(self);
        }
    }
    /// A `Vec3`.
    impl RouteArg for [f64; 3] {
        fn put(&self, out: &mut Out) {
            for v in self {
                out.double(*v);
            }
        }
    }
    impl RouteArg for super::ComponentArg<'_> {
        fn put(&self, out: &mut Out) {
            // `Flat.raw(Component)`'s wrapper, which Java unwraps the same way.
            let mut map = horizon_core::nbt::Compound::new();
            map.insert("text".into(), self.0.clone());
            let tag = crate::game::java_mods::write_tag(horizon_core::nbt::Tag::Compound(map));
            out.bytes(&tag.expect("a component encodes as NBT"));
        }
    }
    impl RouteArg for super::PlayerArg {
        fn put(&self, out: &mut Out) {
            let fill = self
                .0
                .as_ref()
                .expect("a routed Player argument needs the acting player");
            out.0.extend_from_slice(fill);
        }
    }
    impl RouteArg for &horizon_client::items::ItemStack {
        fn put(&self, out: &mut Out) {
            let tag = crate::game::java_mods::write_tag(horizon_client::save::item_to_nbt(self));
            out.bytes(&tag.expect("an item stack encodes as NBT"));
        }
    }

    pub trait RouteRet: Clone + 'static + Sized {
        fn read(reply: &mut In<'_>) -> Option<Self>;
    }

    impl RouteRet for () {
        fn read(_reply: &mut In<'_>) -> Option<Self> {
            Some(())
        }
    }
    impl RouteRet for bool {
        fn read(reply: &mut In<'_>) -> Option<Self> {
            reply.bool()
        }
    }
    impl RouteRet for i32 {
        fn read(reply: &mut In<'_>) -> Option<Self> {
            reply.int()
        }
    }
    impl RouteRet for i64 {
        fn read(reply: &mut In<'_>) -> Option<Self> {
            reply.long()
        }
    }
    impl RouteRet for f32 {
        fn read(reply: &mut In<'_>) -> Option<Self> {
            reply.float()
        }
    }
    impl RouteRet for f64 {
        fn read(reply: &mut In<'_>) -> Option<Self> {
            reply.double()
        }
    }

    /// The reply's status and value; a throw in Java panics here, as an exception unwinds a vanilla tick.
    pub(super) fn read_value<R: RouteRet>(key: &str, reply: &mut In<'_>) -> R {
        fn pieces<T>(key: &str) -> T {
            panic!("Horizon: route {key} answered in pieces")
        }
        match reply.byte().unwrap_or_else(|| pieces(key)) {
            0 => R::read(reply).unwrap_or_else(|| pieces(key)),
            _ => {
                let owner = reply.text().unwrap_or_else(|| pieces(key));
                let message = reply.text().unwrap_or_else(|| pieces(key));
                panic!("Horizon: a Java mod threw in {key} ({owner}): {message}");
            }
        }
    }

    impl Game<'_> {
        /// Runs a routed vanilla method as Java's merged method; `None` only when there is no host.
        pub(in crate::game) fn route_call<R: RouteRet>(
            &mut self,
            id: RouteId,
            recv: Recv,
            args: impl FnOnce(&mut Out),
        ) -> Option<R> {
            horizon_java_host::get()?;
            let key = &*routes::get()?.get(id).key;
            self.java_bind_states();
            let mut out = Out::default();
            out.int(id.0 as i32);
            match recv {
                Recv::Player => {
                    out.0.push(0);
                    out.0.extend(self.java_use_request()?);
                }
                Recv::Static => out.0.push(2),
            }
            args(&mut out);
            let Some(reply) = self.java_request("route", &out.0) else {
                panic!("Horizon: route {key} failed in Java (its stack trace is above)");
            };
            let mut reply = In(&reply);
            let value = read_value::<R>(key, &mut reply);
            if self.java_apply_effects(&mut reply).is_none() {
                panic!("Horizon: route {key} answered in pieces");
            }
            self.java_drain_block_events();
            Some(value)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn routes(on: bool, admitted: i32) -> Routes {
            let mut out = Out::default();
            out.int(1);
            out.int(1);
            out.text("a/B#m(I)V");
            out.0.push(0);
            out.text("player");
            out.bool(on);
            out.bool(false);
            out.int(admitted);
            Routes::read(&out.0).unwrap()
        }

        #[test]
        fn a_gate_without_routes_stays_native() {
            static GATE: Gate = Gate::new("a/B#m(I)V");
            assert_eq!(GATE.admitted(&Recv::Player), None);
            assert_eq!(GATE.state.load(Ordering::Relaxed), 0);
        }

        #[test]
        fn an_off_route_is_never_admitted() {
            let gate = Gate::new("a/B#m(I)V");
            assert_eq!(gate.resolve_with(&routes(false, -1)), None);
            assert_eq!(gate.admitted(&Recv::Player), None);
            assert_eq!(gate.state.load(Ordering::Relaxed), 1);
        }

        #[test]
        fn a_resolved_gate_caches_its_id() {
            let gate = Gate::new("a/B#m(I)V");
            assert_eq!(gate.resolve_with(&routes(true, -1)), Some(RouteId(0)));
            assert_eq!(gate.state.load(Ordering::Relaxed), 2);
            assert_eq!(gate.admitted(&Recv::Static), Some(RouteId(0)));
            let other = Gate::new("x#y()V");
            assert_eq!(other.resolve_with(&routes(true, -1)), None);
        }

        #[test]
        fn pending_admission_never_crosses() {
            let gate = Gate::new("a/B#m(I)V");
            assert_eq!(gate.resolve_with(&routes(true, -2)), None);
            assert_eq!(gate.admitted(&Recv::Player), None);
        }

        #[test]
        #[should_panic(expected = "a Java mod threw in a/B#m(I)V (somemod): boom")]
        fn a_throwing_route_panics_naming_the_mod() {
            let mut reply = Out::default();
            reply.0.push(1);
            reply.text("somemod");
            reply.text("boom");
            read_value::<()>("a/B#m(I)V", &mut In(&reply.0));
        }

        #[test]
        fn a_value_reply_reads_its_result() {
            let mut reply = Out::default();
            reply.0.push(0);
            reply.int(7);
            assert_eq!(read_value::<i32>("k", &mut In(&reply.0)), 7);
        }

        #[test]
        #[should_panic(expected = "answered in pieces")]
        fn a_short_reply_is_a_broken_protocol() {
            read_value::<i32>("k", &mut In(&[0, 0]));
        }

        #[test]
        fn an_item_stack_goes_as_its_saved_tag() {
            let stack = horizon_client::items::ItemStack {
                item: "minecraft:stone".into(),
                count: 3,
                damage: 0,
                extra: None,
            };
            let mut out = Out::default();
            RouteArg::put(&&stack, &mut out);
            let mut reply = In(&out.0);
            let tag = crate::game::java_mods::read_tag(reply.bytes().unwrap()).unwrap();
            let back = horizon_client::save::item_from_nbt(&tag).unwrap();
            assert_eq!((back.item, back.count), (stack.item, 3));
            assert!(reply.bytes().is_none());
        }

        #[test]
        fn a_vec3_and_a_component_go_as_java_reads_them() {
            let name = horizon_core::nbt::Tag::String("container.chest".into());
            let mut out = Out::default();
            RouteArg::put(&[1.5, -2.0, 0.25], &mut out);
            RouteArg::put(&super::super::ComponentArg(&name), &mut out);
            let mut reply = In(&out.0);
            assert_eq!(
                [reply.double(), reply.double(), reply.double()],
                [Some(1.5), Some(-2.0), Some(0.25)]
            );
            // `GameBridge.component` reads `Flat.raw(Component)`'s wrapper.
            let tag = crate::game::java_mods::read_tag(reply.bytes().unwrap()).unwrap();
            assert_eq!(tag.get("text"), Some(&name));
            assert!(reply.bytes().is_none());
        }

        #[test]
        fn route_args_write_as_java_reads_them() {
            let mut out = Out::default();
            RouteArg::put(&-2i32, &mut out);
            RouteArg::put(&0x0102_0304_0506_0708i64, &mut out);
            RouteArg::put(&1.5f32, &mut out);
            RouteArg::put(&-2.25f64, &mut out);
            RouteArg::put(&true, &mut out);
            RouteArg::put(&BlockPos::new(1, 2, 3), &mut out);
            RouteArg::put(&"hi", &mut out);
            RouteArg::put(&String::from("yo"), &mut out);
            let mut want = vec![0xff, 0xff, 0xff, 0xfe];
            want.extend([1, 2, 3, 4, 5, 6, 7, 8]);
            want.extend(1.5f32.to_be_bytes());
            want.extend((-2.25f64).to_be_bytes());
            want.push(1);
            want.extend(BlockPos::new(1, 2, 3).pack().to_be_bytes());
            want.extend([0, 0, 0, 2, b'h', b'i', 0, 0, 0, 2, b'y', b'o']);
            assert_eq!(out.0, want);
            println!("ROUTE_ARGS {:02x?}", out.0);
        }
    }
}
