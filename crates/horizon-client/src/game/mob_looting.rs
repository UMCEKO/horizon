//! `Mob.aiStep`'s looting loop runs in `horizon_gameplay::mob_looting` on the game's
//! `MobLevel`; the pickup trigger and the equipment numbers piglins share stay here.
use super::Game;
use horizon_client::{
    entities::ItemEntity,
    items::ItemStack,
    mobs::{
        Mob,
        equip::{Facts, Slot},
    },
};

impl Game<'_> {
    /// `Mob.aiStep`'s looting block, once per mob per tick.
    pub(super) fn tick_mob_looting(&mut self) {
        horizon_gameplay::mob_looting::tick_mob_looting(self);
    }

    /// The numbers `Mob.compareArmor` and `compareWeapons` read off `stack`
    /// in `slot` for the mob `m`.
    pub(super) fn stack_facts(&self, m: &Mob, stack: &ItemStack, slot: Slot) -> Facts {
        horizon_gameplay::mob_looting::stack_facts(self.items(), m, stack, slot)
    }

    /// `LivingEntity.onItemPickup`: the advancement of whoever threw the item,
    /// whichever mob took it.
    pub(super) fn mob_item_pickup_trigger(&mut self, index: usize, e: &ItemEntity) {
        let Some(owner) = e.thrower else {
            return;
        };
        let stack = e.stack.clone();
        let target = self.mob_entity(&self.mobs[index]);
        self.for_each_player(|g| {
            if g.acting_uuid() == owner {
                g.trigger_item_entity("minecraft:thrown_item_picked_up_by_entity", &stack, &target);
            }
        });
    }
}
