//! `CatSpawner`: every 1200 ticks a cat may appear near a random player,
//! in a village with enough occupied beds or inside a swamp hut.
use super::{
    Dim, Game,
    raids::{section, sections_to_village},
};
use horizon_client::mobs::{Species, spawn_rules};
use horizon_core::geometry::Vec3;
use horizon_gameplay::cat_spawner::{candidate, hut_takes_cat, village_takes_cat};

pub(super) use horizon_gameplay::cat_spawner::CatSpawner;

impl Game<'_> {
    /// `CatSpawner.tick` (the overworld's custom spawner).
    pub(super) fn tick_cat_spawner(&mut self) {
        if self.dim != Dim::Overworld || !self.creatures.cat.due() {
            return;
        }
        // ServerLevel.getRandomPlayer: one of the living players.
        let alive: Vec<Vec3> = self
            .level_players()
            .into_iter()
            .filter(|p| p.alive)
            .map(|p| p.position)
            .collect();
        let Some(pos) = candidate(&mut self.world.random, &alive, &self.world.chunks) else {
            return;
        };
        let ok = spawn_rules::is_spawn_position_ok(
            Species::Cat,
            &self.mob_world_base(&[]),
            pos[0],
            pos[1],
            pos[2],
        );
        if !ok {
            return;
        }
        if sections_to_village(&self.village_centers(), section(pos)) <= 2 {
            if village_takes_cat(&self.mobs, pos) {
                self.spawn_cat(pos, false);
            }
        } else {
            let hut = self.structure_tag("minecraft:cats_spawn_in");
            if self.structures_at(pos).iter().any(|s| hut.contains(s))
                && hut_takes_cat(&self.mobs, pos)
            {
                self.spawn_cat(pos, true);
            }
        }
    }
    /// `spawnCat`: finalized for a natural spawn, black inside
    /// `#cats_spawn_as_black` structures (`Cat.finalizeSpawn`).
    fn spawn_cat(&mut self, pos: [i32; 3], persistent: bool) {
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]),
            f64::from(pos[2]) + 0.5,
        ];
        let Some(id) = self.spawn_mob("minecraft:cat", at, persistent) else {
            return;
        };
        let black = self.structure_tag("minecraft:cats_spawn_as_black");
        let in_black = self.structures_at(pos).iter().any(|s| black.contains(s));
        if in_black && let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
            m.extra.variant = 10;
        }
    }
}
