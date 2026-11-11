use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// A server tick of the block entity at `pos`, in vanilla's order among
    /// the others (`Level.tickBlockEntities`), when its `id` is yours: read
    /// and change its saved compound `nbt`, its item slots `items` (keep
    /// their count: as many as its container has, none without one) and the
    /// data slots its menu shows (`data`). It is out of the world during the
    /// call (the block entity at `pos` reads as a `Mod` with an empty id, and
    /// a save leaves it out); if the call removes or replaces it, `entity` is
    /// dropped. Clients, the window's own in single player included, get
    /// what `native_block_entity_update_tag` gives when its chunk is sent or
    /// its block changes.
    pub(in crate::game) fn native_block_entity_tick(
        &mut self,
        pos: [i32; 3],
        entity: &mut ModBlockEntity,
    ) {
        self.java_hook_block_entity_tick(pos);
        self.fire_block_entity_tick(pos, entity);
    }
}
