//! What the custom spawners (`CustomSpawner`: cats, the wandering trader, the
//! village siege) call on `ServerLevel`. They run once a tick at most, so the
//! level is a `&mut dyn` and their code is compiled here.
use horizon_client::{
    blocks::BlockFacts,
    join_seq::JoinList,
    mobs::{Mob, Species, raider, spawn_rules},
    world::World,
};
use horizon_core::geometry::Vec3;
use std::collections::HashSet;

/// The level's data as a custom spawner reads it: borrows only.
pub struct SpawnerParts<'a> {
    pub world: &'a mut World,
    pub mobs: &'a mut JoinList<Mob>,
    pub facts: &'a BlockFacts,
}

/// A player of the level (`ServerLevel.players()`).
pub struct SpawnerPlayer {
    pub uuid: u128,
    pub position: Vec3,
    pub alive: bool,
    pub spectator: bool,
}

/// The services the custom spawners call on their level beyond its data.
pub trait CustomSpawnerLevel {
    fn parts(&mut self) -> SpawnerParts<'_>;
    /// Whether this level is the overworld, the only one with custom spawners.
    fn overworld(&self) -> bool;
    /// `ServerLevel.getGameRules().get(rule)` for a boolean rule.
    fn rule(&self, name: &str) -> bool;
    /// `ServerLevel.players()`: the local player and the guests in the level.
    fn players(&self) -> Vec<SpawnerPlayer>;
    /// The sections that are village centres (`PoiManager`'s distance tracker).
    fn village_centers(&self) -> HashSet<(i32, i32, i32)>;
    /// `Level.getSkyDarken`.
    fn sky_darken(&self) -> i32;
    /// `Heightmap.Types.WORLD_SURFACE` at a column.
    fn surface_height(&self, x: i32, z: i32) -> i32;
    /// `Heightmap.Types.MOTION_BLOCKING_NO_LEAVES` at a column.
    fn motion_top(&self, x: i32, z: i32) -> i32;
    /// `Biome.is(tag)` at a block, through the spawn rules' biome zoom.
    fn biome_tag(&self, tag: &str, pos: [i32; 3]) -> bool;
    /// The chunk's own biome at a block is in the mob data's biome tag; false unloaded.
    fn chunk_biome_has(&self, tag: &str, pos: [i32; 3]) -> bool;
    /// The structures a structure tag lists.
    fn structure_tag(&self, tag: &str) -> Vec<String>;
    /// The structures whose pieces hold `pos`.
    fn structures_at(&self, pos: [i32; 3]) -> Vec<String>;
    /// `SpawnPlacements.isSpawnPositionOk`.
    fn is_spawn_position_ok(&self, species: Species, pos: [i32; 3]) -> bool;
    /// `SpawnPlacements.checkSpawnRules`, drawing from the level random.
    fn check_spawn_rules(
        &mut self,
        species: Species,
        reason: spawn_rules::Reason,
        pos: [i32; 3],
    ) -> bool;
    /// `EntityType.spawn` finalized for a natural spawn: the new mob's id.
    fn spawn_mob(&mut self, kind: &str, at: [f64; 3], persistent: bool) -> Option<u32>;
    /// `EntityType.create` and `finalizeSpawn` for a patrol or an event.
    fn spawn_event_mob(
        &mut self,
        kind: &str,
        at: [f64; 3],
        leader: bool,
        reason: raider::Reason,
    ) -> Option<u32>;
    /// The jockeys and riders a new mob brings (`finalizeSpawn`'s passengers).
    fn spawn_jockeys(&mut self, id: u32, natural: bool);
    /// Mods hear of a village siege's spawn position; true when one cancelled it.
    fn village_siege_cancelled(&mut self, player: u128, attempted: [f64; 3]) -> bool;
}

/// `SectionPos` of a block position.
pub fn section(p: [i32; 3]) -> (i32, i32, i32) {
    (p[0] >> 4, p[1] >> 4, p[2] >> 4)
}

/// `PoiManager.DistanceTracker`: sections from the nearest village centre,
/// 7 when farther than 6.
pub fn sections_to_village(centers: &HashSet<(i32, i32, i32)>, at: (i32, i32, i32)) -> i32 {
    centers
        .iter()
        .map(|c| {
            (c.0 - at.0)
                .abs()
                .max((c.1 - at.1).abs())
                .max((c.2 - at.2).abs())
        })
        .min()
        .map_or(7, |d| d.min(7))
}
