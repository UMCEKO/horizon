//! The players a `tick_block_entities` pass reads, listed once and frozen for the pass.
use super::{Game, dragon::FightPlayer, natural_spawning::LevelView};
use horizon_client::{
    mobs::{spawn_selection::PlayerCells, trial_spawner::TrialPlayer},
    vault::VaultPlayer,
};
use horizon_core::geometry::Aabb;
use horizon_world::loot::LootData;
use std::{cell::OnceCell, sync::Arc};

/// Fewer players than this are scanned: a grid costs more than it saves.
const GRID_MIN: usize = 16;

/// The chunks around a point that hold every player within `reach` blocks of it.
pub(super) fn reach_chunks(reach: f64) -> i32 {
    ((reach.abs() / 16.0).ceil() as i32).saturating_add(1)
}
pub(super) fn chunk_of(v: f64) -> i32 {
    (v.floor() as i32) >> 4
}
/// The players' chunks, when there are enough of them to be worth it.
pub(super) fn grid(points: impl ExactSizeIterator<Item = (f64, f64)>) -> Option<PlayerCells> {
    (points.len() >= GRID_MIN).then(|| PlayerCells::new(points))
}

/// Players standing at blocks, with the chunks they are in.
pub(super) struct Listed<T> {
    players: Vec<T>,
    cells: Option<PlayerCells>,
}
impl<T: Copy> Listed<T> {
    fn new(players: Vec<T>, block: impl Fn(&T) -> [i32; 3]) -> Self {
        let cells = grid(players.iter().map(|p| {
            let at = block(p);
            (f64::from(at[0]), f64::from(at[2]))
        }));
        Self { players, cells }
    }
    pub(super) fn all(&self) -> &[T] {
        &self.players
    }
    /// A superset, in order, of the players whose block is closer than `range` to `at`.
    pub(super) fn near(&self, at: [i32; 3], range: f64) -> Vec<T> {
        let Some(cells) = &self.cells else {
            return self.players.clone();
        };
        let mut found = Vec::new();
        let (cx, cz) = (chunk_of(f64::from(at[0])), chunk_of(f64::from(at[2])));
        cells.within(cx, cz, reach_chunks(range), &mut found);
        found.into_iter().map(|i| self.players[i]).collect()
    }
}

/// The players of `Game::fight_players`, with the chunks of their positions.
pub(super) struct Fighters {
    players: Vec<FightPlayer>,
    cells: Option<PlayerCells>,
    /// The farthest a player's box reaches from its position on x or z.
    reach: f64,
}
impl Fighters {
    fn new(players: Vec<FightPlayer>) -> Self {
        let reach = players.iter().fold(0.0_f64, |reach, p| {
            let (lo, hi) = (p.bounds.min_position(), p.bounds.max_position());
            let edges = [lo.x - p.position.x, hi.x - p.position.x];
            let sides = [lo.z - p.position.z, hi.z - p.position.z];
            edges
                .into_iter()
                .chain(sides)
                .fold(reach, |r, d| r.max(d.abs()))
        });
        let cells = grid(players.iter().map(|p| (p.position.x, p.position.z)));
        Self {
            players,
            cells,
            reach,
        }
    }
    /// In order, the alive players whose box meets `area`.
    pub(super) fn touching(&self, area: Aabb) -> Vec<FightPlayer> {
        let hit = |p: &&FightPlayer| p.alive && p.bounds.intersects(area);
        let Some(cells) = &self.cells else {
            return self.players.iter().filter(hit).copied().collect();
        };
        let (lo, hi) = (area.min_position(), area.max_position());
        let half = (hi.x - lo.x).max(hi.z - lo.z) / 2.0 + self.reach;
        let mut found = Vec::new();
        let (cx, cz) = (chunk_of((lo.x + hi.x) / 2.0), chunk_of((lo.z + hi.z) / 2.0));
        cells.within(cx, cz, reach_chunks(half), &mut found);
        let near = found.into_iter().map(|i| &self.players[i]);
        near.filter(hit).copied().collect()
    }
}

