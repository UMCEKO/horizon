//! Random scenarios must give the same bytes, in the same order, per connection as [`super::reference`].
use super::{Icon, PLAYER_RANGE, Player, Waypoints, reference};
use horizon_protocol::packets::play::{Clientbound as C, WaypointTarget, waypoint_operation};
use std::collections::BTreeMap;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }
    fn f(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn n(&mut self, k: usize) -> usize {
        (self.f() * k as f64) as usize % k
    }
    fn chance(&mut self, p: f64) -> bool {
        self.f() < p
    }
}

fn as_reference(p: &Player) -> reference::Player {
    reference::Player {
        uuid: p.uuid,
        conn: p.conn,
        position: p.position,
        spectator: p.spectator,
        transmit_range: p.transmit_range,
        receive_range: p.receive_range,
        first_tick: p.first_tick,
        view_distance: p.view_distance,
        team: p.team.clone(),
        icon: reference::Icon {
            style: p.icon.style.to_string(),
            color: p.icon.color,
        },
    }
}

/// The bytes sent to each connection, in order.
type Sent = BTreeMap<u32, Vec<Vec<u8>>>;

fn sent_by_reference(out: &[(u32, C)], into: &mut Sent) {
    for (conn, packet) in out {
        into.entry(*conn).or_default().push(packet.encode());
    }
}

fn sent_by_new(out: &[(u32, C)], into: &mut Sent) {
    for (conn, packet) in out {
        into.entry(*conn).or_default().push(packet.encode());
    }
}

fn first_difference(new: &Sent, old: &Sent) -> String {
    for conn in new.keys().chain(old.keys()) {
        let (a, b) = (new.get(conn), old.get(conn));
        if a != b {
            let (a, b) = (a.map_or(&[][..], |v| &v[..]), b.map_or(&[][..], |v| &v[..]));
            let i = (0..a.len().max(b.len()))
                .find(|&i| a.get(i) != b.get(i))
                .unwrap();
            return format!(
                "connection {conn}, packet {i} of {} and {}: {:?} against {:?}",
                a.len(),
                b.len(),
                a.get(i),
                b.get(i)
            );
        }
    }
    String::new()
}

/// What a scenario sent, to show the test saw every kind of packet.
#[derive(Default, Debug, PartialEq)]
pub struct Counts {
    pub packets: usize,
    pub track: usize,
    pub untrack: usize,
    pub update: usize,
    pub block: usize,
    pub chunk: usize,
    pub azimuth: usize,
}

impl Counts {
    fn add(&mut self, out: &[(u32, C)]) {
        for (_, packet) in out {
            let C::TrackedWaypoint(w) = packet else {
                panic!("{packet:?}");
            };
            self.packets += 1;
            match w.operation {
                waypoint_operation::TRACK => self.track += 1,
                waypoint_operation::UNTRACK => self.untrack += 1,
                _ => self.update += 1,
            }
            match w.target {
                WaypointTarget::Block(_) => self.block += 1,
                WaypointTarget::Chunk(_) => self.chunk += 1,
                WaypointTarget::Azimuth(_) => self.azimuth += 1,
                WaypointTarget::Empty => {}
            }
        }
    }
    fn merge(&mut self, o: &Counts) {
        self.packets += o.packets;
        self.track += o.track;
        self.untrack += o.untrack;
        self.update += o.update;
        self.block += o.block;
        self.chunk += o.chunk;
        self.azimuth += o.azimuth;
    }
}

struct Entry {
    player: Player,
    /// The level the player is in (None: gone for good).
    level: Option<usize>,
}

fn new_player(rng: &mut Rng, next: &mut u128, spread: f64, tall: bool) -> Player {
    *next += 1;
    Player {
        uuid: *next * 0x1_0000_0001,
        conn: Some(*next as u32),
        position: [
            (rng.f() - 0.5) * 2.0 * spread,
            if tall { -60.0 + rng.f() * 376.0 } else { 70.0 },
            (rng.f() - 0.5) * 2.0 * spread,
        ],
        spectator: false,
        transmit_range: PLAYER_RANGE,
        receive_range: PLAYER_RANGE,
        first_tick: true,
        view_distance: if tall { 2 + rng.n(31) as i32 } else { 10 },
        team: None,
        icon: Icon::default(),
    }
}

