//! Player candidates for EntityGetter.isUnobstructed while a guest owns the
//! game body. Identity keeps borrowed ancestor bodies distinct from hub copies.
use super::Dim;
use horizon_core::geometry::Aabb;
use std::cell::{Cell, OnceCell};
use std::rc::Rc;

#[derive(Clone, Copy)]
pub(super) struct BuildingPlayer {
    pub(super) conn: Option<u32>,
    pub(super) dimension: Dim,
    pub(super) bounds: Option<Aabb>,
    pub(super) borrowed: bool,
}

/// The hub's bodies, shared by the turns of one tick instead of built for each
/// turn. A body is refreshed in place when its guest's turn (or anything that
/// changes it) ends, so every holder sees it where it is now.
pub(super) type HubBodies = Rc<Vec<Cell<BuildingPlayer>>>;

pub(super) fn shared(players: impl IntoIterator<Item = BuildingPlayer>) -> HubBodies {
    Rc::new(players.into_iter().map(Cell::new).collect())
}

/// The players' boxes a guest's turn sees: the borrowed ancestor bodies, and
/// the hub's bodies but for the actor, which a lazy turn lists at its first `boxes_with`.
pub(super) struct BuildingPlayers {
    ancestors: Vec<BuildingPlayer>,
    actor: Option<u32>,
    hub: OnceCell<HubBodies>,
}

impl Default for BuildingPlayers {
    fn default() -> Self {
        Self {
            ancestors: Vec::new(),
            actor: None,
            hub: OnceCell::from(HubBodies::default()),
        }
    }
}

impl BuildingPlayers {
    /// The live ancestor bodies plus the outer body; `present` says whether the hub still has the conn.
    fn ancestors(
        &self,
        actor: u32,
        outer: Option<BuildingPlayer>,
        present: impl Fn(u32) -> bool,
    ) -> Vec<BuildingPlayer> {
        let alive = |conn: Option<u32>| conn.is_none_or(&present);
        // The borrowed ancestors are the few nested turns (usually none or
        // one): only they are copied, the hub's bodies are shared.
        let mut ancestors: Vec<_> = self
            .ancestors
            .iter()
            .copied()
            .filter(|p| p.borrowed && p.conn != Some(actor) && alive(p.conn))
            .collect();
        if let Some(outer) = outer {
            ancestors.retain(|p| p.conn != outer.conn);
            if outer.conn != Some(actor) && alive(outer.conn) {
                ancestors.push(outer);
            }
        }
        ancestors
    }

    /// Refresh hub-owned bodies, preserve live ancestor bodies held outside
    /// the hub, and omit the actor (GameLevel.player_box supplies that body).
    pub(super) fn for_turn(
        &self,
        actor: u32,
        outer: Option<BuildingPlayer>,
        current: HubBodies,
    ) -> Self {
        let ancestors = self.ancestors(actor, outer, |conn| {
            current.iter().any(|g| g.get().conn == Some(conn))
        });
        Self {
            ancestors,
            actor: Some(actor),
            hub: OnceCell::from(current),
        }
    }

    /// `for_turn` whose hub part is listed by the first `boxes_with`.
    pub(super) fn lazy_turn(
        &self,
        actor: u32,
        outer: Option<BuildingPlayer>,
        present: impl Fn(u32) -> bool,
    ) -> Self {
        Self {
            ancestors: self.ancestors(actor, outer, present),
            actor: Some(actor),
            hub: OnceCell::new(),
        }
    }

    /// The boxes in `dimension`; `hub` lists the hub's bodies if this turn still has to.
    pub(super) fn boxes_with(
        &self,
        dimension: Dim,
        hub: impl FnOnce() -> HubBodies,
    ) -> Vec<([f64; 3], [f64; 3])> {
        // Hub conns are unique, so only the ancestors can already hold one.
        let hub =
            self.hub.get_or_init(hub).iter().map(Cell::get).filter(|p| {
                p.conn != self.actor && !self.ancestors.iter().any(|a| a.conn == p.conn)
            });
        self.ancestors
            .iter()
            .copied()
            .chain(hub)
            .filter(|p| p.dimension == dimension)
            .filter_map(|p| p.bounds)
            .map(|bounds| {
                (
                    bounds.min_position().components(),
                    bounds.max_position().components(),
                )
            })
            .collect()
    }

