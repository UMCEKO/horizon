use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// A player breaks the block `state` at `pos`, once the game allowed it
    /// (the held item can break blocks, the debug stick didn't take the
    /// click): where `ServerPlayerGameMode.destroyBlock` starts. Set
    /// `canceled` to keep the block: nothing is destroyed or dropped, and a
    /// guest sees it back. Server-side, for every player of the world (the
    /// helpers read the acting one).
    pub(in crate::game) fn native_break_block(
        &mut self,
        pos: [i32; 3],
        state: BlockStateId,
        canceled: &mut bool,
        exp_to_drop: &mut Option<u32>,
    ) {
        self.fire_break_block(pos, state, canceled, exp_to_drop);
    }
}
