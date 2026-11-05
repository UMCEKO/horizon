//! The acting player, on its view.

use super::*;
use horizon_client::effects::EffectInstance;
use horizon_client::first_person::SwingAnimation;

/// `ServerGamePacketListenerImpl.teleport`: the player stands there, the client hears of it as the turn ends.
fn put_player(game: &mut Game<'static>, at: Vec3) {
    if let Some(player) = game.player.as_mut() {
        player.position = at;
    }
    game.previous_position = at;
    game.java_refresh_player();
}

fn current_position(game: &Game<'static>) -> Vec3 {
    game.player
        .as_ref()
        .map_or(Vec3::new(0.0, 0.0, 0.0), |p| p.position)
}

fn change_mode(game: &mut Game<'static>, id: i32) -> bool {
    let changed = game.set_game_mode(game_mode::mode_from_id(id));
    game.java_refresh_player();
    changed
}

fn parse_effect(text: &str) -> Option<EffectInstance> {
    let mut fields = text.split('\t');
    let definition = horizon_client::brewing::effect(fields.next()?)?;
    let mut effect = EffectInstance::new(
        definition.id,
        fields.next()?.parse().ok()?,
        fields.next()?.parse().ok()?,
    );
    effect.ambient = fields.next()? == "1";
    effect.visible = fields.next()? == "1";
    Some(effect)
}

