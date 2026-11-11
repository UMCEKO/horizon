use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// Whether block `block`'s block entity ticks (`EntityBlock.getTicker`):
    /// set `ticks` when it is yours. Asked for each of the block's states
    /// when a world opens.
    /// Set `ticks` to true to make it tick, and never clear a value an earlier mod or mixin set.
    pub(in crate::game) fn native_block_entity_ticks(&self, block: &str, ticks: &mut bool) {
        self.java_hook_block_ticks(block, ticks);
        self.fire_block_entity_ticks(block, ticks);
    }
}
