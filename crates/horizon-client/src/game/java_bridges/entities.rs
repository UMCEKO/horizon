//! Entities, on live views: queries answered with ids and groups, and every setter of a backed view written through to native.

use super::super::entity_life::Removal;
use super::super::java_entities::{
    AI, AIR, EQUIPMENT, FIRE, FLAGS, HEALTH, ITEM, MOTION, NAME, POSITION, REMOVAL, RIDING, SPECIES,
};
use super::*;
use horizon_client::projectiles::ProjectileKind;
use horizon_java_host::bridge::Ent;
use horizon_world::nbt::Tag;

fn removal_of(ordinal: i32) -> Removal {
    match ordinal {
        0 => Removal::Killed,
        1 => Removal::Discarded,
        2 => Removal::UnloadedToChunk,
        3 => Removal::UnloadedWithPlayer,
        _ => Removal::ChangedDimension,
    }
}

fn after(game: &mut Game<'static>, id: Ent, groups: u32) {
    game.java_refresh_entity(id.0, groups);
}

fn remove(game: &mut Game<'static>, id: Ent, why: Removal) {
    game.java_discard(id.0, why);
}

fn kill(game: &mut Game<'static>, id: Ent) {
    game.java_kill(id.0);
    after(game, id, HEALTH | REMOVAL);
}

fn hurt(game: &mut Game<'static>, id: Ent, source: &Source, amount: f32) -> bool {
    let hurt = game.java_hurt_target(id.0, source, amount);
    after(game, id, HEALTH | REMOVAL);
    hurt
}

fn motion(game: &mut Game<'static>, id: Ent, v: [f64; 3], add: bool) {
    if game.java_set_motion(id.0, v, add) {
        after(game, id, MOTION);
    }
}

fn place(game: &mut Game<'static>, id: Ent, at: [f64; 3], facing: Option<[f32; 2]>) {
    if game.java_set_position(id.0, at, facing) {
        after(game, id, POSITION);
    }
}

fn fire(game: &mut Game<'static>, id: Ent, ticks: i32, raise_only: bool) {
    let patched = game.java_patch(id.0, |map| {
        let now = horizon_client::save::number(map.get(&"Fire".into())).unwrap_or(0) as i32;
        if !raise_only || now < ticks {
            map.insert("Fire".into(), Tag::Short(ticks as i16));
        }
    });
    if patched {
        after(game, id, FIRE);
    }
}

fn rotate(game: &mut Game<'static>, id: Ent, yaw: Option<f32>, pitch: Option<f32>) {
    let Some(at) = game.java_target(id.0).and_then(|t| game.target_position(t)) else {
        return;
    };
    let now = game
        .java_target(id.0)
        .map_or([0.0; 2], |t| game.target_rotation(t));
    let facing = [yaw.unwrap_or(now[0]), pitch.unwrap_or(now[1])];
    place(game, id, at, Some(facing));
}

fn patch(
    game: &mut Game<'static>,
    id: Ent,
    groups: u32,
    f: impl FnOnce(&mut horizon_world::nbt::Compound),
) -> bool {
    let done = game.java_patch(id.0, f);
    if done {
        after(game, id, groups);
    }
    done
}

fn set_number(map: &mut horizon_world::nbt::Compound, key: &str, tag: Tag) {
    map.insert(key.into(), tag);
}

fn tags_of(map: &horizon_world::nbt::Compound) -> Vec<Tag> {
    match map.get(&"Tags".into()) {
        Some(Tag::List(list)) => list.clone(),
        _ => Vec::new(),
    }
}

fn slot_name(slot: i32) -> &'static str {
    [
        "mainhand", "offhand", "feet", "legs", "chest", "head", "body", "saddle",
    ][slot.clamp(0, 7) as usize]
}

