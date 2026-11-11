//! Where entities joining, leaving and saving reach Java mods; each does nothing without `--features java-mods`.
use super::Game;
use std::collections::HashSet;

impl Game<'_> {
    /// Whether Java listens to entities joining and leaving: its server runs a level.
    pub(super) fn java_hook_hears_entities(&self) -> bool {
        #[cfg(feature = "java-mods")]
        return self.java_level_running();
        #[cfg(not(feature = "java-mods"))]
        false
    }

    /// `EntityJoinLevelEvent` for an entity already in its list: Java makes its view and may keep it out (`addEntity`'s synchronous refusal is #181's).
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

    /// `EntityLeaveLevelEvent`, once the entity is out of its list: Java drops its view, told `DISCARDED` as the leave carries no reason.
    pub(super) fn java_hook_entity_left(&mut self, entity: u32) {
        #[cfg(feature = "java-mods")]
        self.java_entity_left(entity, super::java_entities::Removal::Discarded);
        let _ = entity;
    }

    /// Before native encodes the entities of these chunks: Java's own keys of each reach what native remembers of it.
    pub(super) fn java_hook_entities_save(&mut self, keys: &HashSet<(i32, i32)>) {
        #[cfg(feature = "java-mods")]
        self.java_entities_save(keys);
        let _ = keys;
    }
}
