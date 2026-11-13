//! Where entities joining, leaving and saving reach Java mods; each does nothing without `--features java-mods`.
use super::Game;
use super::native_entities::Removal;
use std::collections::HashSet;

impl Game<'_> {
    /// Whether Java listens to entities joining and leaving: its server runs a level.
    pub(super) fn java_hook_hears_entities(&self) -> bool {
        #[cfg(feature = "java-mods")]
        return self.java_level_running();
        #[cfg(not(feature = "java-mods"))]
        false
    }

    /// `EntityJoinLevelEvent` for an entity in its list that has not ticked: Java makes its view and may keep it out, before anything else sees it.
    pub(super) fn java_hook_entity_joining(
        &mut self,
        entity: u32,
        loaded_from_disk: bool,
        canceled: &mut bool,
    ) {
        #[cfg(feature = "java-mods")]
        if !*canceled && !self.java_entity_joining(entity, loaded_from_disk) {
            *canceled = true;
        }
        let _ = (entity, loaded_from_disk, canceled);
    }

    /// `Entity.onAddedToLevel`, once the joined entity is in the level for good.
    pub(super) fn java_hook_entity_added(&mut self, entity: u32) {
        #[cfg(feature = "java-mods")]
        self.java_entity_added(entity);
        let _ = entity;
    }

    /// `onRemovedFromLevel` then `EntityLeaveLevelEvent`, once the entity is out of its list, for the reason its leave was stamped with.
    pub(super) fn java_hook_entity_left(&mut self, entity: u32, kind: &str, why: Removal) {
        #[cfg(feature = "java-mods")]
        self.java_entity_left(entity, kind, why);
        let _ = (entity, kind, why);
    }

    /// A player's view joins (`ServerLevel.addPlayer`) or leaves (`removePlayerImmediately`) the level `dimension`.
    pub(super) fn java_hook_player_level(
        &mut self,
        uuid: u128,
        dimension: &str,
        join: bool,
        why: Removal,
    ) {
        #[cfg(feature = "java-mods")]
        self.java_player_level(uuid, dimension, join, why);
        let _ = (uuid, dimension, join, why);
    }

    /// Before native encodes the entities of these chunks: Java's own keys of each reach what native remembers of it.
    pub(super) fn java_hook_entities_save(&mut self, keys: &HashSet<(i32, i32)>) {
        #[cfg(feature = "java-mods")]
        self.java_entities_save(keys);
        let _ = keys;
    }
}

impl Game<'_> {
    /// `ItemTossEvent` for a stack `Player.drop` threw: `None` when no mod listens, `Some(None)` when it is gone, `Some(Some(id))` when Java spawned the item.
    pub(super) fn java_hook_item_toss(
        &mut self,
        stack: &horizon_client::items::ItemStack,
        at: [f64; 3],
        motion: [f64; 3],
        delay: i32,
        named: bool,
    ) -> Option<Option<u32>> {
        #[cfg(feature = "java-mods")]
        return self.java_item_toss(stack, at, motion, delay, named);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (stack, at, motion, delay, named);
            None
        }
    }

    /// `ArrowLooseEvent`: the charge the mods left, `charge` itself when none listens; negative cancels the shot.
    pub(super) fn java_hook_arrow_loose(
        &mut self,
        hand: super::Hand,
        charge: i32,
        has_ammo: bool,
        crossbow: bool,
    ) -> i32 {
        #[cfg(feature = "java-mods")]
        return self.java_arrow_loose(hand, charge, has_ammo, crossbow);
        #[cfg(not(feature = "java-mods"))]
        {
            let _ = (hand, has_ammo, crossbow);
            charge
        }
    }
}
