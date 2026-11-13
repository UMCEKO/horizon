//! `ServerGamePacketListenerImpl`'s pick handlers and `tryPickItem`, for the acting guest.
use super::*;
use std::cell::Cell;

/// What the pick packet in hand named, for the `player_pick_item` hook.
#[derive(Clone, Copy)]
pub(in crate::game) enum PickSource {
    Block([i32; 3]),
    Entity(i32),
}

thread_local! {
    /// The pick packet in hand: `tryPickItem` may run under Java's merged handler, which passes only the stack.
    pub(in crate::game) static PICKING: Cell<PickSource> = const { Cell::new(PickSource::Block([0; 3])) };
}

impl Game<'_> {
    /// `Player.isWithinBlockInteractionRange(pos, buffer)` for the player in place.
    pub(in crate::game) fn within_block_interaction_range(
        &self,
        pos: [i32; 3],
        buffer: f64,
    ) -> bool {
        self.eye_position().is_some_and(|eye| {
            horizon_client::lan::guest::within_block_range(eye, self.reach() + buffer, pos)
        })
    }

    /// `handlePickItemFromBlock`'s body: a loaded block in reach, its item (with its data for Ctrl), then `tryPickItem`.
    pub(in crate::game) fn handle_pick_item_from_block_body(
        &mut self,
        pos: [i32; 3],
        include_data: bool,
    ) {
        if !self.within_block_interaction_range(pos, 1.0) || !self.world.loaded(pos[0], pos[2]) {
            return;
        }
        // The block's item, a part's in its place, then the block entity's data on whichever won.
        let include_data = self.creative && include_data;
        let stack = self.clone_stack(pos, include_data);
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let mut replacement = None;
        self.native_clone_item_stack(
            self.dim.level(),
            horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]),
            &self.res.facts().get(state).name,
            state,
            include_data,
            &mut replacement,
        );
        let Some(mut stack) = replacement.or(stack) else {
            return;
        };
        if include_data {
            self.add_block_data(pos, &mut stack);
        }
        self.try_pick_item_routed(&stack);
    }

    /// `handlePickItemFromEntity`'s body: an entity in reach, then `tryPickItem` with its pick result.
    pub(in crate::game) fn handle_pick_item_from_entity_body(
        &mut self,
        id: i32,
        _include_data: bool,
    ) {
        if !self.within_entity_range(id, 3.0) {
            return;
        }
        if let Some(stack) = self.entity_pick_result(id) {
            self.try_pick_item_routed(&stack);
        }
    }

    fn try_pick_item_routed(&mut self, stack: &ItemStack) {
        #[cfg(feature = "java-mods")]
        self.try_pick_item(stack);
        #[cfg(not(feature = "java-mods"))]
        self.try_pick_item_body(stack);
    }

    /// `tryPickItem`'s body: the native mods may cancel, then the pick, then the held slot goes back to the client.
    pub(in crate::game) fn try_pick_item_body(&mut self, stack: &ItemStack) {
        let (Some(uuid), Some(name)) = (self.lan_acting_uuid, self.lan_acting_name.clone()) else {
            return;
        };
        let (from_entity, pos, entity_id) = match PICKING.get() {
            PickSource::Block(pos) => (false, pos, 0),
            PickSource::Entity(id) => (true, [0; 3], id),
        };
        let mut cancel = false;
        self.native_player_pick_item(uuid, &name, from_entity, pos, entity_id, &mut cancel);
        if cancel {
            return;
        }
        #[cfg(feature = "java-mods")]
        self.pick_item_routed(stack, self.creative);
        #[cfg(not(feature = "java-mods"))]
        horizon_client::pick_item::pick(
            &mut self.inventory[..36],
            &mut self.selected,
            stack,
            self.creative,
        );
        let (Some(conn), Some(host)) = (self.lan_acting, self.lan.as_ref()) else {
            return;
        };
        host.hub.send_to(
            conn,
            &C::SetHeldSlot(play::SetHeldSlot {
                slot: self.selected as i32,
            }),
        );
    }
}
