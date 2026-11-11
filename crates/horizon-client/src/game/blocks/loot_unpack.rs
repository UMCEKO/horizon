//! `RandomizableContainer.unpackLootTable`: a container keeps its loot table
//! and seed until its first access, which rolls them into its slots.
use super::Game;
use horizon_client::{
    biome::Biomes, containers::BlockEntity, save, structure_blocks, world::World,
};
use horizon_world::loot::LootData;

type Pos = (i32, i32, i32);

/// The table pending at `key`, if any.
pub(super) fn pending_table(world: &World, key: Pos) -> Option<&save::PendingLoot> {
    world
        .chunks
        .get(&(key.0 >> 4, key.2 >> 4))?
        .saved
        .as_ref()?
        .pending_loot
        .iter()
        .find(|r| r.pos == key)
}

/// `setLootTable(null)` and `setChanged`: the pending table taken off the container.
fn take_pending(world: &mut World, key: Pos) -> Option<save::PendingLoot> {
    pending_table(world, key)?;
    let chunk_key = (key.0 >> 4, key.2 >> 4);
    let chunk = std::sync::Arc::make_mut(world.chunks.get_mut(&chunk_key)?);
    let mut extras = chunk
        .saved
        .take()
        .map(std::sync::Arc::unwrap_or_clone)
        .unwrap_or_default();
    let pending = save::take_pending_loot(&mut extras, key);
    chunk.saved = Some(std::sync::Arc::new(extras));
    world.modified.insert(chunk_key);
    pending
}

/// `unpackLootTable` on the level: the pending table is cleared before the
/// fill, then rolled into the container with `luck`. Returns the table that rolled.
pub(super) fn unpack_in(
    world: &mut World,
    biomes: &Biomes,
    loot: Option<&LootData>,
    key: Pos,
    luck: f32,
) -> Option<String> {
    let pending = take_pending(world, key)?;
    let biome = world
        .chunks
        .get(&(key.0 >> 4, key.2 >> 4))
        .map(|c| biomes.get(c.biome(key.0 & 15, key.1, key.2 & 15)).name.clone())
        .unwrap_or_default();
    if let Some(BlockEntity::Chest(slots)) = world.block_entities.get_mut(&key) {
        structure_blocks::roll_container(
            slots,
            loot,
            &pending.table,
            pending.seed,
            key,
            &biome,
            luck,
        );
    }
    Some(pending.table)
}

impl Game<'_> {
    /// `BaseContainerBlockEntity.canOpen`'s loot check: a table still to unpack.
    pub(super) fn loot_pending(&self, key: Pos) -> bool {
        pending_table(&self.world, key).is_some()
    }
    /// `RandomizableContainer.unpackLootTable(null)`: a hopper, a dropper, a
    /// command or a break reads the container.
    pub(super) fn unpack_loot_table(&mut self, key: Pos) {
        if !self.loot_pending(key) {
            return;
        }
        let loot = self.server_loot.get();
        let biomes = world_biomes!(self).clone();
        unpack_in(&mut self.world, &biomes, loot.as_deref(), key, 0.0);
    }
    /// `RandomizableContainer.unpackLootTable(player)` for the player who opens
    /// the container: `player_generates_container_loot` fires first, then the
    /// roll takes the player's luck (`LootParams.withLuck`).
    pub(super) fn unpack_loot_table_for_player(&mut self, key: Pos) {
        let Some(table) = pending_table(&self.world, key).map(|p| p.table.clone()) else {
            return;
        };
        self.trigger_loot_table(&table);
        let luck = self
            .player
            .as_mut()
            .and_then(|p| p.attribute("minecraft:luck").ok())
            .unwrap_or(0.0) as f32;
        let loot = self.server_loot.get();
        let biomes = world_biomes!(self).clone();
        unpack_in(&mut self.world, &biomes, loot.as_deref(), key, luck);
    }
    /// The menu provider's unpack when a player opens a container:
    /// `ChestBlock.getMenuProvider` unpacks both halves of a double chest.
    pub(super) fn unpack_opened_loot(&mut self, key: Pos) {
        match self.chest_partner(key) {
            Some((other, true)) => {
                self.unpack_loot_table_for_player(key);
                self.unpack_loot_table_for_player(other);
            }
            Some((other, false)) => {
                self.unpack_loot_table_for_player(other);
                self.unpack_loot_table_for_player(key);
            }
            None => self.unpack_loot_table_for_player(key),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_client::chunk::{Chunk, ChunkExtras};
    use std::sync::Arc;

    fn world_with_pending(key: Pos) -> World {
        let mut world = World::new(horizon_world::registry::BlockStateId(0), 0);
        let mut chunk = Chunk::empty(0, 0);
        chunk.saved = Some(Arc::new(ChunkExtras {
            pending_loot: vec![save::PendingLoot {
                pos: key,
                table: "minecraft:chests/simple_dungeon".into(),
                seed: Some(12345),
            }],
            ..Default::default()
        }));
        world.chunks.insert((0, 0), Arc::new(chunk));
        world
            .block_entities
            .insert(key, BlockEntity::Chest(vec![None; 27]));
        world
    }

    #[test]
    fn unpack_clears_table_before_fill() {
        let key = (1, 64, 2);
        let mut world = world_with_pending(key);
        let taken = take_pending(&mut world, key).expect("pending table");
        assert_eq!(taken.table, "minecraft:chests/simple_dungeon");
        assert_eq!(taken.seed, Some(12345));
        // What the fill sees: a container with no table left to unpack.
        assert!(pending_table(&world, key).is_none());
        assert!(world.modified.contains(&(0, 0)));
    }

    #[test]
    fn second_unpack_is_a_noop() {
        let key = (1, 64, 2);
        let mut world = world_with_pending(key);
        assert!(take_pending(&mut world, key).is_some());
        assert!(take_pending(&mut world, key).is_none());
    }
}
