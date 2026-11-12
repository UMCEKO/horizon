// game-module-cfg: feature java-mods
// java-listener-family
//! A player's block placement (`BlockEvent.EntityPlaceEvent`, `EntityMultiPlaceEvent`), posted after its captured writes.

use super::Game;
use super::java_listener::{Family, listens};
use horizon_core::geometry::Direction;
use horizon_java_host::bridge::Ret;
use horizon_java_host::wire::Out;
use horizon_world::level::Snapshot;
use mod_api::HookId;

const HOOKS: &[HookId] = &[HookId::EntityPlace];

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(PlacePosts),
    }
}

struct PlacePosts;

impl mod_api::Hooks for PlacePosts {
    // NeoForge posts after the block is written, so `use_hand` posts from its capture rather than this pre-write hook.
    fn subscribed(&self) -> &'static [Option<HookId>] {
        &[]
    }
}

/// `face` (its 3D data value), then each snapshot's position, old state as Java's id and flags.
fn fields(face: Direction, snapshots: &[Snapshot]) -> Out {
    let mut out = Out::default();
    out.0.push(face as u8);
    out.int(snapshots.len() as i32);
    for snapshot in snapshots {
        out.long(snapshot.pos.pack());
        out.int(snapshot.old.into_raw());
        out.int(snapshot.flags);
    }
    out
}

impl Game<'_> {
    /// Whether a Java mod hears placements: `use_hand` then places under NeoForge's capture.
    pub(super) fn java_place_listened(&self) -> bool {
        listens(HookId::EntityPlace)
    }

    /// `CommonHooks.onPlaceItemIntoWorld`'s event over the captured `snapshots`: whether a listener cancelled it.
    pub(super) fn java_post_place(&mut self, face: Direction, snapshots: &[Snapshot]) -> bool {
        if snapshots.is_empty() || !listens(HookId::EntityPlace) {
            return false;
        }
        self.java_bind_states();
        let fields = fields(face, snapshots);
        self.java_post_player_event("BlockEvent.EntityPlaceEvent", fields.0)
            .is_some_and(|(canceled, _)| canceled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_core::position::BlockPos;
    use horizon_java_host::wire::In;
    use horizon_world::registry::BlockStateId;

    #[test]
    fn the_fields_carry_the_face_and_every_snapshot_in_order() {
        let snapshots = [
            Snapshot {
                pos: BlockPos::new(1, 64, -2),
                old: BlockStateId(0),
                flags: 11,
            },
            Snapshot {
                pos: BlockPos::new(1, 65, -2),
                old: BlockStateId(0),
                flags: 3,
            },
        ];
        let out = fields(Direction::West, &snapshots);
        let mut read = In(&out.0);
        assert_eq!(read.byte(), Some(4), "Direction.WEST.get3DDataValue()");
        assert_eq!(read.int(), Some(2));
        for snapshot in &snapshots {
            assert_eq!(read.long(), Some(snapshot.pos.pack()));
            read.int().unwrap();
            assert_eq!(read.int(), Some(snapshot.flags));
        }
        assert!(read.0.is_empty());
    }
}
