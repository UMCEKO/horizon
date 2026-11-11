use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// Before an authoritative server level's tick pass. `dimension` is its ID.
    pub(in crate::game) fn native_level_tick_pre(&mut self, dimension: &str) {
        self.java_hook_level_tick(false);
        self.fire_level_tick_pre(dimension);
    }
}