horizon_java_host::java_bridges! { table for Game<'static> {
    "net/minecraft/world/entity/LivingEntity".releaseUsingItem
    "()V" if "net/minecraft/server/level/ServerPlayer" =>
    fn player_release_using_item(game) -> () {
        game.java_release_using_item();
        game.java_refresh_player();
    }

    "net/minecraft/server/level/ServerPlayer".teleportTo
    "(DDD)V" =>
    fn player_teleport_to(game, x: f64, y: f64, z: f64) -> () {
        put_player(game, Vec3::new(x, y, z));
    }

    "net/minecraft/server/level/ServerPlayer".teleportRelative
    "(DDD)V" =>
    fn player_teleport_relative(game, dx: f64, dy: f64, dz: f64) -> () {
        let at = current_position(game);
        put_player(game, Vec3::new(at.x + dx, at.y + dy, at.z + dz));
    }

    "net/minecraft/server/level/ServerPlayer".snapTo
    "(DDD)V" =>
    fn player_snap_to(game, x: f64, y: f64, z: f64) -> () {
        put_player(game, Vec3::new(x, y, z));
    }

    "net/minecraft/world/entity/Entity".snapTo
    "(DDDFF)V" if "net/minecraft/server/level/ServerPlayer" =>
    fn player_snap_to_facing(game, x: f64, y: f64, z: f64, yaw: f32, pitch: f32) -> () {
        game.yaw = yaw;
        game.pitch = pitch.clamp(-90.0, 90.0);
        put_player(game, Vec3::new(x, y, z));
    }

    "net/minecraft/world/entity/Entity".setDeltaMovement
    "(Lnet/minecraft/world/phys/Vec3;)V" if "net/minecraft/server/level/ServerPlayer" =>
    fn player_set_delta_movement(game, motion: Point) -> () {
        // A server's player moves by its client, so the push is sent as `hurtMarked` would.
        if motion.0.iter().all(|c| c.is_finite()) && let Some(player) = game.player.as_mut() {
            player.velocity = Vec3::new(motion.0[0], motion.0[1], motion.0[2]);
            player.health.sync_velocity = true;
        }
    }

    "net/minecraft/server/level/ServerPlayer".hurtServer
    "(Lnet/minecraft/server/level/ServerLevel;Lnet/minecraft/world/damagesource/DamageSource;F)Z" =>
    fn player_hurt(game, _level: Skip, source: Source, amount: f32) -> bool {
        let hurt = !game.spectator() && game.hurt_player_from(amount, None, &source.kind, Some(None));
        game.java_refresh_player();
        hurt
    }

    "net/minecraft/world/entity/Entity".thunderHit
    "(Lnet/minecraft/server/level/ServerLevel;Lnet/minecraft/world/entity/LightningBolt;)V" if "net/minecraft/server/level/ServerPlayer" =>
    fn player_thunder_hit(game, _level: Skip, bolt: Bytes) -> () {
        if let Some(hit) = Game::java_bolt_hit(&bolt.0) {
            game.thunder_hit_entity(
                super::super::commands::Target::Player,
                &hit,
                &mut Vec::new(),
            );
        }
        game.java_refresh_player();
    }

    "net/minecraft/world/entity/LivingEntity".swing
    "(Lnet/minecraft/world/InteractionHand;Lnet/minecraft/world/item/component/SwingAnimation;Z)Z" if "net/minecraft/server/level/ServerPlayer" =>
    fn player_swing(game, hand: i32, animation: Point, _to_self: bool) -> bool {
        let before = game.swings_started;
        let animation = SwingAnimation {
            kind: animation.0[0] as u8,
            duration: animation.0[1] as i32,
        };
        game.swing_hand(if hand == 0 { Hand::Main } else { Hand::Off }, animation);
        game.swings_started != before
    }

    "net/minecraft/server/level/ServerPlayer".setGameMode
    "(Lnet/minecraft/world/level/GameType;)Z" =>
    fn player_set_game_mode(game, mode: i32) -> bool {
        change_mode(game, mode)
    }

    "net/minecraft/server/level/ServerPlayerGameMode".changeGameModeForPlayer
    "(Lnet/minecraft/world/level/GameType;)Z" =>
    fn player_change_game_mode(game, mode: i32) -> bool {
        change_mode(game, mode)
    }

    "net/minecraft/server/level/ServerPlayerGameMode".destroyBlock
    "(Lnet/minecraft/core/BlockPos;)Z" =>
    fn player_destroy_block(game, pos: Pos) -> bool {
        let at = [pos.0.x, pos.0.y, pos.0.z];
        let before = game.world.block(at[0], at[1], at[2]);
        let _ = game.break_block(at);
        // Vanilla answers false only for a block it refused to break.
        let broke = game.world.block(at[0], at[1], at[2]) != before || game.res.facts().is_air(before);
        game.after_java_change(broke)
    }

    "net/minecraft/world/item/ItemCooldowns".addCooldown
    "(Lnet/minecraft/resources/Identifier;I)V" if "net/minecraft/world/item/ServerItemCooldowns" =>
    fn player_add_cooldown(game, group: Text, ticks: i32) -> () {
        game.add_cooldown(group.0.into(), ticks);
    }

    "net/minecraft/world/item/ItemCooldowns".removeCooldown
    "(Lnet/minecraft/resources/Identifier;)V" if "net/minecraft/world/item/ServerItemCooldowns" =>
    fn player_remove_cooldown(game, group: Text) -> () {
        game.cooldowns.remove(&group.0);
        if game.lan_acting.is_some() {
            game.guest_cooldowns.push((group.0.into(), 0));
        }
    }

    "dev/horizonlab/javamods/boot/PlayerSync".food
    "(IFF)V" =>
    fn sync_food(game, food: i32, saturation: f32, exhaustion: f32) -> () {
        if let Some(player) = game.player.as_mut() {
            player.health.food.food = food;
            player.health.food.saturation = saturation;
            player.health.food.exhaustion = exhaustion;
        }
    }

    "dev/horizonlab/javamods/boot/PlayerSync".health
    "(F)V" =>
    fn sync_health(game, health: f32) -> () {
        if let Some(player) = game.player.as_mut() {
            let _ = player.set_health(health.max(0.0), false);
        }
    }

    "dev/horizonlab/javamods/boot/PlayerSync".absorption
    "(F)V" =>
    fn sync_absorption(game, amount: f32) -> () {
        if let Some(player) = game.player.as_mut() {
            let _ = player.set_health(amount.max(0.0), true);
        }
    }

    "dev/horizonlab/javamods/boot/PlayerSync".effect
    "(Lnet/minecraft/world/effect/MobEffectInstance;)V" =>
    fn sync_effect(game, effect: Text) -> () {
        match parse_effect(&effect.0) {
            Some(effect) => {
                game.potions.effects.list.retain(|e| e.id != effect.id);
                game.add_player_effect(effect);
            }
            None => log::warn!("Horizon: a Java mod gave the player {}, an effect Horizon doesn't have", effect.0),
        }
    }

    "dev/horizonlab/javamods/boot/PlayerSync".effectRemoved
    "(Lnet/minecraft/core/Holder;)V" =>
    fn sync_effect_removed(game, effect: Text) -> () {
        if let Some(definition) = horizon_client::brewing::effect(&effect.0) {
            game.potions.effects.list.retain(|e| e.id != definition.id);
            game.apply_effect_status();
        }
    }
}}
