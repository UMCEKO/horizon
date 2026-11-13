//! `ClientLevel.syncBlockState`'s body: what a server's acknowledgement does to a block the client predicted.

use super::*;

impl Game<'_> {
    /// The block takes the server's state; one that comes back may now hold the player, who goes to `player_pos`, where the prediction began.
    pub(in crate::game) fn sync_block_state_body(
        &mut self,
        pos: [i32; 3],
        state: i32,
        player_pos: Option<[f64; 3]>,
    ) {
        if self.world.block(pos[0], pos[1], pos[2]).0 as i32 == state {
            return;
        }
        self.set_block(pos, state);
        if let Some(at) = player_pos
            && self.block_meets_player(pos, state)
        {
            self.snap_player_to(at);
        }
    }
}
