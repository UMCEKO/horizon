// game-module-cfg: feature java-mods
//! What the acting guest's packet handlers ask native when Java runs them; reach and mode checks read the filled view.

use super::*;
use horizon_java_host::bridge::{Doubles, Ent};

/// Writes a stack as `Flat.toItemStack` reads it: its saved tag, empty bytes for none.
fn stack_bytes(stack: Option<ItemStack>) -> Bytes {
    Bytes(
        stack
            .and_then(|s| java_mods::write_tag(horizon_client::save::item_to_nbt(&s)))
            .unwrap_or_default(),
    )
}

/// `Flat.raw(BlockHitResult)`: the point, the face's ordinal, 1 if inside, then the block.
fn read_hit(hit: &[f64]) -> Option<Hit> {
    let &[x, y, z, face, inside, bx, by, bz] = hit else {
        return None;
    };
    let face = match face as i32 {
        0 => [0, -1, 0],
        1 => [0, 1, 0],
        2 => [0, 0, -1],
        3 => [0, 0, 1],
        4 => [-1, 0, 0],
        _ => [1, 0, 0],
    };
    Some(Hit {
        pos: [bx as i32, by as i32, bz as i32],
        face,
        point: [x, y, z],
        inside: inside != 0.0,
    })
}

horizon_java_host::java_bridges! { table for Game<'static> {
    "net/minecraft/world/entity/player/Player".resetAttackStrengthTicker
    "()V" if "net/minecraft/server/level/ServerPlayer" =>
    fn player_reset_attack_strength(game) -> () {
        game.reset_attack_strength();
    }

    "net/minecraft/world/entity/Entity".getPickResult
    "()Lnet/minecraft/world/item/ItemStack;" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_pick_result(game, entity: Ent) -> Bytes {
        stack_bytes(game.entity_pick_result(entity.0 as i32))
    }

    "net/minecraft/server/level/ServerPlayer".sendBuildLimitMessage
    "(ZI)V" =>
    fn player_build_limit_message(game, too_high: bool, limit: i32) -> () {
        if let Some(conn) = game.lan_acting {
            game.build_limit_message(conn, too_high, limit);
        }
    }

    "net/minecraft/server/level/ServerPlayer".sendSpawnProtectionMessage
    "(Lnet/minecraft/core/BlockPos;)V" =>
    fn player_spawn_protection_message(game, pos: Pos) -> () {
        if let Some(conn) = game.lan_acting {
            game.spawn_protection_message(conn, [pos.0.x, pos.0.y, pos.0.z]);
        }
    }

    "net/minecraft/server/network/ServerGamePacketListenerImpl".ackBlockChangesUpTo
    "(I)V" =>
    fn connection_ack_block_changes(game, sequence: i32) -> () {
        if let (Some(conn), Some(host)) = (game.lan_acting, game.lan.as_ref()) {
            host.hub.ack(conn, sequence);
        }
    }

    "net/minecraft/server/dedicated/DedicatedServer".isUnderSpawnProtection
    "(Lnet/minecraft/server/level/ServerLevel;Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/entity/player/Player;)Z" =>
    fn server_under_spawn_protection(game, _level: Skip, pos: Pos, player: Ent) -> bool {
        game.guest_with_entity_id(player.0 as i32)
            .is_some_and(|gi| game.under_spawn_protection(gi, [pos.0.x, pos.0.y, pos.0.z]))
    }

    "net/minecraft/server/level/ServerLevel".mayInteract
    "(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/core/BlockPos;)Z" =>
    fn level_may_interact(game, on: LevelOn, entity: Ent, pos: Pos) -> bool {
        // Only a player is ever refused (`entity instanceof Player`).
        game.on_level(&on, |game| {
            game.guest_with_entity_id(entity.0 as i32)
                .is_none_or(|gi| game.may_interact(gi, [pos.0.x, pos.0.y, pos.0.z]))
        })
    }

    "net/minecraft/server/level/ServerPlayerGameMode".useItemOn
    "(Lnet/minecraft/server/level/ServerPlayer;Lnet/minecraft/world/level/Level;Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/InteractionHand;Lnet/minecraft/world/phys/BlockHitResult;)Lnet/minecraft/world/InteractionResult;"
    if "@dev/horizonlab/javamods/boot/Routing.nativeUseItemOn" =>
    fn game_mode_use_item_on(game, _player: Skip, _level: Skip, _stack: Skip, hand: i32, hit: Doubles) -> i32 {
        let Some(hit) = read_hit(&hit.0) else {
            panic!("Horizon: ServerPlayerGameMode.useItemOn from Java gave no block hit");
        };
        game.java_game_mode_use_item_on(hit, if hand == 0 { Hand::Main } else { Hand::Off })
    }

    "dev/horizonlab/javamods/boot/Clients".nativeBlockUpdate
    "(JJJ)V" =>
    fn connection_block_update(game, most: i64, least: i64, pos: Pos) -> () {
        let uuid = (u128::from(most as u64) << 64) | u128::from(least as u64);
        let pos = pos.0;
        let state = game.world.block(pos.x, pos.y, pos.z).0 as i32;
        let Some(host) = game.lan.as_ref() else { return };
        if let Some(g) = host.hub.guests.iter().find(|g| g.uuid == uuid) {
            host.hub.send_to(
                g.conn,
                &horizon_protocol::packets::play::Clientbound::BlockUpdate(horizon_protocol::packets::play::BlockUpdate {
                    pos: [pos.x, pos.y, pos.z],
                    state,
                }),
            );
        }
    }
}}
