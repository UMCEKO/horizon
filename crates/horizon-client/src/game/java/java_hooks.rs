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

#[cfg(feature = "java-mods")]
pub(super) type LockstepTick = super::java_lockstep::LockstepTick;
/// No Java client, no lockstep tick.
#[cfg(not(feature = "java-mods"))]
pub(super) enum LockstepTick {}

/// A mod item's name (`getHoverName`) and tooltip lines (`getTooltipLines`) as its code computes them.
pub(super) struct JavaItemText {
    pub name: horizon_client::text::Text,
    pub lines: Vec<horizon_client::text::Text>,
}

/// Who trampled farmland, as `BlockEvent.FarmlandTrampleEvent` is posted for them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TrampleBy {
    /// The acting player.
    Player,
    /// The mob with this entity id.
    Mob(u32),
}

impl Game<'_> {
    /// `BlockEvent.FarmlandTrampleEvent` after the native checks passed; true when a Java mod canceled the trample.
    pub(super) fn java_hook_trample(
        &mut self,
        pos: [i32; 3],
        distance: f64,
        who: TrampleBy,
    ) -> bool {
        #[cfg(feature = "java-mods")]
        return self.java_trample(pos, distance, who);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (pos, distance, who);
            false
        }
    }

    /// What the Java mods' F3 entries added to the displayer in the Java client's last tick.
    pub(super) fn java_hook_debug_recorded(&self) -> Vec<horizon_client::debug_overlay::Recorded> {
        #[cfg(feature = "java-mods")]
        return super::java_debug::recorded();
        #[cfg(not(feature = "java-mods"))]
        Vec::new()
    }

    /// Each placed guest's tree gets the Java server's roots as that player may use them (`send`: its Java client gets its permission packet and tree too).
    pub(super) fn java_hook_player_commands(&mut self, only: Option<u32>, send: bool) {
        #[cfg(feature = "java-mods")]
        self.java_player_commands(only, send);
        let _ = (only, send);
    }

    /// A loaded mod patches the attribute system: the window's sky comes from Java's probe.
    pub(super) fn client_env_routed(&self) -> bool {
        #[cfg(feature = "java-mods")]
        return super::java_routing::client_env_routed();
        #[cfg(not(feature = "java-mods"))]
        false
    }

    /// The Java client's parser for the chat box's lines it owns; none without a Java client.
    pub(super) fn java_hook_foreign_commands(
        &self,
    ) -> Option<&'static dyn horizon_client::chat::ForeignCommands> {
        #[cfg(feature = "java-mods")]
        return super::java_chat::foreign_commands();
        #[cfg(not(feature = "java-mods"))]
        None
    }

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

    /// The camera `camera_view` aligned: its eye, the entity's view rotation and the camera type's ordinal, for the Java client's view events.
    pub(super) fn java_hook_view_camera(
        &self,
        partial: f32,
        eye: [f64; 3],
        rotation: [f32; 2],
        mode: u8,
    ) {
        #[cfg(feature = "java-mods")]
        super::java_view::record_camera(partial, eye, rotation[0], rotation[1], mode);
        let _ = (partial, eye, rotation, mode);
    }

    /// The yaw and pitch the mods' camera angle listeners left for `Camera.alignWithEntity`.
    pub(super) fn java_hook_view_angles(&self) -> Option<[f32; 2]> {
        #[cfg(feature = "java-mods")]
        return super::java_view::angles().map(|[yaw, pitch, _]| [yaw, pitch]);
        #[cfg(not(feature = "java-mods"))]
        None
    }

    /// The roll the mods' camera angle listeners left, as the pose turns the view about its forward axis; the front view turns it the other way.
    pub(super) fn java_hook_view_roll(&self, mirrored: bool) -> f32 {
        #[cfg(feature = "java-mods")]
        return super::java_view::angles().map_or(0.0, |[_, _, roll]| {
            if mirrored { -roll } else { roll }
        });
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = mirrored;
            0.0
        }
    }

    /// The first-person hand's field of view, as the mods' listeners leave it.
    pub(super) fn java_hook_view_hud_fov(&self, fov: f32) -> f32 {
        #[cfg(feature = "java-mods")]
        return super::java_view::hud_fov(fov);
        #[cfg(not(feature = "java-mods"))]
        fov
    }

    /// What this frame's mouse turn reads: the options, and whether the pointer is grabbed.
    pub(super) fn java_hook_view_look(&self, sensitivity: f64, smooth: bool, grabbed: bool) {
        #[cfg(feature = "java-mods")]
        super::java_view::record_look(sensitivity, smooth, grabbed);
        let _ = (sensitivity, smooth, grabbed);
    }

    /// The fog `setup_fog` computed, as the mods' fog event listeners leave it; untouched while none listens.
    pub(super) fn java_hook_view_fog(
        &self,
        fog_type: horizon_client::fog::FogType,
        environment: horizon_client::fog::FogEnvironmentKind,
        render_distance_chunks: i32,
        darken: f32,
        fog: horizon_client::fog::FogData,
    ) -> horizon_client::fog::FogData {
        #[cfg(feature = "java-mods")]
        {
            use mod_api::HookId;
            if super::java_listener::listens(HookId::ComputeFogColor)
                || super::java_listener::listens(HookId::RenderFog)
            {
                super::java_view::hook_fog(super::java_view::FogIn {
                    fog_type,
                    environment,
                    render_distance_chunks,
                    darken,
                    fog,
                })
            } else {
                super::java_view::forget_fog();
                fog
            }
        }
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (fog_type, environment, render_distance_chunks, darken);
            fog
        }
    }

    /// A world's end, failed open or close: the Java server stops.
    pub(super) fn java_hook_world(&mut self, phase: &WorldPhase) {
        #[cfg(feature = "java-mods")]
        self.java_world_phase(false);
        let _ = phase;
    }

    /// `ServerAboutToStartEvent`, before native builds the world's levels: the Java mods' worldgen export, parsed.
    pub(super) fn java_hook_world_opening(
        &mut self,
        opening: &super::world_loading::WorldOpening,
    ) -> super::world_loading::JavaWorldStart {
        #[cfg(feature = "java-mods")]
        return self.java_world_opening(opening);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = opening;
            Ok(None)
        }
    }

    /// Whether a Java mod listens to `BlockEvent.EntityPlaceEvent`: a player's placement then runs under NeoForge's capture.
    pub(super) fn java_hook_place_listened(&self) -> bool {
        #[cfg(feature = "java-mods")]
        return self.java_place_listened();
        #[cfg(not(feature = "java-mods"))]
        false
    }

    /// `EntityPlaceEvent` (one snapshot) or `EntityMultiPlaceEvent` over a captured placement: whether it was cancelled.
    pub(super) fn java_hook_place(
        &mut self,
        face: horizon_core::geometry::Direction,
        snapshots: &[horizon_world::level::Snapshot],
    ) -> bool {
        #[cfg(feature = "java-mods")]
        return self.java_post_place(face, snapshots);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (face, snapshots);
            false
        }
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

    /// Whether a Java mod listens to `PistonEvent.Pre` or `PistonEvent.Post`.
    pub(super) fn java_hook_piston_listened(&self) -> bool {
        #[cfg(feature = "java-mods")]
        return super::java_listener::listens(mod_api::HookId::PistonPre)
            || super::java_listener::listens(mod_api::HookId::PistonPost);
        #[cfg(not(feature = "java-mods"))]
        false
    }

    /// `PistonEvent.Pre` (`post` false; true when a mod canceled it) or `PistonEvent.Post` for the piston at `pos`.
    pub(super) fn java_hook_piston(
        &mut self,
        pos: BlockPos,
        direction: horizon_core::geometry::Direction,
        extend: bool,
        post: bool,
    ) -> bool {
        #[cfg(feature = "java-mods")]
        {
            let (hook, event) = if post {
                (mod_api::HookId::PistonPost, "PistonEvent.Post")
            } else {
                (mod_api::HookId::PistonPre, "PistonEvent.Pre")
            };
            if !super::java_listener::listens(hook) {
                return false;
            }
            let posted = self.java_post_level_event(event, false, |out| {
                out.long(pos.pack());
                out.0.push(direction as u8);
                out.bool(extend);
            });
            !post && posted.is_some_and(|(canceled, _)| canceled)
        }
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (pos, direction, extend, post);
            false
        }
    }

    pub(super) fn java_hook_block_ticks(&self, block: &str, ticks: &mut bool) {
        #[cfg(feature = "java-mods")]
        if let Some(host) = horizon_java_host::get() {
            super::java_mods::register_level(host, super::Dim::Overworld);
            *ticks |= host.request("block_ticks", block.as_bytes()).is_some();
        }
        let _ = (block, ticks);
    }

    pub(super) fn java_hook_block_entity_tick(&mut self, pos: [i32; 3]) {
        #[cfg(feature = "java-mods")]
        self.java_block_entity_tick(pos);
        let _ = pos;
    }

    /// The ticks of the mod block entities from `slots[0]` on, as one Java run: how many slots it ticked, 0 to tick the first alone.
    pub(super) fn java_hook_block_entity_run(
        &mut self,
        slots: &[(horizon_client::block_entities::Pos, u32)],
        ticking: &dyn Fn(i32, i32) -> bool,
    ) -> usize {
        #[cfg(feature = "java-mods")]
        return self.java_block_entity_run(slots, ticking);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (slots, ticking);
            0
        }
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

    /// The window's tick starts; in lockstep the guard settles the Java tick's parks on every exit.
    pub(super) fn java_hook_tick_begin(&mut self) -> Option<LockstepTick> {
        #[cfg(feature = "java-mods")]
        return self.java_client_tick_begin();
        #[cfg(not(feature = "java-mods"))]
        None
    }

    /// `handleKeybinds`' moment: in lockstep the Java tick waits there while the window serves its clicks.
    pub(super) fn java_hook_keys_begin(&mut self) {
        #[cfg(feature = "java-mods")]
        self.java_lockstep_keys_begin();
    }

    pub(super) fn java_hook_keys_end(&mut self) {
        #[cfg(feature = "java-mods")]
        self.java_lockstep_keys_end();
    }

    /// `LocalPlayer.aiStep`'s moment: in lockstep the Java tick waits there for the window's player step; its record.
    pub(super) fn java_hook_player_begin(&mut self) -> Option<Vec<u8>> {
        #[cfg(feature = "java-mods")]
        return self.java_lockstep_player_begin();
        #[cfg(not(feature = "java-mods"))]
        None
    }

    /// Mods' writes to the local player in the park's record, applied before the step; where vanilla took `xo/yo/zo`, when the record marked it.
    pub(super) fn java_hook_motion_writes(&mut self, record: Option<&[u8]>) -> Option<horizon_core::geometry::Vec3> {
        #[cfg(feature = "java-mods")]
        return self.java_lockstep_motion_writes(record?);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = record;
            None
        }
    }

    /// What `tick_fov` computed: the Java camera tick's event reads it on the next lockstep tick.
    pub(super) fn java_hook_fov_base(&self, base: f32, scoping: bool, scale: f32) {
        #[cfg(feature = "java-mods")]
        super::java_screens::CLIENT.with_borrow_mut(|c| c.fov_base = Some((base, scoping, scale)));
        let _ = (base, scoping, scale);
    }

    /// The modifier the Java camera tick's event left for this tick; none outside lockstep.
    pub(super) fn java_hook_fov_target(&self) -> Option<f32> {
        #[cfg(feature = "java-mods")]
        return super::java_lockstep::take_fov_target();
        #[cfg(not(feature = "java-mods"))]
        None
    }

    /// `MovementInputUpdateEvent` on the keys native read; Java's listeners run before native mods'.
    pub(super) fn java_hook_movement_input(&mut self, keys: &mut horizon_client::sprint::Keys, strafe: &mut f32, forward: &mut f32) {
        #[cfg(feature = "java-mods")]
        self.java_lockstep_movement_input(keys, strafe, forward);
        let _ = (keys, strafe, forward);
    }

    /// The window's player step ran, with the tick's `Input` bits: the Java tick goes on from `aiStep` with its result.
    pub(super) fn java_hook_player_end(&mut self, moved: bool, keys: u8) {
        #[cfg(feature = "java-mods")]
        self.java_lockstep_player_end(moved, keys);
        let _ = (moved, keys);
    }

    #[cfg(feature = "java-mods")]
    pub(super) fn is_java_block_entity(&self, pos: [i32; 3]) -> bool {
        matches!(self.world.block_entities.get(&(pos[0], pos[1], pos[2])), Some(horizon_client::containers::BlockEntity::Mod(m)) if !m.id().is_empty())
    }

    /// `ITEMS` of a Java mod's block entity at `at`: the handler its code gives for that side, if any.
    pub(super) fn java_hook_item_part(
        &mut self,
        at: mod_api::BlockSide,
    ) -> Option<mod_api::Capability<dyn mod_api::ResourceHandler<mod_api::ItemResource>>> {
        #[cfg(feature = "java-mods")]
        return self.java_item_part(at);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = at;
            None
        }
    }

    /// A transaction a Java handler wrote in closed at `depth` (1: the root): Java's closes the same way.
    pub(super) fn java_hook_transaction_closed(&mut self, depth: usize, commit: bool) {
        #[cfg(feature = "java-mods")]
        self.java_items_closed(depth, commit);
        let _ = (depth, commit);
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

    /// The tool of a loot roll with the enchantment levels Java gives it; None leaves the tool as it is.
    pub(super) fn java_hook_tool_for_loot(&mut self, tool: Option<&ItemStack>) -> Option<ItemStack> {
        #[cfg(feature = "java-mods")]
        return self.java_tool_for_loot(tool?);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = tool;
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

    /// Hands a block's destroy (0) or hit (1) particles to the Java client when its mod makes them; true when it took them.
    pub(super) fn java_hook_block_effect(
        &mut self,
        kind: u8,
        pos: [i32; 3],
        state: BlockStateId,
        face: [i32; 3],
    ) -> bool {
        #[cfg(feature = "java-mods")]
        return self.java_block_effect(kind, pos, state, face);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (kind, pos, state, face);
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

    /// Whether a Java mod registered this particle type, which `/particle` then runs in Java.
    pub(super) fn java_hook_particle_owned(&self, name: &str) -> bool {
        #[cfg(feature = "java-mods")]
        return super::java_particles::java_particle_owned(name);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = name;
            false
        }
    }

    /// `/particle` of a Java mod's type: vanilla's codec and sends in Java; the players reached, or the codec's error.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn java_hook_server_particles(
        &mut self,
        dim: &str,
        spec: &str,
        pos: [f64; 3],
        delta: [f32; 3],
        speed: f32,
        count: i32,
        force: bool,
        viewers: &[u128],
    ) -> Result<i32, String> {
        #[cfg(feature = "java-mods")]
        return self.java_server_particles(dim, spec, pos, delta, speed, count, force, viewers);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (dim, spec, pos, delta, speed, count, force, viewers);
            Err("no Java".into())
        }
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

    /// Native opened a menu for the acting guest `conn`: its Java view gets the real vanilla menu while a menu route is on.
    pub(super) fn java_hook_menu_opened(
        &mut self,
        conn: u32,
        window: i32,
        menu: &str,
        pos: Option<[i32; 3]>,
    ) {
        #[cfg(feature = "java-mods")]
        self.java_menu_open(conn, window, menu, pos);
        let _ = (conn, window, menu, pos);
    }

    /// Native closed the acting guest's menu `window`.
    pub(super) fn java_hook_menu_closed(&mut self, window: i32) {
        #[cfg(feature = "java-mods")]
        self.java_menu_close(window);
        let _ = window;
    }

    /// `PlayerRespawnEvent` for the guest in place; `end_conquered` is the return from the End credits, which no native hook reports.
    pub(super) fn java_hook_respawned(&mut self, end_conquered: bool) {
        #[cfg(feature = "java-mods")]
        super::java_listen_respawn::post_respawned(self, end_conquered);
        let _ = end_conquered;
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

    /// A Java mod's command, or `CommandEvent` for a native one: `Some(ok)` when Java ran or stopped it, `Err` a replaced line.
    pub(super) fn java_hook_command(
        &mut self,
        command: &str,
        quiet: bool,
    ) -> Result<Option<bool>, String> {
        #[cfg(feature = "java-mods")]
        return match self.java_command(command, quiet) {
            None => Ok(None),
            Some(super::java_commands::JavaVerdict::Ran) => Ok(Some(true)),
            Some(super::java_commands::JavaVerdict::Stopped) => Ok(Some(false)),
            Some(super::java_commands::JavaVerdict::Replaced(text)) => Err(text),
        };
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (command, quiet);
            Ok(None)
        }
    }

    /// The line a `CommandEvent` listener put in place of the player's command `player_command` just heard.
    pub(super) fn java_hook_replaced_command() -> Option<String> {
        #[cfg(feature = "java-mods")]
        return super::java_commands::java_take_replaced_command();
        #[cfg(not(feature = "java-mods"))]
        None
    }

    /// Whether a Java mod owns `line`'s root command, which any player may send whatever the world allows.
    pub(super) fn java_hook_owns_command(line: &str) -> bool {
        #[cfg(feature = "java-mods")]
        return Self::java_owns_command(line);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = line;
            false
        }
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

/// Whether a Java mod modifies the loot of a top-level roll of `table` (NeoForge's `modifyLoot`).
pub(crate) fn java_hook_loot_wanted(table: &str) -> bool {
    #[cfg(feature = "java-mods")]
    return super::java_loot_modify::loot_wanted(table);
    #[cfg(not(feature = "java-mods"))]
    {
        let _ = table;
        false
    }
}

/// NeoForge's `CommonHooks.modifyLoot` for a roll [`java_hook_loot_wanted`] took.
pub(crate) fn java_hook_loot_rolled(
    table: &str,
    ctx: &horizon_world::loot::LootContext<'_>,
    tool: Option<&horizon_world::loot::LootStack>,
    random: &mut Option<horizon_core::random::RandomState>,
    items: &mut Vec<horizon_world::loot::LootStack>,
) {
    #[cfg(feature = "java-mods")]
    super::java_loot_modify::loot_rolled(table, ctx, tool, random, items);
    let _ = (table, ctx, tool, random, items);
}

/// Whether a world's start waits for the Java mods' `ServerAboutToStart` and worldgen export before it builds a level.
pub(super) fn java_hook_wants_world_opening() -> bool {
    #[cfg(feature = "java-mods")]
    return horizon_java_host::configured();
    #[cfg(not(feature = "java-mods"))]
    false
}

/// A loot function of a kind a Java mod owns runs in Java; any other kind is left alone.
pub(crate) fn java_hook_loot_function(
    kind: &str,
    data: &serde_json::Value,
    ctx: &horizon_world::loot::LootContext<'_>,
    stack: &mut horizon_world::loot::LootStack,
) {
    #[cfg(feature = "java-mods")]
    super::java_loot::hook_function(kind, data, ctx, stack);
    let _ = (kind, data, ctx, stack);
}

/// A loot condition of a kind a Java mod owns is decided in Java; any other kind is left alone.
pub(crate) fn java_hook_loot_condition(
    kind: &str,
    data: &serde_json::Value,
    ctx: &horizon_world::loot::LootContext<'_>,
    holds: &mut bool,
) {
    #[cfg(feature = "java-mods")]
    super::java_loot::hook_condition(kind, data, ctx, holds);
    let _ = (kind, data, ctx, holds);
}

/// Opening a world waits on the Java mods' start: Err when they failed to start.
pub(super) fn java_hook_ready() -> Result<(), &'static str> {
    #[cfg(feature = "java-mods")]
    return horizon_java_host::ready();
    #[cfg(not(feature = "java-mods"))]
    Ok(())
}

