//! Player statistics (`ServerStatsCounter`): the custom counters, blocks
//! mined, items used/crafted/broken/picked up/dropped and mobs killed or
//! killed by, saved to `players/stats/<uuid>.json`, plus the phantom
//! spawner that reads the time since rest (`PhantomSpawner`).
use super::{Dim, Game};
use horizon_core::geometry::Vec3;
use horizon_core::random::RandomSource;
pub(super) use horizon_gameplay::stats::{Stats, locate_player_json, player_json};
use horizon_gameplay::stats::{namespaced, phantom_group_size};
use std::path::PathBuf;

#[cfg(test)]
#[path = "phantom_spawn_tests.rs"]
mod phantom_spawn_tests;

impl Game<'_> {
    fn stats_path(&self) -> Option<PathBuf> {
        Some(locate_player_json(
            &self.world_root,
            "stats",
            self.level.player_uuid?,
            &self.options.player_name(),
        ))
    }
    /// `ServerStatsCounter.parseLocal`.
    pub(super) fn load_stats(&mut self) {
        if self.stats.loaded || self.remote.is_some() || self.no_player_here() {
            return;
        }
        self.stats.loaded = true;
        if let Some(path) = self.stats_path() {
            self.stats.values = Stats::read(&path).values;
        }
    }
    /// `ServerStatsCounter.save`.
    pub(super) fn save_stats(&self) {
        if !self.stats.loaded || self.remote.is_some() || self.no_player_here() {
            return;
        }
        if let Some(path) = self.stats_path() {
            self.stats.write(&path);
        }
    }
    /// `ServerPlayer.awardStat`: `n` more of the statistic `ty` (a bare type
    /// such as `used`) and `key`, unless a native mod cancels the award.
    pub(super) fn award_stat(&mut self, ty: &str, key: &str, n: i32) {
        // No mod, no name copied: the per-tick counters come through here.
        let acting = if horizon_client::mod_api::mods().is_empty() {
            None
        } else {
            self.lan_acting_uuid.zip(self.lan_acting_name.clone())
        };
        if let Some((player, name)) = acting {
            let mut cancel = false;
            namespaced(ty, |stat_type| {
                self.native_player_stat_award(player, &name, stat_type, key, n, &mut cancel)
            });
            if cancel {
                return;
            }
        }
        self.stats.add(ty, key, n);
    }
    /// `Player.awardStat(Identifier)`: `n` more of the custom statistic `key`.
    pub(super) fn award_custom_stat(&mut self, key: &str, n: i32) {
        namespaced(key, |key| self.award_stat("custom", key, n));
    }
    /// The per-tick counters (`Player.tick`/`ServerPlayer.tick` awardStat).
    pub(super) fn tick_stats(&mut self) {
        self.load_stats();
        self.count_tick_stats(self.sleeping.is_some());
        self.tick_movement_stats();
    }
    /// `Player.tick`'s counters for the player in `self.player`.
    pub(super) fn count_tick_stats(&mut self, sleeping: bool) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let alive = !p.health.dead_or_dying();
        let crouching = p.pose == horizon_client::pose::Pose::Crouching;
        self.award_custom_stat("play_time", 1);
        self.award_custom_stat("total_world_time", 1);
        if alive {
            self.award_custom_stat("time_since_death", 1);
        }
        if crouching {
            self.award_custom_stat("sneak_time", 1);
        }
        // ServerPlayer.doTick: dead or alive, every tick out of bed.
        if !sleeping {
            self.award_custom_stat("time_since_rest", 1);
        }
    }
    fn tick_movement_stats(&mut self) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let d = p.position.subtract(self.previous_position);
        self.movement_stats(d);
    }
    /// `Player.checkMovementStatistics` / `checkRidingStatistics` for one move.
    pub(super) fn movement_stats(&mut self, d: Vec3) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let horizontal = ((d.x * d.x + d.z * d.z).sqrt() * 100.0).round() as i32;
        let total = ((d.x * d.x + d.y * d.y + d.z * d.z).sqrt() * 100.0).round() as i32;
        // Teleports are not travel.
        if total <= 0 || total > 1000 {
            return;
        }
        let crouching = p.pose == horizon_client::pose::Pose::Crouching;
        let feet = [
            p.position.x.floor() as i32,
            p.position.y.floor() as i32,
            p.position.z.floor() as i32,
        ];
        let eye_in_water = self.res.facts().get(self.world.block(
            feet[0],
            (p.position.y + f64::from(p.dimensions.eye_height)).floor() as i32,
            feet[2],
        ));
        let in_water = self
            .res
            .facts()
            .get(self.world.block(feet[0], feet[1], feet[2]))
            .fluid
            == horizon_client::blocks::Fluid::Water;
        let on_ground = p.ground.on_ground;
        let (sprinting, gliding, swimming) = (
            p.sprinting,
            p.fall_flying,
            p.pose == horizon_client::pose::Pose::Swimming,
        );
        let climbing = p.last_climbable.is_some() && d.y > 0.0;
        let riding = self.riding.map(|id| {
            self.mobs
                .iter()
                .find(|m| m.id == id)
                .map(|m| m.species())
                .ok_or(())
        });
        match riding {
            Some(Ok(sp)) => {
                use horizon_client::mobs::Species;
                // `checkRidingStatistics`: only these mounts count, the horse
                // family by the full 3D distance; any other mount nothing.
                match sp {
                    Species::Pig => self.award_custom_stat("pig_one_cm", total),
                    Species::Strider => self.award_custom_stat("strider_one_cm", total),
                    Species::HappyGhast => self.award_custom_stat("happy_ghast_one_cm", total),
                    Species::Nautilus | Species::ZombieNautilus => {
                        self.award_custom_stat("nautilus_one_cm", total)
                    }
                    sp if horizon_client::mobs::equine::is_equine(sp) => {
                        self.award_custom_stat("horse_one_cm", total)
                    }
                    _ => {}
                }
            }
            Some(Err(())) => {
                if self.boats.iter().any(|b| Some(b.id) == self.riding) {
                    self.award_custom_stat("boat_one_cm", total)
                } else if self.carts.iter().any(|c| Some(c.id) == self.riding) {
                    self.award_custom_stat("minecart_one_cm", total)
                }
            }
            None if swimming => self.award_custom_stat("swim_one_cm", total),
            None if eye_in_water.fluid == horizon_client::blocks::Fluid::Water => {
                self.award_custom_stat("walk_under_water_one_cm", total)
            }
            None if in_water => {
                if horizontal > 0 {
                    self.award_custom_stat("walk_on_water_one_cm", horizontal)
                }
            }
            None if climbing => {
                self.award_custom_stat("climb_one_cm", (d.y * 100.0).round() as i32)
            }
            None if on_ground => {
                if horizontal > 0 {
                    if sprinting {
                        self.award_custom_stat("sprint_one_cm", horizontal)
                    } else if crouching {
                        self.award_custom_stat("crouch_one_cm", horizontal)
                    } else {
                        self.award_custom_stat("walk_one_cm", horizontal)
                    }
                }
            }
            None if gliding => self.award_custom_stat("aviate_one_cm", total),
            // Airborne and not gliding: more than 25 cm sideways per move
            // (creative flight, or a long jump), not only flying.
            None if horizontal > 25 => self.award_custom_stat("fly_one_cm", horizontal),
            None => {}
        }
    }
    /// `PhantomSpawner.tick`: tired players under an open night sky draw
    /// phantoms. A custom spawner ticks only while `spawn_mobs` is on
    /// (`ServerChunkCache.tickChunks`), and this one only spawns enemies
    /// (`ServerLevel.isSpawningMonsters`: `spawn_monsters` too). Every
    /// player of the level counts: the game's own and the hosted ones (a
    /// server thread's players are all hosted).
    pub(super) fn spawn_phantoms(&mut self) {
        let rules = &self.ui.rules;
        if !rules.bool("spawn_mobs")
            || !rules.bool("spawn_monsters")
            || !rules.bool("spawn_phantoms")
            || self.dim != Dim::Overworld
        {
            return;
        }
        if !self.stats.phantom_due() {
            return;
        }
        let r = &mut self.world.random;
        self.stats
            .phantom_delay((60 + r.next_int_bound(60).unwrap_or(0)) * 20);
        if self.sky_darken() < 5 {
            return;
        }
        let rest = |s: &Stats| s.get("custom", "minecraft:time_since_rest");
        // `level.players()`: (block position, time since rest) of each player
        // who isn't a spectator.
        let mut players = Vec::new();
        if let Some(p) = self.player.as_ref()
            && !self.spectator()
        {
            players.push((p.position, rest(&self.stats)));
        }
        if let Some(host) = self.lan.as_ref() {
            let level = self.dim.id();
            for g in host.hub.guests.iter().filter(|g| {
                g.placed
                    && g.dimension == level
                    && g.game_mode != horizon_client::lan::guest::SPECTATOR
            }) {
                let since = host.play.get(&g.conn).map_or(0, |p| rest(&p.stats));
                players.push((
                    Vec3::new(g.position[0], g.position[1], g.position[2]),
                    since,
                ));
            }
        }
        for (position, since) in players {
            self.spawn_phantoms_near(position, since);
        }
    }
    /// One player's turn of `PhantomSpawner.tick`.
    fn spawn_phantoms_near(&mut self, position: Vec3, since_rest: i32) {
        let pos = [
            position.x.floor() as i32,
            position.y.floor() as i32,
            position.z.floor() as i32,
        ];
        // `playerPos.getY() >= level.getSeaLevel() && level.canSeeSky(playerPos)`:
        // sky light 15 at the position, whatever stands above it, so a glass
        // or stained-glass roof still spawns phantoms (`MobWorld` reads the
        // same rule for the spawn rules and the mob goals).
        if pos[1] < 63 || !self.mob_world_base(&[]).can_see_sky(pos[0], pos[1], pos[2]) {
            return;
        }
        // `getCurrentDifficultyAt(playerPos).isHarderThan(random * 3)`.
        let difficulty = self.local_difficulty([position.x, position.y, position.z]);
        let r = &mut self.world.random;
        if difficulty.effective <= r.next_float() * 3.0 {
            return;
        }
        if r.next_int_bound(since_rest.max(1)).unwrap_or(0) < 72000 {
            return;
        }
        // `playerPos.above(20 + nextInt(15)).east(-10 + nextInt(21)).south(...)`:
        // the height is drawn first.
        let up = 20 + r.next_int_bound(15).unwrap_or(0);
        let east = -10 + r.next_int_bound(21).unwrap_or(10);
        let south = -10 + r.next_int_bound(21).unwrap_or(10);
        let at = [pos[0] + east, pos[1] + up, pos[2] + south];
        if !self
            .res
            .facts()
            .is_air(self.world.block(at[0], at[1], at[2]))
        {
            return;
        }
        let difficulty_id = self.difficulty_id();
        let group = phantom_group_size(&mut self.world.random, difficulty_id);
        for _ in 0..group {
            let _ = self.spawn_mob(
                "minecraft:phantom",
                [
                    f64::from(at[0]) + 0.5,
                    f64::from(at[1]),
                    f64::from(at[2]) + 0.5,
                ],
                false,
            );
        }
    }
}
