// game-module-cfg: feature java-mods
//! The acting player's inventory methods that mods' mixins can patch.

use super::java_routing::{Recv, route};
use super::*;
use horizon_client::pick_item;

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

    /// `Inventory.setSelectedSlot`'s body: the acting player's own slot; the client already knows it.
    pub(in crate::game) fn set_selected_slot_body(&mut self, slot: i32) {
        if let Ok(slot) = usize::try_from(slot)
            && slot < 9
        {
            self.selected = slot;
        }
    }

    /// `ServerGamePacketListenerImpl.tryPickItem`'s inventory part through the methods mods patch.
    pub(in crate::game) fn pick_item_routed(&mut self, stack: &ItemStack, infinite: bool) {
        match pick_item::find_slot_matching_item(&self.inventory[..36], stack) {
            Some(slot) if slot < 9 => self.set_selected_slot(slot as i32),
            Some(slot) => self.pick_slot(slot as i32),
            None if infinite => self.add_and_pick_item(stack),
            None => {}
        }
    }

    /// `Inventory.pickSlot`: a main-inventory stack swaps into a suitable hotbar slot.
    pub(in crate::game) fn pick_slot(&mut self, slot: i32) {
        route!(
            self,
            "net/minecraft/world/entity/player/Inventory#pickSlot(I)V",
            Recv::Player,
            (slot)
        );
        self.pick_slot_body(slot);
    }

    /// `Inventory.pickSlot`'s body.
    pub(in crate::game) fn pick_slot_body(&mut self, slot: i32) {
        let suitable = pick_item::suitable_hotbar_slot(&self.inventory[..36], self.selected);
        self.set_selected_slot(suitable as i32);
        if let Ok(slot) = usize::try_from(slot)
            && slot < 36
        {
            pick_item::swap_selected_with(&mut self.inventory[..36], self.selected, slot);
        }
    }

    /// `Inventory.addAndPickItem`: a creative pick puts the stack in a suitable hotbar slot.
    pub(in crate::game) fn add_and_pick_item(&mut self, stack: &ItemStack) {
        route!(
            self,
            "net/minecraft/world/entity/player/Inventory#addAndPickItem(Lnet/minecraft/world/item/ItemStack;)V",
            Recv::Player,
            (stack)
        );
        self.add_and_pick_item_body(stack);
    }

    /// `Inventory.addAndPickItem`'s body: what the hotbar slot held moves to a free slot, or is replaced.
    pub(in crate::game) fn add_and_pick_item_body(&mut self, stack: &ItemStack) {
        let suitable = pick_item::suitable_hotbar_slot(&self.inventory[..36], self.selected);
        self.set_selected_slot(suitable as i32);
        let free = usize::try_from(self.free_slot()).ok();
        pick_item::place_picked(&mut self.inventory[..36], self.selected, stack, free);
    }

    /// `Inventory.getFreeSlot`: the first empty main-inventory slot, or -1.
    pub(in crate::game) fn free_slot(&mut self) -> i32 {
        route!(
            self,
            "net/minecraft/world/entity/player/Inventory#getFreeSlot()I",
            Recv::Player,
            ()
        );
        self.free_slot_body()
    }

    /// `Inventory.getFreeSlot`'s body.
    pub(in crate::game) fn free_slot_body(&self) -> i32 {
        pick_item::free_slot_in(&self.inventory[..36]).map_or(-1, |slot| slot as i32)
    }
}
