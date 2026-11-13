// game-module-cfg: feature java-mods
//! W12: a vanilla method a loaded mod's mixin patches runs as Java's merged method; its body stays native unless the patch is inside it.

/// Who a routed method's receiver is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // the specs that gate these families construct them
pub(in crate::game) enum Recv {
    Player,
    Static,
    /// `GameBridge.server`.
    Server,
    /// The server's scoreboard.
    Scoreboard,
    /// The block entity at this position of the current level.
    BlockEntity(horizon_core::position::BlockPos),
}

#[cfg(feature = "java-mods")]
macro_rules! route_gate {
    ($game:expr, $key:literal, $recv:expr, ($($arg:expr),* $(,)?) $(, item = $item:expr)? $(, block = $block:expr)? $(,)?) => {
        {
            static GATE: $crate::game::java_routing::Gate = $crate::game::java_routing::Gate::new($key);
            if let Some(id) = GATE.admitted(&$recv)
                && GATE.narrowed(
                    id,
                    None::<&str> $(.or(Some($item)))?,
                    None::<horizon_world::registry::BlockStateId> $(.or(Some($block)))?,
                    concat!(file!(), ":", line!()),
                )
            {
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

/// Marks the native site that serves an environment attribute key from Java's probe; expands to nothing.
#[allow(unused_macros)]
macro_rules! env_route {
    ($key:literal) => {};
}

#[allow(unused_imports)]
pub(in crate::game) use {env_route, gui_route, route_gate as route};

/// Whether a mod patches the attribute system in this install, so the window's sky is Java's probe values.
#[cfg(feature = "java-mods")]
pub(in crate::game) fn client_env_routed() -> bool {
    env_route!("net/minecraft/world/attribute/EnvironmentAttributeProbe#tick(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/phys/Vec3;)V");
    env_route!("net/minecraft/world/attribute/EnvironmentAttributeSystem#addDynamicLayers(Lnet/minecraft/world/attribute/EnvironmentAttributeSystem$Builder;Lnet/minecraft/world/level/Level;)V");
    env_route!("net/minecraft/world/attribute/EnvironmentAttributeSystem$ValueSampler#computeValueNotPositional()Ljava/lang/Object;");
    env_route!("net/minecraft/world/attribute/EnvironmentAttributeSystem$ValueSampler#computeValuePositional(Lnet/minecraft/world/phys/Vec3;Lnet/minecraft/world/attribute/SpatialAttributeInterpolator;)Ljava/lang/Object;");
    let Some(routes) = horizon_java_host::routes::get() else {
        return false;
    };
    ENV_KEYS
        .iter()
        .any(|key| routes.id(key).is_some_and(|id| routes.get(id).on))
}

#[cfg(feature = "java-mods")]
const ENV_KEYS: [&str; 4] = [
    "net/minecraft/world/attribute/EnvironmentAttributeProbe#tick(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/phys/Vec3;)V",
    "net/minecraft/world/attribute/EnvironmentAttributeSystem#addDynamicLayers(Lnet/minecraft/world/attribute/EnvironmentAttributeSystem$Builder;Lnet/minecraft/world/level/Level;)V",
    "net/minecraft/world/attribute/EnvironmentAttributeSystem$ValueSampler#computeValueNotPositional()Ljava/lang/Object;",
    "net/minecraft/world/attribute/EnvironmentAttributeSystem$ValueSampler#computeValuePositional(Lnet/minecraft/world/phys/Vec3;Lnet/minecraft/world/attribute/SpatialAttributeInterpolator;)Ljava/lang/Object;",
];

/// A routed `Component` argument: its saved form.
pub(in crate::game) struct ComponentArg<'a>(pub &'a horizon_core::nbt::Tag);

/// A routed `Player` argument: the acting player, as a call fills its view.
pub(in crate::game) struct PlayerArg(pub Option<Vec<u8>>);

/// A routed `MobEffectInstance` argument.
#[allow(dead_code)] // gated routes construct it
pub(in crate::game) struct EffectArg {
    pub id: String,
    pub duration: i32,
    pub amplifier: i32,
    pub ambient: bool,
    pub visible: bool,
    pub icon: bool,
}

/// A routed `Entity` argument: its id, `None` for null.
#[allow(dead_code)] // gated routes construct it
pub(in crate::game) struct EntityArg(pub Option<u32>);

/// A routed `Holder`: the registry's path, a tab, and the entry's id.
#[allow(dead_code)] // gated routes construct it
pub(in crate::game) struct HolderArg<'a>(pub &'a str);

/// A routed `ResourceKey`: the registry's id, then the key's.
#[allow(dead_code)] // gated routes construct it
pub(in crate::game) struct KeyArg<'a>(pub &'a str, pub &'a str);

/// A routed `PlayerTeam`: its name, then its packed tag (empty for a team the board no longer has), which builds the team the view dropped.
pub(in crate::game) struct TeamArg(pub String, pub Option<horizon_client::scoreboard::Team>);

impl TeamArg {
    /// The board's team of that name, as it stands now.
    pub(in crate::game) fn named(board: &horizon_client::scoreboard::Scoreboard, name: &str) -> Self {
        Self(name.to_owned(), board.team(name).cloned())
    }

    /// A team the call removed: its packed form from before.
    pub(in crate::game) fn removed(team: horizon_client::scoreboard::Team) -> Self {
        Self(team.name.clone(), Some(team))
    }
}

/// A routed `Level`: its dimension's id.
#[allow(dead_code)] // gated routes construct it
pub(in crate::game) struct LevelArg<'a>(pub &'a str);

/// A routed `TeleportTransition`: the bytes `Flat.raw(TeleportTransition)` writes.
#[allow(dead_code)] // gated routes construct it
pub(in crate::game) struct TransitionArg(pub Vec<u8>);

/// A routed `ValueInput`: a compound tag.
#[allow(dead_code)] // gated routes construct it
pub(in crate::game) struct InputArg<'a>(pub &'a horizon_core::nbt::Tag);

