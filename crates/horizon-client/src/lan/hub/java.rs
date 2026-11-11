//! The hub's side of the server's Java mods; their chunk Sent and Unwatch come from `ChunkWatchLog`.

use super::{C, Hub};
use crate::lan::guest::{Guest, GuestKind};

/// A change in what `ChunkMap` knows of a player, for the Java side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tracking {
    /// `ChunkMap.updateChunkTracking`: the player's `ChunkTrackingView`.
    View {
        uuid: u128,
        entity_id: i32,
        dimension: String,
        center: (i32, i32),
        radius: i32,
    },
    /// Every chunk dropped at once: a respawn, a change of level.
    Reset { uuid: u128 },
    /// `TrackedEntity.seenBy` gained or lost this player.
    Pairing {
        uuid: u128,
        entity: i32,
        paired: bool,
    },
    /// The player left the server.
    Left { uuid: u128 },
}

/// `g`'s view moved to `center`.
pub(super) fn view(log: &mut Vec<Tracking>, g: &Guest, center: (i32, i32)) {
    note(
        log,
        Tracking::View {
            uuid: g.uuid,
            entity_id: g.entity_id,
            dimension: g.dimension.clone(),
            center,
            radius: g.view_distance,
        },
    );
}

/// `g` dropped every chunk it had.
pub(super) fn reset(log: &mut Vec<Tracking>, g: &Guest) {
    note(log, Tracking::Reset { uuid: g.uuid });
}

/// `g` left the server.
pub(super) fn left(log: &mut Vec<Tracking>, g: &Guest) {
    note(log, Tracking::Left { uuid: g.uuid });
}

fn note(log: &mut Vec<Tracking>, change: Tracking) {
    if crate::lan::chunk_watch::java_side() {
        log.push(change);
    }
}

fn java_client(g: &Guest) -> bool {
    g.java && g.kind == GuestKind::Network
}

impl Hub {
    /// Views, resets and departures since the last call, then the pairings left in `native_tracking`.
    pub fn take_tracking(&mut self) -> Vec<Tracking> {
        self.collect_tracking();
        let mut out = std::mem::take(&mut self.java_tracking);
        for batch in &self.native_tracking {
            out.extend(batch.events.iter().map(|e| Tracking::Pairing {
                uuid: batch.player,
                entity: e.entity,
                paired: e.start,
            }));
        }
        out
    }

    /// One frame of `horizon:java` for the Java client of the guest `uuid`.
    pub fn send_java(&self, uuid: u128, frame: Vec<u8>) {
        let body = C::CustomPayload(horizon_protocol::packets::configuration::CustomPayload {
            channel: horizon_protocol::java::CHANNEL.into(),
            data: frame,
        })
        .encode();
        for g in self
            .guests
            .iter()
            .filter(|g| java_client(g) && g.uuid == uuid)
        {
            g.sender.send(body.clone());
        }
    }

    /// The guests with a Java client of their own, in the order they joined.
    pub fn java_guests(&self) -> impl Iterator<Item = u128> + '_ {
        self.guests
            .iter()
            .filter(|g| java_client(g))
            .map(|g| g.uuid)
    }

    /// `MinecraftServer.getProfilePermissions`: a player's command level, joined or not.
    pub fn permission_of(&self, uuid: u128) -> u8 {
        match self.guests.iter().find(|g| g.uuid == uuid) {
            Some(guest) => guest.permission,
            None => self.default_permission(uuid),
        }
    }

    /// `PlayerList.isOp`: on the operators' list, or the single-player owner with commands allowed.
    pub fn is_op(&self, uuid: u128) -> bool {
        match self.ops.get(&uuid) {
            Some(&level) if self.singleplayer_owner == Some(uuid) => level > 0,
            Some(_) => true,
            None => false,
        }
    }
}
