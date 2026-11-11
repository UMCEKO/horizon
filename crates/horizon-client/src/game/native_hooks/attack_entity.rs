use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// A player attacks the entity `entity` (`Entity.getId`) of type `kind`
    /// (`minecraft:item_frame`, `minecraft:cow`, `minecraft:player`) standing
    /// at `pos`: `ServerGamePacketListenerImpl.handleInteract`'s attack, once
    /// in reach, where `Player.attack` starts. The swing has not begun, so the
    /// attacking player's attack strength is still the charge the blow uses
    /// (vanilla reads it at the start of `Player.attack`; the swing resets it
    /// afterwards). Set `canceled` and the blow does nothing (no damage, no
    /// knockback, the frame keeps its item). Server-side, for every player of
    /// the world (`native_acting_player` says who).
    pub(in crate::game) fn native_attack_entity(
        &mut self,
        entity: i32,
        kind: &str,
        pos: [f64; 3],
        canceled: &mut bool,
    ) {
        self.java_hook_attack(entity, canceled);
        self.fire_attack_entity(entity, kind, pos, canceled);
    }
}
