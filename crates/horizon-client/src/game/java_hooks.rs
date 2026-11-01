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
