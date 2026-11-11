// game-module-cfg: feature java-mods
//! The acting player's inventory methods that mods' mixins can patch.
#![allow(dead_code)]

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

    /// `Inventory.setSelectedSlot`'s body, through the server's own write of the acting player's slot.
    pub(in crate::game) fn set_selected_slot_body(&mut self, slot: i32) {
        if let (Ok(slot), Some(player)) = (usize::try_from(slot), self.native_acting_player()) {
            self.player_set_selected_slot(player, slot);
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
            self.inventory.swap(self.selected, slot);
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
        let selected = self.selected;
        if self.inventory[selected].is_some()
            && let Ok(free) = usize::try_from(self.free_slot())
        {
            self.inventory[free] = self.inventory[selected].take();
        }
        self.inventory[selected] = Some(stack.clone());
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
        self.inventory[..36]
            .iter()
            .position(Option::is_none)
            .map_or(-1, |slot| slot as i32)
    }
}