/// A routed `ValueOutput`: nothing crosses in; the reply carries what Java wrote.
#[allow(dead_code)] // gated routes construct it
pub(in crate::game) struct OutputArg;

/// `InteractionResult` as a routed use method answers it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // the use paths read it as their specs gate them
pub(in crate::game) enum UseResult {
    /// With whether its `SwingSource` is not `NONE`.
    Success(bool),
    Fail,
    Pass,
    TryEmptyHand,
}

#[cfg(feature = "java-mods")]
pub(in crate::game) use gate::{Gate, RouteArg};

#[cfg(feature = "java-mods")]
impl crate::game::Game<'_> {
    /// The acting player as a routed `Player` argument.
    pub(in crate::game) fn java_player_arg(&mut self) -> PlayerArg {
        PlayerArg(self.java_use_request())
    }

    /// `runTick`'s `pauseAllExcept(MUSIC, UI)` through the mods' `@ModifyArg`s: the engine's sources to keep, None while the route is off.
    pub(in crate::game) fn java_pause_sources(&mut self) -> Option<Vec<usize>> {
        const KEY: &str = "net/minecraft/client/Minecraft#runTick(Z)V";
        gui_route!("net/minecraft/client/Minecraft#runTick(Z)V");
        let routes = horizon_java_host::routes::get()?;
        if !routes.get(routes.id(KEY)?).on {
            return None;
        }
        let host = horizon_java_host::get()?;
        let reply = horizon_java_host::bridge::with_active(self, || {
            host.request("client_pause_sources", &[])
        });
        let kept = reply.as_deref().and_then(pause_sources);
        if kept.is_none() {
            log::error!("Horizon: the Java client gave no pause sources; pausing vanilla's");
        }
        kept
    }
}

