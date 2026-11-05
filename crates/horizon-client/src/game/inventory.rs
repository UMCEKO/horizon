//! The acting player's inventory methods that mods' mixins can patch.

use super::*;

impl Game<'_> {
    /// `Inventory.setSelectedSlot`: the acting player's hotbar selection.
    pub(in crate::game) fn set_selected_slot(&mut self, slot: i32) {
        route!(
            self,
            "net/minecraft/world/entity/player/Inventory#setSelectedSlot(I)V",
            Recv::Player,
            (slot)
        );
        self.set_selected_slot_body(slot);
    }

    /// `Inventory.setSelectedSlot`'s body.
    pub(in crate::game) fn set_selected_slot_body(&mut self, slot: i32) {
        if let Ok(slot) = usize::try_from(slot)
            && slot < 9
        {
            self.selected = slot;
        }
    }
}
