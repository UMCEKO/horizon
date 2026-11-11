use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// After a successful authoritative server level tick pass.
    pub(in crate::game) fn native_level_tick_post(&mut self, dimension: &str) {
        self.java_hook_level_tick(true);
        self.fire_level_tick_post(dimension);
    }
}
