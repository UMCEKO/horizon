// game-module-cfg: feature java-mods
//! The acting guest's packet handlers that mods' mixins patch inside their bodies (W12 SPEC-15).

use super::java_routing::{PickBlockArg, PickEntityArg, Ran, Recv, UseOnArg, route, route_live};
use super::*;
use std::cell::Cell;

const USE_ITEM_ON: &str = "net/minecraft/server/network/ServerGamePacketListenerImpl#handleUseItemOn(Lnet/minecraft/network/protocol/game/ServerboundUseItemOnPacket;)V";

/// What native's `useItemOn` left for after the guest's turn, when Java's handler called it.
#[derive(Clone, Copy, Default)]
pub(in crate::game) struct AfterUse {
    pub(in crate::game) swung: Option<bool>,
    pub(in crate::game) portal: Option<[i32; 3]>,
}

thread_local! {
    pub(in crate::game) static AFTER_USE: Cell<AfterUse> = const { Cell::new(AfterUse { swung: None, portal: None }) };
}

impl Game<'_> {
    /// `ServerGamePacketListenerImpl.handlePickItemFromBlock` for the acting guest.
    pub(in crate::game) fn handle_pick_item_from_block(
        &mut self,
        pos: [i32; 3],
        include_data: bool,
    ) {
        route!(
            self,
            "net/minecraft/server/network/ServerGamePacketListenerImpl#handlePickItemFromBlock(Lnet/minecraft/network/protocol/game/ServerboundPickItemFromBlockPacket;)V",
            Recv::Player,
            (PickBlockArg {
                pos: horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]),
                include_data,
            })
        );
        self.handle_pick_item_from_block_body(pos, include_data);
    }

    /// `ServerGamePacketListenerImpl.handlePickItemFromEntity` for the acting guest.
    pub(in crate::game) fn handle_pick_item_from_entity(&mut self, id: i32, include_data: bool) {
        route!(
            self,
            "net/minecraft/server/network/ServerGamePacketListenerImpl#handlePickItemFromEntity(Lnet/minecraft/network/protocol/game/ServerboundPickItemFromEntityPacket;)V",
            Recv::Player,
            (PickEntityArg { id, include_data })
        );
        self.handle_pick_item_from_entity_body(id, include_data);
    }

    /// `ServerGamePacketListenerImpl.handleUseItemOn` for the acting guest: `Ran::Native` leaves the packet to native's handler.
    pub(in crate::game) fn handle_use_item_on(&mut self, packet: UseOnArg) -> Ran {
        route!(
            self,
            "net/minecraft/server/network/ServerGamePacketListenerImpl#handleUseItemOn(Lnet/minecraft/network/protocol/game/ServerboundUseItemOnPacket;)V",
            Recv::Player,
            (packet)
        );
        Ran::Native
    }

    /// Guest `conn`'s `ServerboundUseItemOnPacket` through Java's merged `handleUseItemOn`; false when no mod patches it.
    pub(in crate::game) fn java_handle_use_item_on(
        &mut self,
        conn: u32,
        gi: usize,
        hit: Hit,
        hand: Hand,
        sequence: i32,
    ) -> bool {
        if !route_live(USE_ITEM_ON) {
            return false;
        }
        let logged = self.world.block_log.as_ref().map_or(0, Vec::len);
        let heard = self.audio.capture.as_ref().map_or(0, Vec::len);
        AFTER_USE.set(AfterUse::default());
        let packet = UseOnArg {
            hand: i32::from(hand == Hand::Off),
            point: hit.point,
            face: hit.face,
            inside: hit.inside,
            pos: hit.pos,
            sequence,
        };
        let ran = self.as_guest_at(gi, conn, |g| g.handle_use_item_on(packet));
        if !matches!(ran, Ok(Ran::Java)) {
            return false;
        }
        let after = AFTER_USE.take();
        if let Some(pos) = after.portal {
            self.spectator_use_portal(conn, gi, pos);
        }
        self.lan_swing_back(conn, after.swung);
        self.guest_use_sounds(conn, logged, heard);
        true
    }

    /// `ServerPlayerGameMode.useItemOn` from Java's handler, in the guest's turn: native's use, as an `InteractionResult` (`Flat.toInteractionResult`).
    pub(in crate::game) fn java_game_mode_use_item_on(&mut self, hit: Hit, hand: Hand) -> i32 {
        const PASS: i32 = 0;
        const FAIL: i32 = 1;
        const CONSUME: i32 = 3;
        let mut after = AFTER_USE.get();
        // A spectator's portal trip leaves the turn, so it runs once the turn is over.
        if self.spectator() && self.portal_block_at(hit.pos) {
            after.portal = Some(hit.pos);
            AFTER_USE.set(after);
            return CONSUME;
        }
        let used = self.use_item_on_in_turn(hit, hand);
        after.swung = Some(used.swung);
        AFTER_USE.set(after);
        self.java_refresh_player();
        // Native swung already: Java's handler is told `CONSUME`, whose `SwingSource` is `NONE`.
        match (used.consumed, used.failed) {
            (true, true) => FAIL,
            (true, false) => CONSUME,
            (false, _) => PASS,
        }
    }
}