/// Joins, leaves, respawns, level changes, hiding, team/mode/range/icon changes and walks, fed to both managers.
pub fn scenario(seed: u64, count: usize, ticks: usize) -> Counts {
    let mut rng = Rng::new(seed);
    let spread = [20.0, 200.0, 2000.0][seed as usize % 3];
    let tall = seed.is_multiple_of(2);
    let mut next = 0u128;
    let mut all: Vec<Entry> = (0..count)
        .map(|_| Entry {
            player: new_player(&mut rng, &mut next, spread, tall),
            level: Some(rng.n(2)),
        })
        .collect();
    let mut news = [Waypoints::default(), Waypoints::default()];
    let mut olds = [
        reference::Waypoints::default(),
        reference::Waypoints::default(),
    ];
    let mut rule = true;
    let mut counts = Counts::default();
    for tick in 0..ticks {
        let mut respawned = Vec::new();
        for i in 0..all.len() {
            let gone = all[i].level.is_none();
            if gone {
                continue;
            }
            if rng.chance(0.003) {
                all[i].level = None;
                let fresh = new_player(&mut rng, &mut next, spread, tall);
                all.push(Entry {
                    player: fresh,
                    level: Some(rng.n(2)),
                });
                continue;
            }
            let e = &mut all[i];
            let p = &mut e.player;
            p.first_tick = false;
            if rng.chance(0.8) {
                let step = if rng.chance(0.05) { 40.0 } else { 0.6 };
                p.position[0] += (rng.f() - 0.5) * step;
                p.position[2] += (rng.f() - 0.5) * step;
                if tall {
                    p.position[1] =
                        (p.position[1] + (rng.f() - 0.5) * step * 4.0).clamp(-64.0, 320.0);
                }
            }
            if rng.chance(0.01) {
                e.level = e.level.map(|l| 1 - l);
                p.first_tick = true;
            }
            if rng.chance(0.005) {
                respawned.push(p.uuid);
            }
            if rng.chance(0.01) {
                p.transmit_range = if p.transmit_range > 0.0 {
                    0.0
                } else {
                    PLAYER_RANGE
                };
            }
            if rng.chance(0.004) {
                p.transmit_range = [40.0, 150.0, 331.5, 332.5, 1000.0, -1.0][rng.n(6)];
            }
            if rng.chance(0.004) {
                p.receive_range = [PLAYER_RANGE, 0.0, 80.0, 400.0, PLAYER_RANGE][rng.n(5)];
            }
            if rng.chance(0.005) {
                p.spectator = !p.spectator;
            }
            if rng.chance(0.005) {
                p.team = p
                    .team
                    .is_none()
                    .then(|| ("t".into(), Some(rng.n(1000) as i32)));
            }
            if rng.chance(0.004) {
                p.icon = Icon {
                    style: ["minecraft:default", "minecraft:bowtie"][rng.n(2)].into(),
                    color: rng.chance(0.5).then(|| rng.n(0x100_0000) as i32),
                };
            }
        }
        if rng.chance(0.01) {
            rule = !rule;
        }
        let (mut new_sent, mut old_sent) = (Sent::new(), Sent::new());
        for level in 0..2 {
            let here: Vec<&Player> = all
                .iter()
                .filter(|e| e.level == Some(level))
                .map(|e| &e.player)
                .collect();
            let away: Vec<(u128, u32)> = all
                .iter()
                .filter(|e| e.level == Some(1 - level))
                .map(|e| (e.player.uuid, e.player.conn.unwrap()))
                .collect();
            if here.is_empty() {
                news[level] = Waypoints::default();
                olds[level] = reference::Waypoints::default();
                continue;
            }
            let players: Vec<Player> = here.into_iter().cloned().collect();
            let old_players: Vec<reference::Player> = players.iter().map(as_reference).collect();
            let new = news[level].tick_level(&players, &away, &respawned, rule);
            let old = olds[level].tick_level(&old_players, &away, &respawned, rule);
            counts.add(&old);
            sent_by_new(&new, &mut new_sent);
            sent_by_reference(&old, &mut old_sent);
        }
        for level in 0..2 {
            let (new, old) = (news[level].dump(), olds[level].dump());
            let only_new: Vec<_> = new.iter().filter(|x| !old.contains(x)).collect();
            let only_old: Vec<_> = old.iter().filter(|x| !new.contains(x)).collect();
            assert!(
                new == old,
                "seed {seed} tick {tick} level {level}: connections only in the new {only_new:?}, only in the old {only_old:?}"
            );
        }
        if new_sent != old_sent {
            panic!(
                "seed {seed} tick {tick}: {}",
                first_difference(&new_sent, &old_sent)
            );
        }
    }
    counts
}