/// The `client_pause_sources` reply (`int n`, n `SoundSource` names) as the engine's source indices, matched by name.
#[cfg(feature = "java-mods")]
fn pause_sources(reply: &[u8]) -> Option<Vec<usize>> {
    use horizon_client::settings::SOUND_SOURCES;
    let mut r = horizon_java_host::wire::In(reply);
    let n = r.int()?;
    let mut kept = Vec::with_capacity(usize::try_from(n).ok()?);
    for _ in 0..n {
        let name = r.text()?;
        match SOUND_SOURCES.iter().position(|s| *s == name) {
            Some(index) => kept.push(index),
            None => log::error!("Horizon: a mod kept an unknown sound source {name} playing through the pause"),
        }
    }
    Some(kept)
}

#[cfg(feature = "java-mods")]
mod gate {
    use super::Recv;
    use crate::game::Game;
    use horizon_core::nbt::Tag;
    use horizon_core::position::BlockPos;
    use horizon_java_host::routes::{self, Admitted, Route, RouteId, Routes};
    use horizon_world::registry::BlockStateId;
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

        /// Whether the call's stack item and block state pass the route's guard lists; a route with a list its site doesn't feed is a seam bug.
        pub fn narrowed(
            &self,
            id: RouteId,
            item: Option<&str>,
            block: Option<BlockStateId>,
            site: &str,
        ) -> bool {
            routes::get().is_none_or(|routes| self.narrowed_by(routes.get(id), item, block, site))
        }

        pub(super) fn narrowed_by(
            &self,
            route: &Route,
            item: Option<&str>,
            block: Option<BlockStateId>,
            site: &str,
        ) -> bool {
            if let Some(items) = &route.items {
                let Some(item) = item else {
                    panic!("Horizon: route {} admits by item but its gate at {site} passes none", self.key)
                };
                if !items.contains(item) {
                    return false;
                }
            }
            if let Some(blocks) = &route.blocks {
                let Some(block) = block else {
                    panic!("Horizon: route {} admits by block but its gate at {site} passes none", self.key)
                };
                let java = horizon_java_host::bridge::STATES
                    .get()
                    .and_then(|m| m.to_java.get(block.0 as usize).copied())
                    .unwrap_or(horizon_java_host::bridge::UNMAPPED);
                if !blocks.contains(java) {
                    return false;
                }
            }
            true
        }

