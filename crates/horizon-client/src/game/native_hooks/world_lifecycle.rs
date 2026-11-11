use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// The world's phases, read-only; `error_text` carries the engine's
    /// own open-failure text, empty otherwise. Server-side. A server loop or final
    /// save failure fires `WorldCloseFailed` with the error text.
    pub(in crate::game) fn native_world_lifecycle(
        &mut self,
        context: &LifecycleContext,
        phase: WorldPhase,
        error_text: &str,
    ) {
        self.java_hook_world(&phase);
        self.fire_world_lifecycle(context, phase, error_text);
    }
}
