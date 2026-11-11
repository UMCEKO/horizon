//! Where the hook points reach Java mods (`docs/java-mods.md`); each does nothing without `--features java-mods`.
use super::Game;
use super::native_lifecycle::{ServerTickPhase, WorldPhase};
use horizon_client::chunk::Chunk;
use horizon_client::items::ItemStack;
use horizon_client::spans::Span;
use horizon_core::nbt::NamedTag;
use horizon_core::position::BlockPos;
use horizon_world::registry::BlockStateId;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A mod item's name (`getHoverName`) and tooltip lines (`getTooltipLines`) as its code computes them.
pub(super) struct JavaItemText {
    pub name: horizon_client::text::Text,
    pub lines: Vec<horizon_client::text::Text>,
}

impl Game<'_> {
    /// The slot bits of the toasts mods added to the Java client, shown in the window's column.
    pub(super) fn java_hook_toast_slots(&self) -> u32 {
        #[cfg(feature = "java-mods")]
        return self.java_toast_slots();
        #[cfg(not(feature = "java-mods"))]
        0
    }

    /// The mods' toasts take the slots native's kinds left free, then draw.
    pub(super) fn java_hook_draw_toasts(
        &mut self,
        batch: &mut horizon_client::hud::HudBatch,
        hidden: bool,
    ) {
        #[cfg(feature = "java-mods")]
        self.java_draw_toasts(batch, hidden);
        let _ = (batch, hidden);
    }

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

    /// A world's end, failed open or close: the Java server stops.
    pub(super) fn java_hook_world(&mut self, phase: &WorldPhase) {
        #[cfg(feature = "java-mods")]
        self.java_world_phase(false);
        let _ = phase;
    }

    /// `ServerStartedEvent`: the Java server starts with the world.
    pub(super) fn java_hook_server_started(&mut self) {
        #[cfg(feature = "java-mods")]
        self.java_world_phase(true);
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

    pub(super) fn java_hook_block_entity_tick(&mut self, pos: [i32; 3]) {
        #[cfg(feature = "java-mods")]
        self.java_block_entity_tick(pos);
        let _ = pos;
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
        tag: &horizon_core::nbt::Tag,
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
        matches!(self.world.block_entities.get(&(pos[0], pos[1], pos[2])), Some(horizon_client::containers::BlockEntity::Mod(m)) if !m.id().is_empty())
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

    /// An item's own `interactLivingEntity` on mob `entity`, once that mob's own action passed.
    pub(super) fn java_hook_interact_living(&mut self, entity: i32) -> bool {
        #[cfg(feature = "java-mods")]
        return self.java_interact_living(entity);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = entity;
            false
        }
    }

    /// Whether Java runs any part of `animateTick`: native then reports the positions it picks.
    pub(super) fn java_animates() -> bool {
        #[cfg(feature = "java-mods")]
        return horizon_java_host::bridge::java_animates();
        #[cfg(not(feature = "java-mods"))]
        false
    }

    /// Whether a mod patches the animate loop itself, so Java's own runs and native's does not.
    pub(super) fn java_animate_whole() -> bool {
        #[cfg(feature = "java-mods")]
        return horizon_java_host::bridge::java_animates_whole();
        #[cfg(not(feature = "java-mods"))]
        false
    }

    /// Which parts of a picked state's `animateTick` Java runs (bit 1 the block's, bit 2 the fluid's).
    pub(super) fn java_animate_route(state: horizon_world::registry::BlockStateId) -> u8 {
        #[cfg(feature = "java-mods")]
        return horizon_java_host::bridge::java_animate(state);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = state;
            0
        }
    }

    pub(super) fn java_hook_attack(&mut self, entity: i32, canceled: &mut bool) {
        #[cfg(feature = "java-mods")]
        self.java_attack_event(entity, canceled);
        let _ = (entity, canceled);
    }

    /// Java's queued block events of this level, before native answers its own.
    pub(super) fn java_hook_drain_block_events(&mut self) {
        #[cfg(feature = "java-mods")]
        self.java_drain_block_events();
    }

    /// `useItemOn` for a mod's block, `RightClickBlock` for a vanilla one: Some(true) took it, Some(false) canceled.
    pub(super) fn java_hook_click_block(&mut self, hit: &super::Hit) -> Option<bool> {
        #[cfg(feature = "java-mods")]
        return self.java_click_block(hit);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = hit;
            None
        }
    }

    /// A mod item's own `useOn`, when native left the click unclaimed.
    pub(super) fn java_hook_use_item_on(
        &mut self,
        stack: &ItemStack,
        hit: &super::Hit,
        claimed: &mut bool,
    ) {
        #[cfg(feature = "java-mods")]
        self.java_use_item_on(stack, hit, claimed);
        let _ = (stack, hit, claimed);
    }

    /// A mod item's own `use`, when native left the click unclaimed.
    pub(super) fn java_hook_use_item(&mut self, stack: &ItemStack, claimed: &mut bool) {
        #[cfg(feature = "java-mods")]
        self.java_use_item(stack, claimed);
        let _ = (stack, claimed);
    }

    /// `LevelTickEvent` before (`post` false) and after a level's tick.
    pub(super) fn java_hook_level_tick(&mut self, post: bool) {
        #[cfg(feature = "java-mods")]
        self.java_level_tick(post);
        let _ = post;
    }

    /// The level's Java block states bound to this game's, once.
    pub(super) fn java_hook_bind_states(&mut self) {
        #[cfg(feature = "java-mods")]
        self.java_bind_states();
    }

    /// `save_level`'s start: Java's block entities reach their native records.
    pub(super) fn java_hook_flush_block_entities(&mut self) {
        #[cfg(feature = "java-mods")]
        self.java_flush_block_entities();
    }

    /// Chunks Java changed since the last save join the modified ones.
    pub(super) fn java_hook_chunks_unsaved(&mut self) {
        #[cfg(feature = "java-mods")]
        self.java_chunks_unsaved();
    }

    /// `ChunkDataEvent.Save` on chunks about to be written: the records Java wrote in their place.
    pub(super) fn java_hook_chunks_saving(
        &mut self,
        chunks: &[Arc<Chunk>],
    ) -> Vec<((i32, i32), NamedTag)> {
        #[cfg(feature = "java-mods")]
        return self.java_chunks_saving(chunks);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = chunks;
            Vec::new()
        }
    }

    /// `LevelEvent.Save` once the level's chunks are written.
    pub(super) fn java_hook_level_saved(&mut self) {
        #[cfg(feature = "java-mods")]
        self.java_level_saved();
    }

    /// Columns that joined and left the level in this update.
    pub(super) fn java_hook_chunks_changed(
        &mut self,
        loaded: &[(i32, i32)],
        unloaded: &[Arc<Chunk>],
    ) {
        #[cfg(feature = "java-mods")]
        self.java_chunks_changed(loaded, unloaded);
        let _ = (loaded, unloaded);
    }

    /// `ServerStoppingEvent`, before the players leave.
    pub(super) fn java_hook_server_stopping(&mut self) {
        #[cfg(feature = "java-mods")]
        self.java_server_stopping();
    }

    /// An idle level closes: its columns unload from Java first.
    pub(super) fn java_hook_level_closing(&mut self) {
        #[cfg(feature = "java-mods")]
        self.java_level_closing();
    }

    /// The server closing: its players leave Java's server, then every level's columns unload from it.
    pub(super) fn java_hook_close_levels(&mut self) -> horizon_client::Result<()> {
        #[cfg(feature = "java-mods")]
        if horizon_java_host::get().is_some() {
            self.java_players_removed();
            self.java_level_closing();
            let back = self.dim;
            let dims = self.levels.parked_dims();
            for &dim in &dims {
                self.enter_level(dim)?;
                self.java_level_closing();
            }
            if !dims.is_empty() {
                self.enter_level(back)?;
            }
        }
        Ok(())
    }

    /// A guest's `ServerPlayer.tick` in Java, its state swapped in for the call while Java mods run.
    pub(super) fn java_hook_player_tick(&mut self, gi: usize, conn: u32) {
        #[cfg(feature = "java-mods")]
        if horizon_java_host::get().is_some() {
            let _ = self.as_guest_at(gi, conn, Self::java_player_tick);
        }
        let _ = (gi, conn);
    }

    /// A guest left the server: its Java player logs out.
    pub(super) fn java_hook_player_left(&mut self, uuid: u128) {
        #[cfg(feature = "java-mods")]
        self.java_player_left(uuid);
        let _ = uuid;
    }

    /// A guest with a Java client of its own logged in.
    pub(super) fn java_hook_remote_join(&mut self, uuid: u128, java_client: bool) {
        #[cfg(feature = "java-mods")]
        if java_client {
            self.java_remote_join(uuid);
        }
        let _ = (uuid, java_client);
    }

    /// A `horizon:java` frame from a guest's Java client.
    pub(super) fn java_hook_remote_in(&mut self, uuid: u128, java_client: bool, frame: &[u8]) {
        #[cfg(feature = "java-mods")]
        if java_client {
            self.java_remote_in(uuid, frame);
        }
        let _ = (uuid, java_client, frame);
    }

    /// The hub's tracking changes reach Java's `ChunkMap` while its Java side is on, then the frames Java wrote reach their clients.
    pub(super) fn java_hook_tracking(&mut self) {
        #[cfg(feature = "java-mods")]
        self.java_lan_tracking();
    }

    /// A server tick ended: its time joins the profile, when one is recorded.
    pub(super) fn java_hook_profile_tick(&mut self, wall_ms: f32, spans: &[Span]) {
        #[cfg(feature = "java-mods")]
        self.profile_tick(wall_ms, spans);
        let _ = (wall_ms, spans);
    }

    /// The JVM's flight recording written now, named by the time (`profile dump`).
    pub(super) fn java_hook_profile_dump(&mut self) -> Result<std::path::PathBuf, String> {
        #[cfg(feature = "java-mods")]
        return self.profile_dump();
        #[cfg(not(feature = "java-mods"))]
        Err("this build has no Java mods".into())
    }

    /// A stopping server writes the JVM's recording when one is kept, since the process exits without the JVM's own exit dump.
    pub(super) fn java_hook_profile_stop(&mut self) -> Option<Result<std::path::PathBuf, String>> {
        #[cfg(feature = "java-mods")]
        if super::profile::dir().is_some() {
            return Some(self.profile_dump());
        }
        None
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

/// Opening a world waits on the Java mods' start: Err when they failed to start.
pub(super) fn java_hook_ready() -> Result<(), &'static str> {
    #[cfg(feature = "java-mods")]
    return horizon_java_host::ready();
    #[cfg(not(feature = "java-mods"))]
    Ok(())
}

