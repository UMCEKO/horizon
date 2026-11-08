//! `entity_constructing` (`EntityEvent.EntityConstructing`): vanilla's `Entity.<init>`, once per
//! entity on the side that makes it. Horizon has no one constructor, so the hook is told where
//! each kind of entity starts: a spawned mob (`spawn_mob`), an entity read from disk or a
//! summon's NBT (`remember_uuid`, before its saved attachments are read), every other new
//! entity in the join pass just before `entity_join_level`, a LAN guest's player as it is
//! loaded or respawned, and on a client of a server the entities and players the server adds.
//! The first attachment access of an entity not yet told tells it first, so no mod reads an
//! attachment before the listeners have set it.

use super::Game;
use mod_api::Holder;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
#[path = "entity_initialization/tests.rs"]
mod tests;

#[derive(Default)]
pub(super) struct EntityInitialization {
    /// The UUIDs already told. An entity is forgotten when it leaves the world (`forget_entity`:
    /// death, a chunk unloading), so the next one made from its data is
    /// told again, as vanilla constructs a new entity for each of those.
    initialized: HashSet<[i32; 4]>,
    /// Each LAN guest's connection when its player was last told: a new connection is a new
    /// `ServerPlayer`.
    players: HashMap<u128, u32>,
    /// The holder being told now: its attachments are reachable before it is in the level.
    pub(super) active: Option<Holder>,
}

impl Game<'_> {
    /// Tell the listeners about a new entity of type `kind`, unless it was already told. Marked
    /// before the dispatch, so a listener's own attachment access doesn't tell it again.
    pub(super) fn initialize_entity(&mut self, holder: Holder, kind: &str) {
        if !mod_api::__private::listened(mod_api::HookId::EntityConstructing) {
            return;
        }
        let uuid = match holder {
            Holder::Entity(id) => self.uuid_for(id),
            Holder::Player(uuid) => horizon_client::lan::playerdata::uuid_ints(uuid),
            _ => return,
        };
        if !self.entity_initialization.initialized.insert(uuid) {
            return;
        }
        let previous = self.entity_initialization.active.replace(holder);
        self.native_entity_constructing(holder, kind);
        self.entity_initialization.active = previous;
    }

    /// `initialize_entity` for a holder whose attachments are about to be read, its type
    /// looked up: an entity that started on a path that doesn't tell (one made and touched
    /// within a pass, before the join pass) is told before the read.
    pub(super) fn initialize_entity_on_access(&mut self, holder: Holder) {
        if !mod_api::__private::listened(mod_api::HookId::EntityConstructing) {
            return;
        }
        match holder {
            Holder::Entity(id) => {
                let Some(kind) = self.entity(id).and_then(|entity| self.entity_kind(entity)) else {
                    return;
                };
                self.initialize_entity(holder, &kind);
            }
            Holder::Player(_) => self.initialize_entity(holder, "minecraft:player"),
            _ => {}
        }
    }

    pub(super) fn forget_entity_initialization(&mut self, uuid: [i32; 4]) {
        self.entity_initialization.initialized.remove(&uuid);
    }

    /// A LAN guest's player, its file just read for connection `conn`: told once per
    /// connection (`PlayerList.placeNewPlayer` constructs the `ServerPlayer`, then
    /// `loadPlayerData` reads its file).
    pub(super) fn initialize_server_player(&mut self, uuid: u128, conn: u32) {
        if self.entity_initialization.players.insert(uuid, conn) != Some(conn) {
            self.forget_entity_initialization(horizon_client::lan::playerdata::uuid_ints(uuid));
        }
        self.initialize_guest_player_under_data(uuid, conn);
    }

    /// `PlayerList.respawn`: the guest gets a new `ServerPlayer`, told again, and the data it
    /// keeps (`copy_on_death`, already in place) is restored over it.
    pub(super) fn reinitialize_server_player(&mut self, uuid: u128, conn: u32) {
        self.forget_entity_initialization(horizon_client::lan::playerdata::uuid_ints(uuid));
        self.initialize_guest_player_under_data(uuid, conn);
    }

    /// Tell a guest's player whose attachments already hold its data, then read that data
    /// back over what the listeners set: the end state of vanilla's order, where the player is
    /// constructed before its data is read (`Entity.load`, `restoreFrom`).
    fn initialize_guest_player_under_data(&mut self, uuid: u128, conn: u32) {
        if !mod_api::__private::listened(mod_api::HookId::EntityConstructing) {
            return;
        }
        let data = self
            .lan
            .as_mut()
            .and_then(|host| host.hub.guests.attachments_mut(conn))
            .map(|attachments| attachments.to_nbt());
        self.initialize_entity(Holder::Player(uuid), "minecraft:player");
        if let Some(data) = data
            && let Some(attachments) = self
                .lan
                .as_mut()
                .and_then(|host| host.hub.guests.attachments_mut(conn))
        {
            attachments.read_saved_over(data);
        }
    }
}
