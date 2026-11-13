// game-module-cfg: feature java-mods
//! SPEC-22: the window's own bodies of the Java client's routed methods, and what those methods do to the window's level and player.

use super::*;
use horizon_java_host::bridge::KeyName;

horizon_java_host::java_bridges! { table for Game<'static> {
    route "net/minecraft/client/KeyboardHandler".handleDebugKeys
    "(Lnet/minecraft/client/input/KeyEvent;)Z" if "@dev/horizonlab/javamods/boot/ClientRoutes.window" =>
    fn route_keyboard_handle_debug_keys(game, key: KeyName) -> bool {
        game.debug_keys_body(&key.0)
    }

    route "net/minecraft/client/Minecraft".continueAttack
    "(Z)V" if "@dev/horizonlab/javamods/boot/ClientRoutes.window" =>
    fn route_minecraft_continue_attack(game, down: bool) -> () {
        game.remote_continue_attack(down);
    }

    "dev/horizonlab/javamods/boot/ClientWorld".setBlock
    "(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/block/state/BlockState;)V" =>
    fn client_world_set_block(game, pos: Pos, state: State) -> () {
        // A state the window has no block for reads as air, as a server's unknown states do.
        let state = state.0.map_or(0, |s| s.0 as i32);
        game.set_block([pos.0.x, pos.0.y, pos.0.z], state);
    }

    "dev/horizonlab/javamods/boot/ClientWorld".snapPlayer
    "(DDD)V" =>
    fn client_world_snap_player(game, x: f64, y: f64, z: f64) -> () {
        game.snap_player_to([x, y, z]);
    }
}}