fn run_seeds(seeds: std::ops::Range<u64>, count: usize, ticks: usize) -> Counts {
    let mut total = Counts::default();
    for seed in seeds {
        total.merge(&scenario(seed, count, ticks));
    }
    total
}

fn assert_saw_everything(c: &Counts) {
    for (what, n) in [
        ("track", c.track),
        ("untrack", c.untrack),
        ("update", c.update),
        ("block", c.block),
        ("chunk", c.chunk),
        ("azimuth", c.azimuth),
    ] {
        assert!(n > 100, "the scenarios sent {n} {what} packets: {c:?}");
    }
}

#[test]
fn matches_the_reference_over_random_scenarios() {
    let total = run_seeds(1..17, 30, 300);
    println!("16 seeds x 300 ticks x 2 levels: {total:?}");
    assert_saw_everything(&total);
}

#[test]
#[ignore = "the large run: 400 scenarios, some with 120 players"]
fn matches_the_reference_over_many_more_scenarios() {
    let mut total = run_seeds(100..500, 30, 300);
    total.merge(&run_seeds(500..510, 120, 300));
    println!("410 seeds x 300 ticks x 2 levels: {total:?}");
    assert_saw_everything(&total);
}

/// Two players `distance` apart on one side of a level, ticked twice with the source nudged between.
fn pair_at(distance: f64, transmit_range: f64, receive_range: f64) -> Counts {
    let p = |uuid: u128, x: f64| Player {
        uuid,
        conn: Some(uuid as u32),
        position: [x, 64.0, 0.0],
        spectator: false,
        transmit_range,
        receive_range,
        first_tick: false,
        view_distance: 4,
        team: None,
        icon: Icon::default(),
    };
    let (mut new, mut old) = (Waypoints::default(), reference::Waypoints::default());
    let mut counts = Counts::default();
    let mut source = p(2, distance);
    for nudge in [0.0, 1.0, 0.00001, -0.00002] {
        source.position[0] += nudge;
        let players = [p(1, 0.0), source.clone()];
        let old_players = players.iter().map(as_reference).collect::<Vec<_>>();
        let (mut a, mut b) = (Sent::new(), Sent::new());
        let out = new.tick(&players, &[], true);
        sent_by_new(&out, &mut a);
        let out = old.tick(&old_players, &[], true);
        sent_by_reference(&out, &mut b);
        assert_eq!(
            a, b,
            "distance {distance} ranges {transmit_range} {receive_range}"
        );
        counts.add(&out);
    }
    counts
}

#[test]
fn matches_the_reference_at_the_edges_of_a_range_and_of_really_far() {
    let mut rng = Rng::new(7);
    let mut counts = Counts::default();
    for range in [64.0, 332.0, 1000.0, PLAYER_RANGE, 1.0e39] {
        for k in -40..=40 {
            let edge = range * (1.0 + f64::from(k) * 1.0e-7);
            counts.merge(&pair_at(edge, range, PLAYER_RANGE));
            counts.merge(&pair_at(edge, PLAYER_RANGE, range));
        }
        for _ in 0..200 {
            let edge = range * (1.0 + (rng.f() - 0.5) * 2.0e-6);
            counts.merge(&pair_at(edge, range, range));
        }
    }
    for k in -400..=400 {
        counts.merge(&pair_at(
            332.0 + f64::from(k) * 5.0e-6,
            PLAYER_RANGE,
            PLAYER_RANGE,
        ));
    }
    counts.merge(&pair_at(20.0, PLAYER_RANGE, PLAYER_RANGE));
    counts.merge(&pair_at(10.0, -5.0, PLAYER_RANGE));
    counts.merge(&pair_at(10.0, PLAYER_RANGE, -5.0));
    println!("edges: {counts:?}");
    assert!(
        counts.azimuth > 100 && counts.block > 100 && counts.update > 0,
        "{counts:?}"
    );
}