        /// Looks the key up and caches the answer; a family whose receivers aren't added yet, or that no receiver can reach, never crosses.
        pub(super) fn resolve_with(&self, routes: &Routes) -> Option<RouteId> {
            let found = routes.id(self.key).filter(|id| {
                !matches!(
                    routes.get(*id).admitted,
                    Admitted::Pending | Admitted::None
                )
            });
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
    impl RouteArg for &horizon_java_host::bridge::Source {
        fn put(&self, out: &mut Out) {
            let mut bytes = Out::default();
            self.write(&mut bytes);
            out.bytes(&bytes.0);
        }
    }
    impl RouteArg for &super::EffectArg {
        fn put(&self, out: &mut Out) {
            out.text(&self.id);
            out.int(self.duration);
            out.int(self.amplifier);
            out.bool(self.ambient);
            out.bool(self.visible);
            out.bool(self.icon);
        }
    }
    impl RouteArg for super::EntityArg {
        fn put(&self, out: &mut Out) {
            out.int(self.0.map_or(0, |id| id as i32));
        }
    }
    impl RouteArg for super::HolderArg<'_> {
        fn put(&self, out: &mut Out) {
            out.text(self.0);
        }
    }
    impl RouteArg for super::KeyArg<'_> {
        fn put(&self, out: &mut Out) {
            out.text(self.0);
            out.text(self.1);
        }
    }
    impl RouteArg for super::TeamArg {
        fn put(&self, out: &mut Out) {
            out.text(&self.0);
            match &self.1 {
                Some(team) => out.bytes(&crate::game::java_mods::write_tag(team.pack()).expect("a team encodes as NBT")),
                None => out.bytes(&[]),
            }
        }
    }
    impl RouteArg for super::LevelArg<'_> {
        fn put(&self, out: &mut Out) {
            out.text(self.0);
        }
    }
    impl RouteArg for super::TransitionArg {
        fn put(&self, out: &mut Out) {
            out.bytes(&self.0);
        }
    }
    impl RouteArg for super::InputArg<'_> {
        fn put(&self, out: &mut Out) {
            let tag = crate::game::java_mods::write_tag(self.0.clone());
            out.bytes(&tag.expect("a compound tag encodes as NBT"));
        }
    }
    impl RouteArg for super::OutputArg {
        fn put(&self, _out: &mut Out) {}
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

    impl RouteRet for String {
        fn read(reply: &mut In<'_>) -> Option<Self> {
            reply.text().map(str::to_owned)
        }
    }
    /// A `Vec3`.
    impl RouteRet for [f64; 3] {
        fn read(reply: &mut In<'_>) -> Option<Self> {
            Some([reply.double()?, reply.double()?, reply.double()?])
        }
    }
    /// An `Optional<Vec3>`.
    impl RouteRet for Option<[f64; 3]> {
        fn read(reply: &mut In<'_>) -> Option<Self> {
            if reply.bool()? {
                <[f64; 3]>::read(reply).map(Some)
            } else {
                Some(None)
            }
        }
    }
    /// An `ItemStack`: its saved tag, empty bytes for the empty stack.
    impl RouteRet for Option<horizon_client::items::ItemStack> {
        fn read(reply: &mut In<'_>) -> Option<Self> {
            let bytes = reply.bytes()?;
            if bytes.is_empty() {
                return Some(None);
            }
            let tag = crate::game::java_mods::read_tag(bytes)?;
            horizon_client::save::item_from_nbt(&tag).map(Some)
        }
    }
    /// An `InteractionResult`: the kind, then the `SwingSource`'s ordinal.
    impl RouteRet for super::UseResult {
        fn read(reply: &mut In<'_>) -> Option<Self> {
            let (kind, swing) = (reply.int()?, reply.int()?);
            match kind {
                0 => Some(Self::Success(swing != 0)),
                1 => Some(Self::Fail),
                2 => Some(Self::Pass),
                3 => Some(Self::TryEmptyHand),
                _ => None,
            }
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
            self.route_call_out(id, recv, 0, args)
                .map(|(value, _)| value)
        }

        /// [`Game::route_call`] for a method with `outputs` `ValueOutput` parameters: the value, then what Java wrote into each.
        pub(in crate::game) fn route_call_out<R: RouteRet>(
            &mut self,
            id: RouteId,
            recv: Recv,
            outputs: usize,
            args: impl FnOnce(&mut Out),
        ) -> Option<(R, Vec<Tag>)> {
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
                Recv::Server => out.0.push(3),
                Recv::Scoreboard => {
                    out.0.push(4);
                    self.java_scoreboard_fill(&mut out);
                }
                Recv::BlockEntity(pos) => {
                    self.java_level_key();
                    out.0.push(5);
                    out.text(self.dim.id());
                    out.long(self.world.game_time);
                    out.long(pos.pack());
                }
            }
            args(&mut out);
            let Some(reply) = self.java_request("route", &out.0) else {
                panic!("Horizon: route {key} failed in Java (its stack trace is above)");
            };
            let mut reply = In(&reply);
            let value = read_value::<R>(key, &mut reply);
            let written = read_outputs(&mut reply, outputs)
                .unwrap_or_else(|| panic!("Horizon: route {key} answered in pieces"));
            let level = reply
                .int()
                .unwrap_or_else(|| panic!("Horizon: route {key} answered in pieces"));
            // -1: no player was filled for this call.
            if level != -1 && self.java_apply_changes(level, &mut reply).is_none() {
                panic!("Horizon: route {key} answered in pieces");
            }
            if self.java_apply_effects(&mut reply).is_none() {
                panic!("Horizon: route {key} answered in pieces");
            }
            self.java_drain_block_events();
            Some((value, written))
        }
    }

    /// What Java wrote into each `ValueOutput`, which follows the value.
    pub(super) fn read_outputs(reply: &mut In<'_>, count: usize) -> Option<Vec<Tag>> {
        (0..count)
            .map(|_| reply.bytes().and_then(crate::game::java_mods::read_tag))
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn routes(on: bool, admitted: i32) -> Routes {
            routes_with(on, admitted, None)
        }

        fn routes_with(on: bool, admitted: i32, items: Option<&[&str]>) -> Routes {
            let mut out = Out::default();
            out.int(2);
            out.int(1);
            out.text("a/B#m(I)V");
            out.0.push(0);
            out.text("player");
            out.bool(on);
            out.bool(false);
            out.int(admitted);
            match items {
                None => out.int(-1),
                Some(ids) => {
                    out.int(ids.len() as i32);
                    ids.iter().for_each(|id| out.text(id));
                }
            }
            out.int(-1);
            Routes::read(&out.0).unwrap()
        }

        #[test]
        fn none_admission_never_crosses() {
            let gate = Gate::new("a/B#m(I)V");
            assert_eq!(gate.resolve_with(&routes(true, -3)), None);
            assert_eq!(gate.state.load(Ordering::Relaxed), 1);
            assert_eq!(gate.admitted(&Recv::Static), None);
        }

        #[test]
        fn an_item_list_admits_only_its_items() {
            let routes = routes_with(true, -1, Some(&["minecraft:bucket", "minecraft:water_bucket"]));
            let gate = Gate::new("a/B#m(I)V");
            let route = routes.get(gate.resolve_with(&routes).unwrap());
            assert!(gate.narrowed_by(route, Some("minecraft:water_bucket"), None, "here"));
            assert!(!gate.narrowed_by(route, Some("minecraft:stone"), None, "here"));
            let open = routes_with(true, -1, None);
            assert!(gate.narrowed_by(open.get(RouteId(0)), None, None, "here"));
        }

        #[test]
        #[should_panic(expected = "route a/B#m(I)V admits by item but its gate at f.rs:3 passes none")]
        fn an_item_list_reached_without_an_item_is_a_seam_bug() {
            let routes = routes_with(true, -1, Some(&["minecraft:bucket"]));
            Gate::new("a/B#m(I)V").narrowed_by(routes.get(RouteId(0)), None, None, "f.rs:3");
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
            reply.int(-1);
            reply.int(0);
            let mut reply = In(&reply.0);
            assert_eq!(read_value::<i32>("k", &mut reply), 7);
            // After the value: no player's changes (-1), then no sounds.
            assert_eq!(
                (reply.int(), reply.int(), reply.int()),
                (Some(-1), Some(0), None)
            );
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
        fn vec3_and_effect_args_write_as_java_reads_them() {
            let effect = super::super::EffectArg {
                id: "minecraft:speed".into(),
                duration: 100,
                amplifier: 1,
                ambient: false,
                visible: true,
                icon: true,
            };
            let mut out = Out::default();
            RouteArg::put(&[1.5, -2.0, 0.25], &mut out);
            RouteArg::put(&&effect, &mut out);
            RouteArg::put(&super::super::EntityArg(None), &mut out);
            RouteArg::put(
                &super::super::KeyArg("minecraft:block", "minecraft:stone"),
                &mut out,
            );
            RouteArg::put(&super::super::LevelArg("minecraft:overworld"), &mut out);
            let mut want = Vec::new();
            for v in [1.5f64, -2.0, 0.25] {
                want.extend(v.to_be_bytes());
            }
            want.extend([0, 0, 0, 15]);
            want.extend(b"minecraft:speed");
            want.extend([0, 0, 0, 100, 0, 0, 0, 1, 0, 1, 1]);
            want.extend([0, 0, 0, 0]);
            for text in ["minecraft:block", "minecraft:stone", "minecraft:overworld"] {
                want.extend((text.len() as i32).to_be_bytes());
                want.extend(text.as_bytes());
            }
            assert_eq!(out.0, want);
            let hex: String = out.0.iter().map(|b| format!("{b:02x}")).collect();
            println!("ROUTE_ARGS_2 {hex}");
        }

        #[test]
        fn a_damage_source_goes_as_the_bytes_java_reads() {
            let source = horizon_java_host::bridge::Source {
                kind: "minecraft:arrow".into(),
                causing: 4,
                direct: 9,
                creative: false,
                at: Some([1.0, 2.0, 3.0]),
            };
            let mut out = Out::default();
            RouteArg::put(&&source, &mut out);
            let bytes = In(&out.0).bytes().unwrap();
            assert_eq!(horizon_java_host::bridge::Source::read(bytes), Some(source));
        }

        #[test]
        fn optional_vec3_reads_both_cases() {
            let mut absent = Out::default();
            absent.bool(false);
            assert_eq!(
                <Option<[f64; 3]>>::read(&mut In(&absent.0)),
                Some(None)
            );
            let mut present = Out::default();
            present.bool(true);
            for v in [1.0f64, 2.0, 3.0] {
                present.double(v);
            }
            assert_eq!(
                <Option<[f64; 3]>>::read(&mut In(&present.0)),
                Some(Some([1.0, 2.0, 3.0]))
            );
            // present without its three doubles is a broken reply
            assert_eq!(<Option<[f64; 3]>>::read(&mut In(&present.0[..9])), None);
        }

        #[test]
        fn item_stack_return_round_trips() {
            let mut components = horizon_core::nbt::Compound::new();
            components.insert("minecraft:custom_name".into(), Tag::String("Rock".into()));
            let mut saved = horizon_core::nbt::Compound::new();
            saved.insert("id".into(), Tag::String("minecraft:stone".into()));
            saved.insert("count".into(), Tag::Int(3));
            saved.insert("components".into(), Tag::Compound(components));
            let stack = horizon_client::save::item_from_nbt(&Tag::Compound(saved)).unwrap();
            assert!(stack.count == 3 && stack.extra.is_some());
            let tag = crate::game::java_mods::write_tag(horizon_client::save::item_to_nbt(&stack));
            let mut reply = Out::default();
            reply.bytes(&tag.unwrap());
            reply.bytes(&[]);
            let mut reply = In(&reply.0);
            let back = <Option<horizon_client::items::ItemStack>>::read(&mut reply);
            assert_eq!(back, Some(Some(stack)));
            // the empty stack is no bytes at all
            let empty = <Option<horizon_client::items::ItemStack>>::read(&mut reply);
            assert_eq!(empty, Some(None));
        }

        #[test]
        fn written_outputs_follow_the_value() {
            let mut compound = horizon_core::nbt::Compound::new();
            compound.insert("a".into(), Tag::Byte(1));
            let saved = crate::game::java_mods::write_tag(Tag::Compound(compound.clone()));
            let mut reply = Out::default();
            reply.0.push(0);
            reply.int(7);
            reply.bytes(&saved.unwrap());
            reply.int(-1);
            reply.int(0);
            let mut reply = In(&reply.0);
            assert_eq!(read_value::<i32>("k", &mut reply), 7);
            assert_eq!(read_outputs(&mut reply, 1), Some(vec![Tag::Compound(compound)]));
            // then no player's changes (-1) and no sounds
            assert_eq!((reply.int(), reply.int()), (Some(-1), Some(0)));
            assert_eq!(read_outputs(&mut In(&[]), 1), None);
        }

        #[test]
        fn use_results_read_their_kind_and_swing() {
            let read = |kind: i32, swing: i32| {
                let mut out = Out::default();
                out.int(kind);
                out.int(swing);
                super::super::UseResult::read(&mut In(&out.0))
            };
            assert_eq!(read(0, 1), Some(super::super::UseResult::Success(true)));
            assert_eq!(read(0, 0), Some(super::super::UseResult::Success(false)));
            assert_eq!(read(3, 0), Some(super::super::UseResult::TryEmptyHand));
            assert_eq!(read(4, 0), None);
        }

        #[test]
        fn a_scoreboard_route_that_is_pending_never_crosses() {
            let gate = Gate::new("a/B#m(I)V");
            assert_eq!(gate.resolve_with(&routes(true, -2)), None);
            assert_eq!(gate.admitted(&Recv::Scoreboard), None);
            let open = Gate::new("a/B#m(I)V");
            assert_eq!(open.resolve_with(&routes(true, -1)), Some(RouteId(0)));
            assert_eq!(open.admitted(&Recv::Server), Some(RouteId(0)));
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
