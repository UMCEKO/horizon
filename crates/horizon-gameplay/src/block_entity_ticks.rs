//! `Level.tickBlockEntities` helpers: the ticker name a sample is attributed to and the
//! block-ticking area of a level.
use horizon_client::{containers::BlockEntity, fast_hash::FxHashSet, ticket_levels::Levels};

/// The block entity type a sample is attributed to while its ticker runs.
pub fn ticker_name(entity: &BlockEntity) -> &'static str {
    match entity {
        BlockEntity::Chest(_) => "hopper",
        BlockEntity::Crafter(_) => "crafter",
        BlockEntity::Jukebox(_) => "jukebox",
        BlockEntity::Beacon(_) => "beacon",
        BlockEntity::CreakingHeart(_) => "creaking_heart",
        BlockEntity::Spawner(_) => "mob_spawner",
        BlockEntity::TrialSpawner(_) => "trial_spawner",
        BlockEntity::Vault(_) => "vault",
        BlockEntity::Beehive(_) => "beehive",
        BlockEntity::Furnace(_) => "furnace",
        BlockEntity::Brewing(_) => "brewing_stand",
        BlockEntity::DaylightDetector => "daylight_detector",
        BlockEntity::Campfire(_) => "campfire",
        BlockEntity::Mod(_) => "mod_block_entity",
        BlockEntity::PotentSulfur(_) => "potent_sulfur",
        _ => "block_entity",
    }
}

/// `LevelChunk.isTicking` (block coordinates): a block-ticking chunk from
/// every player's tickets (`update_chunk_tickets`), its entities loaded
/// (`ServerLevel.areEntitiesLoaded`).
pub fn block_ticking_area(
    levels: Levels,
    entities_pending: &FxHashSet<(i32, i32)>,
) -> impl Fn(i32, i32) -> bool + use<> {
    let has_pending = !entities_pending.is_empty();
    let pending = has_pending.then(|| entities_pending.clone());
    move |x, z| {
        levels.block_ticking_at(x, z)
            && pending
                .as_ref()
                .is_none_or(|p| !p.contains(&(x >> 4, z >> 4)))
    }
}