    #[cfg(test)]
    pub(super) fn boxes(&self, dimension: Dim) -> Vec<([f64; 3], [f64; 3])> {
        self.boxes_with(dimension, || unreachable!("a list that was not eager"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::geometry::Vec3;

    fn player(conn: Option<u32>, x: f64, dimension: Dim, borrowed: bool) -> BuildingPlayer {
        BuildingPlayer {
            conn,
            dimension,
            borrowed,
            bounds: Some(Aabb::new(
                Vec3::new(x, 0.0, 0.0),
                Vec3::new(x + 0.6, 1.8, 0.6),
            )),
        }
    }
    fn xs(players: &BuildingPlayers, dim: Dim) -> Vec<f64> {
        players.boxes(dim).iter().map(|(lo, _)| lo[0]).collect()
    }

    fn hub() -> Vec<BuildingPlayer> {
        vec![
            player(Some(1), 10.0, Dim::Overworld, false),
            player(Some(2), 20.0, Dim::Overworld, false),
            player(Some(3), 30.0, Dim::Nether, false),
        ]
    }
    fn lazy_xs(players: &BuildingPlayers, dim: Dim) -> Vec<f64> {
        let boxes = players.boxes_with(dim, || shared(hub()));
        boxes.iter().map(|(lo, _)| lo[0]).collect()
    }
    fn in_hub(conn: u32) -> bool {
        hub().iter().any(|p| p.conn == Some(conn))
    }

    #[test]
    fn a_lazy_turn_lists_the_hub_when_asked_and_only_once() {
        let builds = Cell::new(0);
        let host = player(None, 0.0, Dim::Overworld, true);
        let turn = BuildingPlayers::default().lazy_turn(1, Some(host), in_hub);
        let counted = || {
            builds.set(builds.get() + 1);
            shared(hub())
        };
        assert_eq!(builds.get(), 0);
        let first = turn.boxes_with(Dim::Overworld, counted);
        let second = turn.boxes_with(Dim::Nether, counted);
        assert_eq!(builds.get(), 1);
        // Player 1 is the actor, so the overworld holds the host and player 2.
        let lows = |b: &[([f64; 3], [f64; 3])]| b.iter().map(|(lo, _)| lo[0]).collect::<Vec<_>>();
        assert_eq!(lows(&first), [0.0, 20.0]);
        assert_eq!(lows(&second), [30.0]);
    }

    #[test]
    fn a_lazy_list_equals_the_eager_one_through_nested_turns() {
        let host = player(None, 0.0, Dim::Overworld, true);
        let eager1 = BuildingPlayers::default().for_turn(1, Some(host), shared(hub()));
        let lazy1 = BuildingPlayers::default().lazy_turn(1, Some(host), in_hub);
        for dim in [Dim::Overworld, Dim::Nether] {
            assert_eq!(lazy_xs(&lazy1, dim), xs(&eager1, dim));
        }
        // A turn nested in the first, from the eager list and from the lazy one.
        let outer = player(Some(1), 11.0, Dim::Overworld, true);
        let eager2 = eager1.for_turn(2, Some(outer), shared(hub()));
        let lazy2 = lazy1.lazy_turn(2, Some(outer), in_hub);
        assert_eq!(lazy_xs(&lazy2, Dim::Overworld), [0.0, 11.0]);
        assert_eq!(lazy_xs(&lazy2, Dim::Nether), [30.0]);
        for dim in [Dim::Overworld, Dim::Nether] {
            assert_eq!(lazy_xs(&lazy2, dim), xs(&eager2, dim));
        }
        // An ancestor whose player left the hub does not linger in either.
        let gone = player(Some(9), 5.0, Dim::Overworld, true);
        let eager3 = eager2.for_turn(3, Some(gone), shared(hub()));
        let lazy3 = lazy2.lazy_turn(3, Some(gone), in_hub);
        assert_eq!(lazy_xs(&lazy3, Dim::Overworld), [0.0, 11.0, 20.0]);
        assert_eq!(lazy_xs(&lazy3, Dim::Overworld), xs(&eager3, Dim::Overworld));
    }

    #[test]
    fn nested_turns_preserve_host_and_live_ancestors_without_the_new_actor() {
        let hub = || {
            vec![
                player(Some(1), 10.0, Dim::Overworld, false),
                player(Some(2), 20.0, Dim::Overworld, false),
                player(Some(3), 30.0, Dim::Nether, false),
            ]
        };
        let host = player(None, 0.0, Dim::Overworld, true);
        let first = BuildingPlayers::default().for_turn(1, Some(host), shared(hub()));
        let second = first.for_turn(
            2,
            Some(player(Some(1), 11.0, Dim::Overworld, true)),
            shared(hub()),
        );
        assert_eq!(xs(&second, Dim::Overworld), [0.0, 11.0]);
        // A further turn in another level keeps each ancestor's own level.
        let third = second.for_turn(
            3,
            Some(player(Some(2), 21.0, Dim::Nether, true)),
            shared(hub()),
        );
        assert_eq!(xs(&third, Dim::Overworld), [0.0, 11.0]);
        assert_eq!(xs(&third, Dim::Nether), [21.0]);
    }

    #[test]
    fn returning_inner_turn_refreshes_moved_and_inactive_hub_bodies() {
        let host = player(None, 0.0, Dim::Overworld, true);
        let first = BuildingPlayers::default().for_turn(
            1,
            Some(host),
            shared(vec![
                player(Some(1), 10.0, Dim::Overworld, false),
                player(Some(2), 20.0, Dim::Overworld, false),
            ]),
        );
        // Restore the first turn after player 2 moved across dimensions.
        let restored = first.for_turn(
            1,
            None,
            shared(vec![
                player(Some(1), 10.0, Dim::Overworld, false),
                player(Some(2), 25.0, Dim::Nether, false),
            ]),
        );
        assert_eq!(xs(&restored, Dim::Overworld), [0.0]);
        assert_eq!(xs(&restored, Dim::Nether), [25.0]);
        // Death, spectator mode, and unloaded bodies provide no building box.
        let mut inactive = player(Some(2), 25.0, Dim::Nether, false);
        inactive.bounds = None;
        let restored = restored.for_turn(1, None, shared(vec![inactive]));
        assert_eq!(xs(&restored, Dim::Overworld), [0.0]);
        assert!(xs(&restored, Dim::Nether).is_empty());
    }

    #[test]
    fn disconnected_ancestors_and_inactive_outer_bodies_do_not_linger() {
        let first = BuildingPlayers::default().for_turn(
            2,
            Some(player(Some(1), 10.0, Dim::Overworld, true)),
            shared(vec![player(Some(1), 1.0, Dim::Overworld, false)]),
        );
        let disconnected = first.for_turn(3, None, shared(Vec::new()));
        assert!(xs(&disconnected, Dim::Overworld).is_empty());
        let disconnected_outer = first.for_turn(
            3,
            Some(player(Some(1), 11.0, Dim::Overworld, true)),
            shared(Vec::new()),
        );
        assert!(xs(&disconnected_outer, Dim::Overworld).is_empty());
        let mut outer = player(None, 0.0, Dim::Overworld, true);
        let first = BuildingPlayers::default().for_turn(1, Some(outer), shared(Vec::new()));
        outer.bounds = None;
        let second = first.for_turn(2, Some(outer), shared(Vec::new()));
        assert!(xs(&second, Dim::Overworld).is_empty());
    }

    #[test]
    #[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
    fn for_player_keeps_the_outer_body_in_its_original_dimension() {
        use crate::game::{Audio, Game, Options, Progress, Settings, load_resources};
        use horizon_client::{
            lan::{Joined, NetConfig, PlayerSender, guest::Guest, hub::HubConfig},
            motion::PlayerMotion,
        };
        use horizon_protocol::packets::configuration::ClientInformation;
        use std::sync::{Arc, atomic::AtomicI32, mpsc};
        let root = std::path::PathBuf::from(std::env::var_os("HORIZON_TEST_RESOURCES").unwrap());
        let settings = Settings::default();
        let loaded = load_resources(
            root.clone(),
            None,
            &root,
            &settings,
            std::time::Instant::now(),
            false,
            true,
            &Progress::new(),
            &mut |_, _| unreachable!(),
        )
        .unwrap();
        let mut options = Options::parse(std::iter::empty()).unwrap().unwrap();
        options.resources = root.clone();
        options.execution_role = crate::game::execution_role::ExecutionRole::IntegratedServer;
        let mut game = Game::new(
            &loaded.resources,
            loaded.pack,
            loaded.gui.unwrap(),
            options,
            settings,
            Audio::silent(&root),
            None,
        )
        .unwrap();
        let scratch = std::env::temp_dir().join(format!(
            "horizon-building-turn-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&scratch).unwrap();
        game.world_root = scratch.clone();
        game.player = Some(PlayerMotion::new(game.motion, Vec3::new(10.5, 64.0, 0.5)).unwrap());
        let original = game.player.as_ref().unwrap().bounds();
        let net = NetConfig {
            announce: false,
            ..NetConfig::lan(0, "building-test".into(), 2)
        };
        game.lan_host(
            super::super::lan::LanSettings {
                port: 0,
                guest_commands: true,
                force_game_mode: false,
            },
            net,
            HubConfig {
                default_game_mode: 0,
                guest_commands: true,
                force_game_mode: false,
                hardcore: false,
                view_distance: 2,
                simulation_distance: 2,
                dimension: Dim::Overworld.id().into(),
                allow_flight: false,
                seed: 0,
                max_players: 2,
                integrated: false,
                levels: Default::default(),
            },
        )
        .unwrap();
        let (tx, _rx) = mpsc::channel();
        let mut guest = Guest::new(
            Joined {
                conn: 2,
                uuid: 2,
                name: "NetherGuest".into(),
                properties: Vec::new(),
                info: ClientInformation::new(2),
                brand: "test".into(),
                sender: PlayerSender::new(tx, Arc::default()),
                latency: Arc::new(AtomicI32::new(0)),
                address: "127.0.0.1:25565".parse().unwrap(),
                expected: false,
                configured: Default::default(),
            },
            2,
            [20.5, 64.0, 0.5],
            2,
        );
        guest.dimension = Dim::Nether.id().into();
        guest.placed = true;
        guest.loaded = true;
        game.lan.as_mut().unwrap().hub.guests.push(guest);
        let seen = game.for_player(super::super::commands::Target::Guest(2), |g| {
            assert_eq!(g.dim, Dim::Nether);
            // for_player already switched the level before entering the guest.
            // The host's body must not become a phantom blocker in the Nether.
            assert!(g.lan_building_boxes.boxes(Dim::Nether).is_empty());
            assert_eq!(
                g.lan_building_boxes.boxes(Dim::Overworld),
                vec![(
                    original.min_position().components(),
                    original.max_position().components()
                )]
            );
            g.player.as_ref().unwrap().position
        });
        assert_eq!(seen, Some(Vec3::new(20.5, 64.0, 0.5)));
        assert_eq!(game.dim, Dim::Overworld);
        assert_eq!(
            game.player.as_ref().unwrap().position,
            Vec3::new(10.5, 64.0, 0.5)
        );
        drop(game);
        assert!(scratch.starts_with(std::env::temp_dir()));
        std::fs::remove_dir_all(scratch).unwrap();
    }
}