horizon_java_host::java_bridges! { table for Game<'static> {
    "dev/horizonlab/javamods/boot/EntityViews".query
    "(DDDDDD)[B" =>
    fn entities_query(game, x1: f64, y1: f64, z1: f64, x2: f64, y2: f64, z2: f64) -> Bytes {
        Bytes(game.java_entities_in([x1, y1, z1], [x2, y2, z2]))
    }

    "dev/horizonlab/javamods/boot/EntityViews".queryId
    "(I)[B" =>
    fn entities_query_id(game, id: i32) -> Bytes {
        Bytes(game.java_entity_by_id(id as u32))
    }

    "dev/horizonlab/javamods/boot/EntityViews".spawn
    "([B[B)I" =>
    fn entities_spawn(game, saved: Bytes, species: Bytes) -> i32 {
        game.java_spawn_entity(&saved.0, &species.0) as i32
    }

    "net/minecraft/world/entity/item/PrimedTnt".setFuse
    "(I)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn tnt_set_fuse(game, id: Ent, fuse: i32) -> () {
        if let Some(t) = game.tnts.iter_mut().find(|t| t.id == id.0) {
            t.fuse = fuse;
        }
        after(game, id, SPECIES);
    }

    "net/minecraft/world/entity/item/PrimedTnt".setBlockState
    "(Lnet/minecraft/world/level/block/state/BlockState;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn tnt_set_block_state(game, id: Ent, state: State) -> () {
        if let (Some(t), Some(state)) = (game.tnts.iter_mut().find(|t| t.id == id.0), state.0) {
            t.block_state = Some(state);
        }
        after(game, id, SPECIES);
    }

    "net/minecraft/world/entity/LightningBolt".setVisualOnly
    "(Z)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn bolt_set_visual_only(game, id: Ent, visual_only: bool) -> () {
        if let Some(b) = game.bolts.iter_mut().find(|b| b.id == id.0) {
            b.visual_only = visual_only;
        }
        after(game, id, SPECIES);
    }

    "net/minecraft/world/entity/LightningBolt".setDamage
    "(F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn bolt_set_damage(game, id: Ent, damage: f32) -> () {
        if let Some(b) = game.bolts.iter_mut().find(|b| b.id == id.0) {
            b.damage = damage;
        }
        after(game, id, SPECIES);
    }

    "net/minecraft/world/entity/Entity".thunderHit
    "(Lnet/minecraft/server/level/ServerLevel;Lnet/minecraft/world/entity/LightningBolt;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_thunder_hit(game, id: Ent, _level: Skip, bolt: Bytes) -> () {
        if let (Some(hit), Some(t)) = (Game::java_bolt_hit(&bolt.0), game.java_target(id.0)) {
            game.thunder_hit_entity(t, &hit, &mut Vec::new());
        }
        after(game, id, HEALTH | FIRE | REMOVAL | FLAGS | SPECIES);
    }

    "net/minecraft/world/entity/Entity".discard
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_discard(game, id: Ent) -> () {
        remove(game, id, Removal::Discarded);
    }

    "net/minecraft/world/entity/Entity".remove
    "(Lnet/minecraft/world/entity/Entity$RemovalReason;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_remove(game, id: Ent, reason: i32) -> () {
        remove(game, id, removal_of(reason));
    }

    "net/minecraft/world/entity/LivingEntity".remove
    "(Lnet/minecraft/world/entity/Entity$RemovalReason;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_remove(game, id: Ent, reason: i32) -> () {
        remove(game, id, removal_of(reason));
    }

    "net/minecraft/world/entity/Entity".setRemoved
    "(Lnet/minecraft/world/entity/Entity$RemovalReason;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_removed(game, id: Ent, reason: i32) -> () {
        remove(game, id, removal_of(reason));
    }

    "net/minecraft/world/entity/Entity".kill
    "(Lnet/minecraft/server/level/ServerLevel;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_kill(game, id: Ent, _level: Skip) -> () {
        kill(game, id);
    }

    "net/minecraft/world/entity/LivingEntity".kill
    "(Lnet/minecraft/server/level/ServerLevel;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_kill(game, id: Ent, _level: Skip) -> () {
        kill(game, id);
    }

    "net/minecraft/world/entity/Entity".hurt
    "(Lnet/minecraft/world/damagesource/DamageSource;F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_hurt(game, id: Ent, source: Source, amount: f32) -> () {
        hurt(game, id, &source, amount);
    }

    "net/minecraft/world/entity/LivingEntity".hurtServer
    "(Lnet/minecraft/server/level/ServerLevel;Lnet/minecraft/world/damagesource/DamageSource;F)Z" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_hurt_server(game, id: Ent, _level: Skip, source: Source, amount: f32) -> bool {
        hurt(game, id, &source, amount)
    }

    "net/minecraft/world/entity/item/ItemEntity".hurtServer
    "(Lnet/minecraft/server/level/ServerLevel;Lnet/minecraft/world/damagesource/DamageSource;F)Z" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_hurt_server(game, id: Ent, _level: Skip, source: Source, amount: f32) -> bool {
        hurt(game, id, &source, amount)
    }

    "net/minecraft/world/entity/Entity".setDeltaMovement
    "(Lnet/minecraft/world/phys/Vec3;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_motion(game, id: Ent, v: Point) -> () {
        motion(game, id, v.0, false);
    }

    "net/minecraft/world/entity/Entity".setDeltaMovement
    "(DDD)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_motion_xyz(game, id: Ent, x: f64, y: f64, z: f64) -> () {
        motion(game, id, [x, y, z], false);
    }

    "net/minecraft/world/entity/Entity".addDeltaMovement
    "(Lnet/minecraft/world/phys/Vec3;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_add_motion(game, id: Ent, v: Point) -> () {
        motion(game, id, v.0, true);
    }

    "net/minecraft/world/entity/Entity".push
    "(DDD)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_push(game, id: Ent, x: f64, y: f64, z: f64) -> () {
        motion(game, id, [x, y, z], true);
    }

    "net/minecraft/world/entity/Entity".move
    "(Lnet/minecraft/world/entity/MoverType;Lnet/minecraft/world/phys/Vec3;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_move(game, id: Ent, mover: i32, delta: Point) -> () {
        if game.java_move(id.0, mover, delta.0) {
            after(game, id, POSITION | MOTION | FLAGS);
        }
    }

    "net/minecraft/world/entity/Entity".setPos
    "(DDD)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_pos(game, id: Ent, x: f64, y: f64, z: f64) -> () {
        place(game, id, [x, y, z], None);
    }

    "net/minecraft/world/entity/Entity".setPos
    "(Lnet/minecraft/world/phys/Vec3;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_pos_vec(game, id: Ent, at: Point) -> () {
        place(game, id, at.0, None);
    }

    "net/minecraft/world/entity/Entity".teleportTo
    "(DDD)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_teleport_to(game, id: Ent, x: f64, y: f64, z: f64) -> () {
        place(game, id, [x, y, z], None);
    }

    "net/minecraft/world/entity/Entity".snapTo
    "(DDDFF)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_snap_to(game, id: Ent, x: f64, y: f64, z: f64, yaw: f32, pitch: f32) -> () {
        place(game, id, [x, y, z], Some([yaw, pitch]));
    }

    "net/minecraft/world/entity/Entity".absSnapTo
    "(DDD)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_abs_snap_to(game, id: Ent, x: f64, y: f64, z: f64) -> () {
        place(game, id, [x, y, z], None);
    }

    "net/minecraft/world/entity/Entity".setYRot
    "(F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_yaw(game, id: Ent, yaw: f32) -> () {
        rotate(game, id, Some(yaw), None);
    }

    "net/minecraft/world/entity/Entity".setXRot
    "(F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_pitch(game, id: Ent, pitch: f32) -> () {
        rotate(game, id, None, Some(pitch));
    }

    "net/minecraft/world/entity/Entity".setYHeadRot
    "(F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_head_yaw(game, id: Ent, yaw: f32) -> () {
        if game.java_set_head(id.0, Some(yaw), None) {
            after(game, id, POSITION);
        }
    }

    "net/minecraft/world/entity/LivingEntity".setYHeadRot
    "(F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_set_head_yaw(game, id: Ent, yaw: f32) -> () {
        if game.java_set_head(id.0, Some(yaw), None) {
            after(game, id, POSITION);
        }
    }

    "net/minecraft/world/entity/Entity".setYBodyRot
    "(F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_body_yaw(game, id: Ent, yaw: f32) -> () {
        if game.java_set_head(id.0, None, Some(yaw)) {
            after(game, id, POSITION);
        }
    }

    "net/minecraft/world/entity/LivingEntity".setYBodyRot
    "(F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_set_body_yaw(game, id: Ent, yaw: f32) -> () {
        if game.java_set_head(id.0, None, Some(yaw)) {
            after(game, id, POSITION);
        }
    }

    "net/minecraft/world/entity/Entity".setOldPosAndRot
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_old_pos_and_rot(game, id: Ent) -> () {
        game.java_set_old_position(id.0);
    }

    "net/minecraft/world/entity/Entity".setOldPos
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_old_pos(game, id: Ent) -> () {
        game.java_set_old_position(id.0);
    }

    "net/minecraft/world/entity/Entity".setId
    "(I)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_id(_game, _id: Ent, _new_id: i32) -> () {
        // Horizon's id is the identity of a backed view.
    }

    "net/minecraft/world/entity/Entity".teleport
    "(Lnet/minecraft/world/level/portal/TeleportTransition;)Lnet/minecraft/world/entity/Entity;" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_teleport(game, id: Ent, transition: Bytes) -> i32 {
        let moved = game.java_teleport(id.0, &transition.0);
        if moved {
            after(game, id, POSITION | MOTION);
        }
        if moved { id.0 as i32 } else { 0 }
    }

    "net/minecraft/world/entity/item/ItemEntity".teleport
    "(Lnet/minecraft/world/level/portal/TeleportTransition;)Lnet/minecraft/world/entity/Entity;" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_teleport(game, id: Ent, transition: Bytes) -> i32 {
        let moved = game.java_teleport(id.0, &transition.0);
        if moved {
            after(game, id, POSITION | MOTION);
        }
        if moved { id.0 as i32 } else { 0 }
    }

    "net/minecraft/world/entity/LivingEntity".knockback
    "(DDDLnet/minecraft/world/damagesource/DamageSource;F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_knockback(game, id: Ent, power: f64, x: f64, z: f64, _source: Skip, _damage: f32) -> () {
        if game.java_knockback(id.0, power, x, z) {
            after(game, id, MOTION);
        }
    }

    "net/minecraft/world/entity/LivingEntity".knockback
    "(DDDLnet/minecraft/world/damagesource/DamageSource;FZ)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_knockback_effect(game, id: Ent, power: f64, x: f64, z: f64, _source: Skip, _damage: f32, _effect: bool) -> () {
        if game.java_knockback(id.0, power, x, z) {
            after(game, id, MOTION);
        }
    }

    "net/minecraft/world/entity/Entity".igniteForSeconds
    "(F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_ignite_seconds(game, id: Ent, seconds: f32) -> () {
        fire(game, id, (seconds * 20.0).floor() as i32, true);
    }

    "net/minecraft/world/entity/Entity".igniteForTicks
    "(I)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_ignite_ticks(game, id: Ent, ticks: i32) -> () {
        fire(game, id, ticks, true);
    }

    "net/minecraft/world/entity/LivingEntity".igniteForTicks
    "(I)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_ignite_ticks(game, id: Ent, ticks: i32) -> () {
        fire(game, id, ticks, true);
    }

    "net/minecraft/world/entity/Entity".setRemainingFireTicks
    "(I)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_fire(game, id: Ent, ticks: i32) -> () {
        fire(game, id, ticks, false);
    }

    "net/minecraft/world/entity/LivingEntity".setHealth
    "(F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_set_health(game, id: Ent, health: f32) -> () {
        if game.java_set_health(id.0, health) {
            after(game, id, HEALTH);
        }
    }

    "net/minecraft/world/entity/LivingEntity".setItemSlot
    "(Lnet/minecraft/world/entity/EquipmentSlot;Lnet/minecraft/world/item/ItemStack;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_set_item_slot(game, id: Ent, slot: i32, stack: Bytes) -> () {
        let name = slot_name(slot);
        let stack = java_mods::read_tag(&stack.0);
        patch(game, id, EQUIPMENT, |map| {
            let mut equipment = match map.get(&"equipment".into()) {
                Some(Tag::Compound(e)) => e.clone(),
                _ => Default::default(),
            };
            match stack {
                Some(stack) => equipment.insert(name.into(), stack),
                None => equipment.remove(&name.into()),
            };
            set_number(map, "equipment", Tag::Compound(equipment));
        });
    }

    "net/minecraft/world/entity/item/ItemEntity".setItem
    "(Lnet/minecraft/world/item/ItemStack;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_set_item(game, id: Ent, stack: Bytes) -> () {
        if let Some(stack) = java_mods::read_tag(&stack.0) {
            patch(game, id, ITEM, |map| set_number(map, "Item", stack));
        }
    }

    "net/minecraft/world/entity/item/ItemEntity".setPickUpDelay
    "(I)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_set_pickup_delay(game, id: Ent, ticks: i32) -> () {
        patch(game, id, ITEM, |map| set_number(map, "PickupDelay", Tag::Short(ticks as i16)));
    }

    "net/minecraft/world/entity/item/ItemEntity".setNoPickUpDelay
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_set_no_pickup_delay(game, id: Ent) -> () {
        patch(game, id, ITEM, |map| set_number(map, "PickupDelay", Tag::Short(0)));
    }

    "net/minecraft/world/entity/item/ItemEntity".setDefaultPickUpDelay
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_set_default_pickup_delay(game, id: Ent) -> () {
        patch(game, id, ITEM, |map| set_number(map, "PickupDelay", Tag::Short(10)));
    }

    "net/minecraft/world/entity/item/ItemEntity".makeFakeItem
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_make_fake(game, id: Ent) -> () {
        patch(game, id, ITEM, |map| {
            set_number(map, "PickupDelay", Tag::Short(32767));
            set_number(map, "Age", Tag::Short(5999));
        });
    }

    "net/minecraft/world/entity/item/ItemEntity".setTarget
    "(Ljava/util/UUID;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_set_target(game, id: Ent, target: Text) -> () {
        patch(game, id, ITEM, |map| match Game::java_uuid_tag(&target.0) {
            Some(uuid) => set_number(map, "Owner", uuid),
            None => {
                map.remove(&"Owner".into());
            }
        });
    }

    "net/minecraft/world/entity/Entity".startRiding
    "(Lnet/minecraft/world/entity/Entity;ZZ)Z" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_start_riding(game, id: Ent, vehicle: Ent, force: bool, events: bool) -> bool {
        let rode = game.java_ride(id.0, vehicle.0, force, events);
        if rode {
            after(game, id, RIDING | POSITION);
            after(game, vehicle, RIDING);
        }
        rode
    }

    "net/minecraft/world/entity/Entity".stopRiding
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_stop_riding(game, id: Ent) -> () {
        let vehicle = game.java_vehicle_of(id.0);
        if game.java_dismount(id.0) {
            after(game, id, RIDING | POSITION);
            if let Some(vehicle) = vehicle {
                after(game, Ent(vehicle), RIDING);
            }
        }
    }

    "net/minecraft/world/entity/LivingEntity".stopRiding
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_stop_riding(game, id: Ent) -> () {
        let vehicle = game.java_vehicle_of(id.0);
        if game.java_dismount(id.0) {
            after(game, id, RIDING | POSITION);
            if let Some(vehicle) = vehicle {
                after(game, Ent(vehicle), RIDING);
            }
        }
    }

    "net/minecraft/world/entity/projectile/Projectile".setOwner
    "(Lnet/minecraft/world/entity/Entity;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn projectile_set_owner(game, id: Ent, owner: Ent) -> () {
        if game.java_set_owner(id.0, owner.0) {
            after(game, id, SPECIES);
        }
    }

    "net/minecraft/world/entity/projectile/Projectile".shootFromRotation
    "(Lnet/minecraft/world/entity/Entity;FFFFF)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn projectile_shoot_from_rotation(game, id: Ent, source: Ent, pitch: f32, yaw: f32, offset: f32, speed: f32, uncertainty: f32) -> () {
        if game.java_shoot_from_rotation(id.0, source.0, [pitch, yaw, offset, speed, uncertainty]) {
            after(game, id, MOTION | POSITION);
        }
    }

    "net/minecraft/world/entity/projectile/arrow/AbstractArrow".setBaseDamage
    "(D)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn arrow_set_base_damage(game, id: Ent, damage: f64) -> () {
        if let Some(p) = game.creatures.projectiles.iter_mut().find(|p| p.id == id.0)
            && let ProjectileKind::Arrow { base_damage, .. } = &mut p.kind
        {
            *base_damage = damage;
        }
        after(game, id, SPECIES);
    }

    "net/minecraft/world/entity/Mob".setPathfindingMalus
    "(Lnet/minecraft/world/level/pathfinder/PathType;F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn mob_set_pathfinding_malus(game, id: Ent, path_type: i32, cost: f32) -> () {
        if let (Some(t), Some(m)) = (
            horizon_client::mobs::path::PathType::from_ordinal(path_type as usize),
            game.mobs.iter_mut().find(|m| m.id == id.0),
        ) {
            m.set_malus(t, cost);
        }
        after(game, id, AI);
    }

    "net/minecraft/world/entity/LivingEntity".actuallyHurt
    "(Lnet/minecraft/server/level/ServerLevel;Lnet/minecraft/world/damagesource/DamageSource;F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_actually_hurt(game, id: Ent, _level: Skip, source: Source, amount: f32) -> () {
        if game.java_actually_hurt(id.0, &source, amount) {
            after(game, id, HEALTH);
        }
    }

    "net/minecraft/world/entity/LivingEntity".releaseUsingItem
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_release_using_item(game, id: Ent) -> () {
        if game.java_mob_stop_using(id.0) {
            after(game, id, EQUIPMENT | FLAGS);
        }
    }

    "net/minecraft/world/entity/Entity".setAirSupply
    "(I)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_air_supply(game, id: Ent, air: i32) -> () {
        if let Some(m) = game.mobs.iter_mut().find(|m| m.id == id.0) {
            m.air = air;
        }
        after(game, id, AIR);
    }

    "net/minecraft/world/entity/LivingEntity".setNoActionTime
    "(I)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_set_no_action_time(game, id: Ent, ticks: i32) -> () {
        if let Some(m) = game.mobs.iter_mut().find(|m| m.id == id.0) {
            m.ai.no_action_time = ticks;
        }
    }

    "net/minecraft/world/entity/Entity".setSilent
    "(Z)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_silent(game, id: Ent, silent: bool) -> () {
        patch(game, id, FLAGS, |map| set_number(map, "Silent", Tag::Byte(silent as i8)));
    }

    "net/minecraft/world/entity/animal/sheep/Sheep".setColor
    "(Lnet/minecraft/world/item/DyeColor;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn sheep_set_color(game, id: Ent, color: i32) -> () {
        patch(game, id, FLAGS, |map| set_number(map, "Color", Tag::Byte(color as i8)));
    }

    "net/minecraft/world/entity/Entity".setCustomName
    "(Lnet/minecraft/network/chat/Component;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_custom_name(game, id: Ent, name: Bytes) -> () {
        let name = java_mods::read_tag(&name.0).and_then(|t| t.get("text").cloned());
        patch(game, id, NAME, |map| match name {
            Some(name) => set_number(map, "CustomName", name),
            None => {
                map.remove(&"CustomName".into());
            }
        });
    }

    "net/minecraft/world/entity/Entity".setCustomNameVisible
    "(Z)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_custom_name_visible(game, id: Ent, visible: bool) -> () {
        patch(game, id, NAME, |map| set_number(map, "CustomNameVisible", Tag::Byte(visible as i8)));
    }

    "net/minecraft/world/entity/Entity".addTag
    "(Ljava/lang/String;)Z" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_add_tag(game, id: Ent, tag: Text) -> bool {
        let mut added = false;
        patch(game, id, NAME, |map| {
            let mut tags = tags_of(map);
            let new = Tag::String(tag.0.as_str().into());
            if tags.len() < 1024 && !tags.contains(&new) {
                tags.push(new);
                added = true;
            }
            set_number(map, "Tags", Tag::List(tags));
        });
        added
    }

    "net/minecraft/world/entity/Entity".removeTag
    "(Ljava/lang/String;)Z" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_remove_tag(game, id: Ent, tag: Text) -> bool {
        let mut removed = false;
        patch(game, id, NAME, |map| {
            let mut tags = tags_of(map);
            let old = Tag::String(tag.0.as_str().into());
            if let Some(i) = tags.iter().position(|t| *t == old) {
                tags.remove(i);
                removed = true;
            }
            set_number(map, "Tags", Tag::List(tags));
        });
        removed
    }
}}
