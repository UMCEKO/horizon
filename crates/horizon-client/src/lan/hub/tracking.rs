//! The entities each player starts and stops tracking, batched for the game to tell the mods.

use super::{AttachmentSnapshot, Guest, Holder, queue_snapshots};

/// What one player started and stopped tracking, in order, for the game to
/// tell the mods.
pub struct TrackingBatch {
    pub player: u128,
    pub name: String,
    pub events: Vec<crate::lan::entity::TrackEvent>,
}

/// Moves a guest's tracking events into `out`, one batch, and the entities it
/// started tracking into `snapshots`.
pub(super) fn take_tracking(
    g: &mut Guest,
    out: &mut Vec<TrackingBatch>,
    snapshots: &mut Vec<AttachmentSnapshot>,
    sync_attachments: bool,
) {
    let events = g.tracker.take_events();
    if !events.is_empty() {
        if sync_attachments {
            let started = events.iter().filter(|e| e.start);
            queue_snapshots(
                snapshots,
                g.uuid,
                started.map(|e| Holder::Entity(e.entity as u32)),
            );
        }
        out.push(TrackingBatch {
            player: g.uuid,
            name: g.name.clone(),
            events,
        });
    }
}