/// Decides the Java run folder once, before the listener families are read; later calls change nothing.
#[cfg(feature = "java-mods")]
pub(crate) fn java_hook_resolve(java_mods: Option<&Path>) -> std::io::Result<()> {
    if !horizon_java_host::run_decided() {
        let run = horizon_java_host::resolve_run(
            java_mods,
            std::env::var_os("HORIZON_JAVA_RUN").as_deref(),
            &std::env::current_dir()?,
        )
        .filter(|run| {
            let has = horizon_java_host::has_mods(run);
            if !has {
                log::info!(
                    "Horizon: no Java mods in {}: starting without Java",
                    run.join("mods").display()
                );
            }
            has
        });
        horizon_java_host::set_run(run);
    }
    Ok(())
}

/// The game's start: the JVM boots with the bridge table, and the protocol learns the mods' components; `window` when this process draws.
pub(super) fn java_hook_start(java_mods: Option<&Path>, window: bool) -> std::io::Result<()> {
    #[cfg(feature = "java-mods")]
    {
        java_hook_resolve(java_mods)?;
        if horizon_java_host::configured() {
            super::java_content::start(super::java_bridges::bridges(), window);
            super::java_components::install();
            super::java_listener::take_census();
        }
    }
    #[cfg(not(feature = "java-mods"))]
    let _ = (java_mods, window);
    Ok(())
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

/// The particle types Java mods registered, which join the game's particle key table.
pub(super) fn java_hook_owned_particles() -> Vec<String> {
    #[cfg(feature = "java-mods")]
    return super::java_particles::java_owned_particles();
    #[cfg(not(feature = "java-mods"))]
    Vec::new()
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

/// Whether Java has block events of this level to run.
pub(super) fn java_has_block_events() -> bool {
    #[cfg(feature = "java-mods")]
    return super::java_mods::has_block_events();
    #[cfg(not(feature = "java-mods"))]
    false
}

/// Whether a Java mod's code runs this fluid: the block that holds its source is a Java one.
pub(super) fn java_owns_fluid(
    behaviours: &horizon_world::level::Behaviours,
    fluid: horizon_world::fluid::ModFluid,
) -> bool {
    #[cfg(feature = "java-mods")]
    return super::java_mods::owns_fluid(behaviours, fluid);
    #[cfg(not(feature = "java-mods"))]
    {
        let _ = (behaviours, fluid);
        false
    }
}

/// `LevelAccess::allow_fluid_tick` of a Java mod's fluid: its `FluidState.tick` runs in Java instead.
pub(super) fn java_note_fluid_tick(pos: BlockPos, state: BlockStateId) {
    #[cfg(feature = "java-mods")]
    super::java_mods::note_fluid_tick(pos, state);
    let _ = (pos, state);
}

/// What a Java mod's fluid answers in this dimension for `FlowingFluid`'s getters, once Java was asked.
pub(super) fn java_fluid_rules(
    dimension: horizon_core::dimension::Dimension,
    fluid: horizon_world::fluid::ModFluid,
) -> Option<horizon_world::fluid::ModFluidRules> {
    #[cfg(feature = "java-mods")]
    return super::java_mods::fluid_rules(dimension, fluid);
    #[cfg(not(feature = "java-mods"))]
    {
        let _ = (dimension, fluid);
        None
    }
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

/// `LivingJumpEvent` for the window's player, after vanilla's jump set the velocity and before native mods hear it.
pub(crate) fn java_living_jump(entity: u32, kind: &str, player: &mut horizon_client::motion::PlayerMotion) {
    #[cfg(feature = "java-mods")]
    if kind == "minecraft:player" {
        super::java_lockstep::living_jump(player);
    } else {
        java_living_jump_server(entity, kind, player);
    }
    let _ = (entity, kind, player);
}

/// The server copy's jump (W9 SPEC-4-6).
#[cfg(feature = "java-mods")]
fn java_living_jump_server(_entity: u32, _kind: &str, _player: &mut horizon_client::motion::PlayerMotion) {}

/// `LivingFallEvent` for the window's player before native mods hear it; any other entity is the server's (W9 SPEC-4-6).
pub(crate) fn java_living_fall(kind: &str, distance: &mut f64, multiplier: &mut f32, cancel: &mut bool) {
    #[cfg(feature = "java-mods")]
    if kind == "minecraft:player" {
        super::java_lockstep::living_fall(false, distance, multiplier, cancel);
    }
    let _ = (kind, distance, multiplier, cancel);
}

/// `PlayerFlyableFallEvent` for the window's player, where `Player.causeFallDamage` returns early for `mayfly`.
pub(crate) fn java_player_flyable_fall(distance: f64, multiplier: f32) {
    #[cfg(feature = "java-mods")]
    super::java_lockstep::living_fall(true, &mut { distance }, &mut { multiplier }, &mut false);
    let _ = (distance, multiplier);
}
