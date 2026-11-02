//! Where the hook points reach Java mods (`docs/java-mods.md`); each does nothing without `--features java-mods`.
use super::Game;
use super::native_lifecycle::{ServerTickPhase, WorldPhase};
use horizon_client::containers::ModBlockEntity;

impl Game<'_> {
    pub(super) fn java_hook_world(&mut self, phase: &WorldPhase) {
        #[cfg(feature = "java-mods")]
        self.java_world_phase(matches!(phase, WorldPhase::WorldOpenReady));
        let _ = phase;
    }

    pub(super) fn java_hook_server_tick(&mut self, phase: &ServerTickPhase) {
        #[cfg(feature = "java-mods")]
        self.java_server_tick(!matches!(phase, ServerTickPhase::ServerTickStart { .. }));
        let _ = phase;
    }

    pub(super) fn java_hook_block_ticks(&self, block: &str, ticks: &mut bool) {
        #[cfg(feature = "java-mods")]
        if let Some(host) = horizon_java_host::get() {
            *ticks |= host.request("block_ticks", block.as_bytes()).is_some();
        }
        let _ = (block, ticks);
    }

    pub(super) fn java_hook_block_entity_tick(&mut self, pos: [i32; 3], entity: &ModBlockEntity) {
        #[cfg(feature = "java-mods")]
        self.java_block_entity_tick(pos, entity);
        let _ = (pos, entity);
    }

    /// Before a command reads a block entity: a mod's live form, from Java.
    pub(super) fn java_hook_block_entity_read(&mut self, pos: [i32; 3]) {
        #[cfg(feature = "java-mods")]
        if self.is_java_block_entity(pos) {
            self.java_refresh_block_entity(horizon_core::position::BlockPos::new(
                pos[0], pos[1], pos[2],
            ));
        }
        let _ = pos;
    }

    /// After a command wrote a block entity: a mod's reaches Java.
    pub(super) fn java_hook_block_entity_written(
        &mut self,
        pos: [i32; 3],
        tag: &horizon_world::nbt::Tag,
    ) {
        #[cfg(feature = "java-mods")]
        if self.is_java_block_entity(pos) {
            self.java_set_block_entity(
                horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]),
                tag.clone(),
            );
        }
        let _ = (pos, tag);
    }

    #[cfg(feature = "java-mods")]
    fn is_java_block_entity(&self, pos: [i32; 3]) -> bool {
        matches!(self.world.block_entities.get(&(pos[0], pos[1], pos[2])), Some(horizon_client::containers::BlockEntity::Mod(m)) if !m.id.is_empty())
    }

    /// A hopper at `hopper` facing a block with no container: `Some(moved)` when that block has a mod's item handler.
    pub(super) fn java_hook_hopper(
        &mut self,
        hopper: (i32, i32, i32),
        target: (i32, i32, i32),
        side: horizon_core::geometry::Direction,
        insert: bool,
    ) -> Option<bool> {
        #[cfg(feature = "java-mods")]
        return self.java_hopper(hopper, target, side, insert);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (hopper, target, side, insert);
            None
        }
    }

    pub(super) fn java_hook_break(&mut self, pos: [i32; 3], canceled: &mut bool) {
        #[cfg(feature = "java-mods")]
        self.java_break_event(pos, canceled);
        let _ = (pos, canceled);
    }

    pub(super) fn java_hook_attack(&mut self, entity: i32, canceled: &mut bool) {
        #[cfg(feature = "java-mods")]
        self.java_attack_event(entity, canceled);
        let _ = (entity, canceled);
    }
}
