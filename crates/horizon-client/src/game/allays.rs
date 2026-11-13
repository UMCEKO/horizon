//! Allays on the game side: the hand-over interaction (`Allay.mobInteract`)
//! and its client prediction. Their tick runs in `horizon_gameplay::allays`.
use super::Game;
use horizon_client::{
    first_person::Hand,
    items::ItemStack,
    mobs::{Species, allay},
};

impl Game<'_> {
    /// The client's `Allay.mobInteract` runs before the server's response.
    /// Whether `predict_allay_interaction` would act, on a copy of the allay.
    pub(super) fn allay_would_take(
        &self,
        index: usize,
        hand: Hand,
        offered: Option<ItemStack>,
    ) -> bool {
        let duplicates = offered.as_ref().is_some_and(|s| {
            self.items()
                .item_tags
                .get("minecraft:duplicates_allays")
                .is_some_and(|tag| tag.contains(&*s.item))
        });
        allay::interaction::predict(
            &mut self.mobs[index].clone(),
            offered.as_ref(),
            hand == Hand::Main,
            duplicates,
            self.acting_uuid(),
        )
        .is_some()
    }

    pub(super) fn predict_allay_interaction(
        &mut self,
        index: usize,
        hand: Hand,
        offered: Option<ItemStack>,
    ) -> bool {
        use allay::interaction::{Interaction, predict};
        let duplicates = offered.as_ref().is_some_and(|s| {
            self.items()
                .item_tags
                .get("minecraft:duplicates_allays")
                .is_some_and(|tag| tag.contains(&*s.item))
        });
        let actor = self.acting_uuid();
        let Some(result) = predict(
            &mut self.mobs[index],
            offered.as_ref(),
            hand == Hand::Main,
            duplicates,
            actor,
        ) else {
            return false;
        };
        self.play_mob_bound_sound(index, result.sound(), 6, 2.0, 1.0, false);
        match result {
            Interaction::Duplicate | Interaction::Give => {
                self.with_hand(hand, Self::consume_held);
            }
            Interaction::Take(stack) => {
                // Player.addItem returns leftovers on the client without
                // spawning an item entity; the server owns any dropped stack.
                let _ = self.give(stack);
            }
        }
        true
    }

    /// `Allay.aiStep`'s looting and the brain's item behaviours, once a tick.
    pub(super) fn tick_allays(&mut self) {
        horizon_gameplay::allays::tick_allays(self);
    }
    /// `Allay.mobInteract`: duplicate when dancing with an amethyst shard,
    /// take the held item, or hand back what it holds.
    pub(super) fn interact_allay(&mut self, index: usize, item: Option<&ItemStack>) -> bool {
        let m = &self.mobs[index];
        if m.species() != Species::Allay || !m.alive() {
            return false;
        }
        let at = [m.position.x, m.position.y, m.position.z];
        let shard = item.is_some_and(|s| {
            self.items()
                .item_tags
                .get("minecraft:duplicates_allays")
                .is_some_and(|tag| tag.contains(&*s.item))
        });
        if m.extra.flag && shard && m.extra.egg_time == 0 {
            let pos = at;
            self.mobs[index].extra.egg_time = 6000;
            if let Some(id) = self.spawn_mob_raw("minecraft:allay", pos, true)
                && let Some(c) = self.mobs.iter_mut().find(|m| m.id == id)
            {
                c.extra.egg_time = 6000;
            }
            // Allay.duplicateAllay: entity event 18, three spawnHeartParticle.
            let id = self.mobs[index].id;
            self.mob_event_particles(id, 18);
            self.play_mob_bound_sound(
                index,
                "minecraft:block.amethyst_block.chime",
                6,
                2.0,
                1.0,
                true,
            );
            self.consume_held();
            return true;
        }
        let acting = self.acting_uuid();
        let holds = m.extra.main_hand.is_some();
        if !holds && let Some(mut given) = item.cloned() {
            let m = &mut self.mobs[index];
            // `Allay.mobInteract` hands over one item, its components and all.
            given.count = 1;
            m.extra.set_main_hand_stack(Some(given));
            // `Allay.mobInteract` remembers LIKED_PLAYER by UUID.
            allay::like_player(m, acting);
            self.play_mob_bound_sound(
                index,
                "minecraft:entity.allay.item_given",
                6,
                2.0,
                1.0,
                true,
            );
            self.consume_held();
            return true;
        }
        // Allay.mobInteract takes the item back only through an empty main hand.
        if holds && item.is_none() && self.hand == Hand::Main {
            let m = &mut self.mobs[index];
            // The hand's stack keeps the components (a potion) the items came with.
            let held = m
                .extra
                .main_hand_stack()
                .cloned()
                .unwrap_or_else(|| ItemStack::new("minecraft:air", 1));
            m.extra.set_main_hand(None);
            let carried = m.extra.allay.as_mut().and_then(|st| st.inventory.take());
            allay::forget_liked_player(m);
            self.play_mob_bound_sound(
                index,
                "minecraft:entity.allay.item_taken",
                6,
                2.0,
                1.0,
                true,
            );
            // Allay.mobInteract ends with swing(MAIN_HAND, ...).
            self.mobs[index].swing();
            if let Some(stack) = carried {
                self.spawn_item(stack, at, [0.0, 0.2, 0.0], 40);
            }
            if let Some(rest) = self.give(ItemStack { count: 1, ..held })
                && let Some(eye) = self.eye_position()
            {
                self.spawn_item(rest, eye, [0.0; 3], 0);
            }
            return true;
        }
        false
    }
}

#[cfg(test)]
#[path = "allay_jukebox_source_tests.rs"]
mod allay_jukebox_source_tests;
