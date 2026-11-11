// game-module-cfg: feature java-mods
//! Container methods that mods' mixins can patch, on the acting guest.

use super::java_routing::{ComponentArg, Recv, route};
use super::*;

impl Game<'_> {
    /// `BaseContainerBlockEntity.sendChestLockedNotifications` for the acting guest `conn`.
    pub(in crate::game) fn send_chest_locked_notifications(
        &mut self,
        conn: u32,
        at: [f64; 3],
        name: &horizon_core::nbt::Tag,
    ) {
        route!(
            self,
            "net/minecraft/world/level/block/entity/BaseContainerBlockEntity#sendChestLockedNotifications(Lnet/minecraft/world/phys/Vec3;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/network/chat/Component;)V",
            Recv::Static,
            (at, self.java_player_arg(), ComponentArg(name))
        );
        self.send_chest_locked_notifications_body(conn, at, name);
    }
}