/// The lists a pass reads, each built at its first ask.
#[derive(Default)]
pub(super) struct PhasePlayers {
    level: OnceCell<LevelView>,
    vault: OnceCell<Listed<VaultPlayer>>,
    trial: OnceCell<Listed<TrialPlayer>>,
    fight: OnceCell<Fighters>,
    loot: OnceCell<Option<Arc<LootData>>>,
}
impl PhasePlayers {
    /// `Game::level_players`, with their chunks.
    pub(super) fn level(&self, game: &Game) -> &LevelView {
        let view = self
            .level
            .get_or_init(|| LevelView::new(game.level_players(), true));
        #[cfg(debug_assertions)]
        same("level", &view.players, &game.level_players());
        view
    }
    /// `Game::vault_players`.
    pub(super) fn vault(&self, game: &Game) -> &Listed<VaultPlayer> {
        let list = self
            .vault
            .get_or_init(|| Listed::new(game.vault_players(), |p| p.block));
        #[cfg(debug_assertions)]
        same("vault", list.all(), &game.vault_players());
        list
    }
    /// `Game::trial_players`.
    pub(super) fn trial(&self, game: &Game) -> &Listed<TrialPlayer> {
        let list = self
            .trial
            .get_or_init(|| Listed::new(game.trial_players(), |p| p.block));
        #[cfg(debug_assertions)]
        same("trial", list.all(), &game.trial_players());
        list
    }
    /// `Game::fight_players`.
    pub(super) fn fight(&self, game: &Game) -> &Fighters {
        let fighters = self
            .fight
            .get_or_init(|| Fighters::new(game.fight_players()));
        #[cfg(debug_assertions)]
        same("fight", &fighters.players, &game.fight_players());
        fighters
    }
    /// The loot tables, fetched once for the pass.
    pub(super) fn loot(&self, game: &Game) -> &Option<Arc<LootData>> {
        self.loot.get_or_init(|| {
            horizon_client::structure_blocks::loot_data(&game.options.resources, &game.pack_files())
        })
    }
    /// Forgets the players: something ran that may have changed one.
    pub(super) fn invalidate(&mut self) {
        self.level.take();
        self.vault.take();
        self.trial.take();
        self.fight.take();
    }
    /// The end of a pass: each list built is what the players are now.
    #[cfg(debug_assertions)]
    pub(super) fn verify(&self, game: &Game) {
        let (level, vault, trial, fight) = (
            self.level.get().is_some(),
            self.vault.get().is_some(),
            self.trial.get().is_some(),
            self.fight.get().is_some(),
        );
        if level {
            self.level(game);
        }
        if vault {
            self.vault(game);
        }
        if trial {
            self.trial(game);
        }
        if fight {
            self.fight(game);
        }
    }
}

#[cfg(debug_assertions)]
fn same<T: std::fmt::Debug>(what: &str, frozen: &[T], now: &[T]) {
    let (frozen, now) = (format!("{frozen:?}"), format!("{now:?}"));
    assert!(
        frozen == now,
        "the {what} players changed during the block entity pass:\n{frozen}\n{now}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// splitmix64 in [0, 1).
    fn unit(state: &mut u64) -> f64 {
        *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    }

    #[test]
    fn near_misses_nobody_a_scan_finds() {
        let mut state = 7;
        for (count, spread) in [
            (1, 40.0),
            (5, 300.0),
            (40, 20_000.0),
            (300, 3_000_000.0),
            (300, 400.0),
        ] {
            let points: Vec<(f64, f64)> = (0..count)
                .map(|_| {
                    (
                        (unit(&mut state) - 0.5) * spread,
                        (unit(&mut state) - 0.5) * spread,
                    )
                })
                .collect();
            let cells = PlayerCells::new(points.iter().copied());
            for ask in 0..300 {
                // Half the asks start beside a player, so something is near.
                let (qx, qz) = if ask % 2 == 0 {
                    let p = points[(unit(&mut state) * count as f64) as usize];
                    (
                        p.0 + unit(&mut state) * 20.0 - 10.0,
                        p.1 + unit(&mut state) * 20.0 - 10.0,
                    )
                } else {
                    (
                        (unit(&mut state) - 0.5) * spread,
                        (unit(&mut state) - 0.5) * spread,
                    )
                };
                let radius = [0.0, 3.0, 16.0, 17.0, 64.0, 150.5, 1000.0][ask % 7];
                let scan: Vec<usize> = (0..count)
                    .filter(|&i| {
                        (points[i].0 - qx).powi(2) + (points[i].1 - qz).powi(2) < radius * radius
                    })
                    .collect();
                let mut found = Vec::new();
                cells.within(chunk_of(qx), chunk_of(qz), reach_chunks(radius), &mut found);
                assert!(
                    found.windows(2).all(|w| w[0] < w[1]),
                    "once each, in the players' order"
                );
                let exact: Vec<usize> = found
                    .into_iter()
                    .filter(|&i| {
                        (points[i].0 - qx).powi(2) + (points[i].1 - qz).powi(2) < radius * radius
                    })
                    .collect();
                assert_eq!(
                    exact, scan,
                    "radius {radius} around ({qx}, {qz}) of {count} spread {spread}"
                );
                let any = cells.any_within(chunk_of(qx), chunk_of(qz), reach_chunks(radius), |i| {
                    scan.contains(&i)
                });
                assert_eq!(any, !scan.is_empty());
            }
        }
    }
}