/// The game's start: the JVM boots with the bridge table, and the protocol learns the mods' components.
pub(super) fn java_hook_start() {
    #[cfg(feature = "java-mods")]
    {
        super::java_content::start(super::java_bridges::bridges());
        super::java_components::install();
    }
}

/// The loading screen's line while the Java mods boot.
pub(super) fn java_hook_loading_note() -> Option<&'static str> {
    #[cfg(feature = "java-mods")]
    return super::java_content::note();
    #[cfg(not(feature = "java-mods"))]
    None
}

/// The Java mods' carried content folders, first among the content mods.
pub(super) fn java_hook_carried_content(root: &Path) -> Vec<PathBuf> {
    #[cfg(feature = "java-mods")]
    return super::java_content::carried(root);
    #[cfg(not(feature = "java-mods"))]
    {
        let _ = root;
        Vec::new()
    }
}

/// Columns a Java mod's `getChunk` unloaded while it waited, for `update_world` to save and drop.
pub(super) fn java_hook_pumped_unloads() -> Vec<Arc<Chunk>> {
    #[cfg(feature = "java-mods")]
    return super::java_mods::take_pumped_unloads();
    #[cfg(not(feature = "java-mods"))]
    Vec::new()
}

/// A server tick starts: the JVM time before it is not the tick's.
pub(super) fn java_hook_profile_begin() {
    #[cfg(feature = "java-mods")]
    super::profile::begin_tick();
}

