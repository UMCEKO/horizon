//! Where the hook points reach Java mods (`docs/java-mods.md`); each does nothing without `--features java-mods`.
use super::Game;
use super::native_lifecycle::{ServerTickPhase, WorldPhase};
use horizon_client::containers::ModBlockEntity;
use horizon_client::items::ItemStack;

/// A mod item's name (`getHoverName`) and tooltip lines (`getTooltipLines`) as its code computes them.
pub(super) struct JavaItemText {
    pub name: horizon_client::text::Text,
    pub lines: Vec<horizon_client::text::Text>,
}

impl Game<'_> {
    /// The Java client's text for a mod item's stack; None for any other item and until it has answered.
    pub(super) fn java_hook_item_text(
        &self,
        stack: &ItemStack,
        creative: bool,
    ) -> Option<std::sync::Arc<JavaItemText>> {
        #[cfg(feature = "java-mods")]
        return self.java_item_text(stack, creative);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (stack, creative);
            None
        }
    }

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

    /// The progress of a break from Java's `getDestroyProgress`; None leaves it to Horizon.
    pub(super) fn java_hook_destroy_progress(&mut self, pos: [i32; 3]) -> Option<f32> {
        #[cfg(feature = "java-mods")]
        return self.java_destroy_progress(pos);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = pos;
            None
        }
    }

    /// Whether the held item harvests the block, from Java's `hasCorrectToolForDrops`; None leaves it to Horizon.
    pub(super) fn java_hook_correct_tool(&mut self, pos: [i32; 3]) -> Option<bool> {
        #[cfg(feature = "java-mods")]
        return self.java_correct_tool(pos);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = pos;
            None
        }
    }

    /// Java's `destroyBlock` sequence for a mod's block; false leaves the break to Horizon.
    pub(super) fn java_hook_destroy_block(
        &mut self,
        pos: [i32; 3],
        state: horizon_world::registry::BlockStateId,
    ) -> bool {
        #[cfg(feature = "java-mods")]
        return self.java_destroy_block(pos, state);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (pos, state);
            false
        }
    }

    /// The held item's own `mineBlock`; false leaves the wear to Horizon.
    pub(super) fn java_hook_mine_block(
        &mut self,
        pos: [i32; 3],
        state: horizon_world::registry::BlockStateId,
    ) -> bool {
        #[cfg(feature = "java-mods")]
        return self.java_mine_block(pos, state);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (pos, state);
            false
        }
    }

    /// A held use's tick for an item whose use is its mod's code; false leaves it to Horizon.
    pub(super) fn java_hook_tick_using(
        &mut self,
        slot: usize,
        ticks: u32,
        stack: &ItemStack,
    ) -> bool {
        #[cfg(feature = "java-mods")]
        return self.java_tick_using(slot, ticks, stack);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (slot, ticks, stack);
            false
        }
    }

    /// The end of such a held use; false leaves it to Horizon.
    pub(super) fn java_hook_release_using(
        &mut self,
        stack: &ItemStack,
        ticks: u32,
        hand_changed: bool,
    ) -> bool {
        #[cfg(feature = "java-mods")]
        return self.java_release_using(stack, ticks, hand_changed);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (stack, ticks, hand_changed);
            false
        }
    }

    /// An item's own `interactLivingEntity`, once the mob aimed at passed.
    pub(super) fn java_hook_interact_living(&mut self) -> bool {
        #[cfg(feature = "java-mods")]
        return self.java_interact_living();
        #[cfg(not(feature = "java-mods"))]
        false
    }

    pub(super) fn java_hook_attack(&mut self, entity: i32, canceled: &mut bool) {
        #[cfg(feature = "java-mods")]
        self.java_attack_event(entity, canceled);
        let _ = (entity, canceled);
    }
}

/// What a server of this run tells a Java client joining from another process in configuration, when it has Java mods: the Java side's configuration frames.
pub(super) fn java_frames() -> Option<std::sync::Arc<dyn Fn() -> Vec<Vec<u8>> + Send + Sync>> {
    #[cfg(feature = "java-mods")]
    return horizon_java_host::configured().then(|| {
        std::sync::Arc::new(|| {
            let mut frames = Vec::new();
            if let Some(reply) =
                horizon_java_host::get().and_then(|host| host.request("remote_config", &[]))
            {
                let mut reply = horizon_java_host::wire::In(&reply);
                for _ in 0..reply.int().unwrap_or(0) {
                    match reply.bytes() {
                        Some(frame) => frames.push(frame.to_vec()),
                        None => break,
                    }
                }
            }
            frames
        }) as std::sync::Arc<dyn Fn() -> Vec<Vec<u8>> + Send + Sync>
    });
    #[cfg(not(feature = "java-mods"))]
    None
}
