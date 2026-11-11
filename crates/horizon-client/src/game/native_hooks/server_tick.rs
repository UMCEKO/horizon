use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// The authoritative server tick's phases: `ServerTickStart` before any
    /// level is ticked (NeoForge's `ServerTickEvent.Pre`), `ServerTickEnd`
    /// after the last (`.Post`). Read blocks with `native_server_block`, by
    /// dimension: which level `self.world` holds here is not defined. A paused
    /// pass fires nothing; a frozen world carries `world_running` false except
    /// during a requested step.
    pub(in crate::game) fn native_server_tick(
        &mut self,
        context: &LifecycleContext,
        phase: ServerTickPhase,
    ) {
        self.java_hook_server_tick(&phase);
        self.fire_server_tick(context, phase);
    }
}
