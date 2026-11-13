// game-module-cfg: feature java-mods
//! W12 SPEC-22: the window's input and frame methods mods patch, run as the Java client's merged methods (`Recv::Client`).

use super::java_routing::{NullableVec3, Recv, StateArg, route};
use super::*;

impl Game<'_> {
    /// `KeyboardHandler.handleDebugKeys` for a key name, while a debug modifier is down.
    pub(in crate::game) fn client_debug_keys(&mut self, key: &str) -> bool {
        route!(
            self,
            "net/minecraft/client/KeyboardHandler#handleDebugKeys(Lnet/minecraft/client/input/KeyEvent;)Z",
            Recv::Client,
            (self.java_key_event_arg(key))
        );
        self.debug_keys_body(key)
    }

    /// `Minecraft.continueAttack`: the held attack button's step of the client tick.
    pub(in crate::game) fn client_continue_attack(&mut self, down: bool) {
        route!(self, "net/minecraft/client/Minecraft#continueAttack(Z)V", Recv::Client, (down));
        self.remote_continue_attack(down);
    }

    /// `ClientLevel.syncBlockState`: a predicted block takes the state the server acknowledged.
    pub(in crate::game) fn client_sync_block_state(
        &mut self,
        pos: [i32; 3],
        state: i32,
        player_pos: Option<[f64; 3]>,
    ) {
        route!(
            self,
            "net/minecraft/client/multiplayer/ClientLevel#syncBlockState(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/phys/Vec3;)V",
            Recv::Client,
            (
                BlockPos::new(pos[0], pos[1], pos[2]),
                StateArg(BlockStateId(state.max(0) as u32)),
                NullableVec3(player_pos)
            )
        );
        self.sync_block_state_body(pos, state, player_pos);
    }
}