/// `LevelAccess::observe_block_tick`: a mod's block's scheduled or random tick.
pub(super) fn java_note_block_tick(pos: BlockPos, state: BlockStateId, random: bool) {
    #[cfg(feature = "java-mods")]
    super::java_mods::note_block_tick(pos, state, random);
    let _ = (pos, state, random);
}

/// `LevelAccess::observe_block_changed`: a mod's block, or one with a block entity, changed.
pub(super) fn java_note_block_changed(
    pos: BlockPos,
    old: BlockStateId,
    new: BlockStateId,
    flags: i32,
    block_changed: bool,
) {
    #[cfg(feature = "java-mods")]
    super::java_mods::note_block_changed(pos, old, new, flags, block_changed);
    let _ = (pos, old, new, flags, block_changed);
}

/// `LevelAccess::observe_neighbor_changed`: a mod's block heard a neighbour change.
pub(super) fn java_note_neighbor_changed(
    pos: BlockPos,
    state: BlockStateId,
    changed: BlockStateId,
    moved: bool,
) {
    #[cfg(feature = "java-mods")]
    super::java_mods::note_neighbor_changed(pos, state, changed, moved);
    let _ = (pos, state, changed, moved);
}

/// `LevelAccess::observe_shape_update`: a mod's block's `updateShape`.
pub(super) fn java_note_shape_update(
    pos: BlockPos,
    state: BlockStateId,
    direction: horizon_core::geometry::Direction,
    neighbour: BlockStateId,
) {
    #[cfg(feature = "java-mods")]
    super::java_mods::note_shape_update(pos, state, direction, neighbour);
    let _ = (pos, state, direction, neighbour);
}
