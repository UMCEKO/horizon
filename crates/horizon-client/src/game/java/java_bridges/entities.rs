// game-module-cfg: feature java-mods
//! Entities, on live views: queries answered with ids and groups, and every setter of a backed view written through to native.
#![allow(dead_code)]

use super::super::java_entities::{
    AI, AIR, EQUIPMENT, FIRE, FLAGS, HEALTH, ITEM, MOTION, NAME, POSITION, REMOVAL, RIDING,
    Removal, SPECIES, uuid_bits,
};
use super::*;
use horizon_core::nbt::{Compound, Tag};
use horizon_java_host::bridge::{Ent, EntUuid, Source};
use mod_api::{EntityRef, EquipmentSlot, PathType, RemovalReason};

/// Java's `PathType` constants in declaration order: an ordinal's name.
const PATH_TYPES: [&str; 27] = [
    "BLOCKED",
    "OPEN",
    "WALKABLE",
    "WALKABLE_DOOR",
    "TRAPDOOR",
    "POWDER_SNOW",
    "ON_TOP_OF_POWDER_SNOW",
    "FENCE",
    "LAVA",
    "WATER",
    "WATER_BORDER",
    "RAIL",
    "UNPASSABLE_RAIL",
    "FIRE_IN_NEIGHBOR",
    "FIRE",
    "DAMAGING_IN_NEIGHBOR",
    "DAMAGING",
    "DOOR_OPEN",
    "DOOR_WOOD_CLOSED",
    "DOOR_IRON_CLOSED",
    "BREACH",
    "LEAVES",
    "STICKY_HONEY",
    "COCOA",
    "DAMAGE_CAUTIOUS",
    "ON_TOP_OF_TRAPDOOR",
    "BIG_MOBS_CLOSE_TO_DANGER",
];

/// Java's `EquipmentSlot` by ordinal: `MAINHAND, OFFHAND, FEET, LEGS, CHEST, HEAD, BODY, SADDLE`.
fn equipment_slot(ordinal: i32) -> Option<EquipmentSlot> {
    Some(match ordinal {
        0 => EquipmentSlot::MainHand,
        1 => EquipmentSlot::OffHand,
        2 => EquipmentSlot::Feet,
        3 => EquipmentSlot::Legs,
        4 => EquipmentSlot::Chest,
        5 => EquipmentSlot::Head,
        6 => EquipmentSlot::Body,
        7 => EquipmentSlot::Saddle,
        _ => return None,
    })
}

fn handle(game: &Game<'static>, id: Ent) -> Option<EntityRef> {
    game.entity(id.0)
}

fn after(game: &mut Game<'static>, id: Ent, groups: u32) {
    game.java_refresh_entity(id.0, groups);
}

/// `Entity.remove(reason)`: native removes for a kill or a discard; the unloading reasons are the game's own.
fn remove(game: &mut Game<'static>, id: Ent, why: Removal) {
    let reason = match why {
        Removal::Killed => RemovalReason::Killed,
        Removal::Discarded => RemovalReason::Discarded,
        Removal::UnloadedToChunk | Removal::UnloadedWithPlayer | Removal::ChangedDimension => {
            log::warn!(
                "Horizon: a Java mod removed entity {} for {why:?}, which only the game does; it is discarded",
                id.0
            );
            RemovalReason::Discarded
        }
    };
    if let Some(e) = handle(game, id) {
        game.entity_remove(e, reason);
    }
}

fn kill(game: &mut Game<'static>, id: Ent) {
    if let Some(t) = game.java_target(id.0) {
        game.kill_target(t);
    }
    after(game, id, HEALTH | REMOVAL);
}

fn hurt(game: &mut Game<'static>, id: Ent, source: &Source, amount: f32) -> bool {
    let Some(e) = handle(game, id) else {
        return false;
    };
    let direct = (source.direct > 0)
        .then(|| game.entity(source.direct as u32))
        .flatten();
    let causing = (source.causing > 0)
        .then(|| game.entity(source.causing as u32))
        .flatten();
    let kind = if source.kind.is_empty() {
        "minecraft:generic"
    } else {
        &source.kind
    };
    let hurt = game
        .entity_hurt(e, kind, amount, direct, causing)
        .unwrap_or(false);
    after(game, id, HEALTH | REMOVAL);
    hurt
}

fn motion(game: &mut Game<'static>, id: Ent, v: [f64; 3], add: bool) {
    let Some(e) = handle(game, id) else { return };
    let v = if add {
        let now = game.entity_velocity(e).unwrap_or([0.0; 3]);
        [now[0] + v[0], now[1] + v[1], now[2] + v[2]]
    } else {
        v
    };
    if game.entity_set_velocity(e, v) {
        after(game, id, MOTION);
    }
}

fn place(game: &mut Game<'static>, id: Ent, at: [f64; 3], facing: Option<[f32; 2]>) {
    if game.java_set_position(id.0, at, facing) {
        after(game, id, POSITION);
    }
}

fn teleport(game: &mut Game<'static>, id: Ent, at: [f64; 3], facing: Option<[f32; 2]>) {
    if handle(game, id).is_some_and(|e| game.entity_teleport_to(e, at, facing)) {
        after(game, id, POSITION | RIDING);
    }
}

fn fire(game: &mut Game<'static>, id: Ent, ticks: i32, raise_only: bool) {
    let Some(e) = handle(game, id) else { return };
    if raise_only && game.entity_fire_ticks(e).is_some_and(|now| now >= ticks) {
        return;
    }
    if game.entity_set_remaining_fire_ticks(e, ticks) {
        after(game, id, FIRE);
    }
}

fn rotate(game: &mut Game<'static>, id: Ent, yaw: Option<f32>, pitch: Option<f32>) {
    let Some(e) = handle(game, id) else { return };
    let now = game.entity_rotation(e).unwrap_or([0.0; 2]);
    if game.entity_set_rotation(e, yaw.unwrap_or(now[0]), pitch.unwrap_or(now[1])) {
        after(game, id, POSITION);
    }
}

fn patch(game: &mut Game<'static>, id: Ent, groups: u32, f: impl FnOnce(&mut Compound)) -> bool {
    let done = game.java_patch(id.0, f);
    if done {
        after(game, id, groups);
    }
    done
}

fn stop_riding(game: &mut Game<'static>, id: Ent) {
    let Some(e) = handle(game, id) else { return };
    let vehicle = game.entity_vehicle(e);
    if game.entity_stop_riding(e) {
        after(game, id, RIDING | POSITION);
        if let Some(vehicle) = vehicle {
            after(game, Ent(vehicle.id()), RIDING);
        }
    }
}

/// `Entity.thunderHit`: one more fire tick, eight seconds of fire on an entity not burning, then the bolt's damage.
fn thunder_hit(game: &mut Game<'static>, id: Ent, bolt: &[u8]) {
    let (Some((damage, _)), Some(e)) = (Game::java_bolt_hit(bolt), handle(game, id)) else {
        return;
    };
    let ticks = game.entity_fire_ticks(e).unwrap_or(0) + 1;
    game.entity_set_remaining_fire_ticks(e, if ticks == 0 { 160 } else { ticks });
    game.entity_hurt(e, "minecraft:lightning_bolt", damage, None, None);
    after(game, id, HEALTH | FIRE | REMOVAL | FLAGS | SPECIES);
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
        thunder_hit(game, id, &bolt.0);
    }

    "net/minecraft/world/entity/Entity".discard
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_discard(game, id: Ent) -> () {
        remove(game, id, Removal::Discarded);
    }

    "net/minecraft/world/entity/Entity".remove
    "(Lnet/minecraft/world/entity/Entity$RemovalReason;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_remove(game, id: Ent, reason: i32) -> () {
        remove(game, id, Removal::of_ordinal(reason));
    }

    "net/minecraft/world/entity/LivingEntity".remove
    "(Lnet/minecraft/world/entity/Entity$RemovalReason;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_remove(game, id: Ent, reason: i32) -> () {
        remove(game, id, Removal::of_ordinal(reason));
    }

    "net/minecraft/world/entity/Entity".setRemoved
    "(Lnet/minecraft/world/entity/Entity$RemovalReason;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_removed(game, id: Ent, reason: i32) -> () {
        remove(game, id, Removal::of_ordinal(reason));
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

    "net/minecraft/world/entity/vehicle/VehicleEntity".hurtServer
    "(Lnet/minecraft/server/level/ServerLevel;Lnet/minecraft/world/damagesource/DamageSource;F)Z" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn vehicle_hurt_server(game, id: Ent, _level: Skip, source: Source, amount: f32) -> bool {
        let vehicle = matches!(game.java_target(id.0), Some(commands::Target::Boat(_) | commands::Target::Cart(_)));
        hurt(game, id, &source, amount);
        // `VehicleEntity.hurtServer` answers true for any hit a vehicle takes.
        vehicle
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
        if handle(game, id).is_some_and(|e| game.entity_push(e, [x, y, z])) {
            after(game, id, MOTION);
        }
    }

    "net/minecraft/world/entity/Entity".move
    "(Lnet/minecraft/world/entity/MoverType;Lnet/minecraft/world/phys/Vec3;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_move(game, id: Ent, mover: i32, delta: Point) -> () {
        let Some(target) = game.java_target(id.0) else { return };
        let mover = match mover {
            0 => horizon_world::entity_move::MoverType::SelfMovement,
            1 => horizon_world::entity_move::MoverType::Player,
            2 => horizon_world::entity_move::MoverType::Piston,
            3 => horizon_world::entity_move::MoverType::ShulkerBox,
            _ => horizon_world::entity_move::MoverType::Shulker,
        };
        if game.entity_move(target, mover, Vec3::new(delta.0[0], delta.0[1], delta.0[2])) {
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
        teleport(game, id, [x, y, z], None);
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
        if handle(game, id).is_some_and(|e| game.entity_set_head_yaw(e, yaw)) {
            after(game, id, POSITION);
        }
    }

    "net/minecraft/world/entity/LivingEntity".setYHeadRot
    "(F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_set_head_yaw(game, id: Ent, yaw: f32) -> () {
        if handle(game, id).is_some_and(|e| game.entity_set_head_yaw(e, yaw)) {
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
        if let Some(at) = handle(game, id).and_then(|e| game.entity_position(e)) {
            game.java_set_position(id.0, at, None);
        }
    }

    "net/minecraft/world/entity/Entity".setOldPos
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_old_pos(game, id: Ent) -> () {
        if let Some(at) = handle(game, id).and_then(|e| game.entity_position(e)) {
            game.java_set_position(id.0, at, None);
        }
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
        if handle(game, id).and_then(|e| game.living(e)).is_some_and(|l| game.living_set_health(l, health)) {
            after(game, id, HEALTH);
        }
    }

    "net/minecraft/world/entity/LivingEntity".setItemSlot
    "(Lnet/minecraft/world/entity/EquipmentSlot;Lnet/minecraft/world/item/ItemStack;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_set_item_slot(game, id: Ent, slot: i32, stack: Bytes) -> () {
        let stack = java_mods::read_tag(&stack.0).and_then(|t| horizon_client::save::item_from_nbt(&t));
        let (Some(slot), Some(living)) = (equipment_slot(slot), handle(game, id).and_then(|e| game.living(e))) else {
            return;
        };
        if game.living_set_equipment(living, slot, stack) {
            after(game, id, EQUIPMENT);
        }
    }

    "net/minecraft/world/entity/item/ItemEntity".setItem
    "(Lnet/minecraft/world/item/ItemStack;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_set_item(game, id: Ent, stack: Bytes) -> () {
        let stack = java_mods::read_tag(&stack.0).and_then(|t| horizon_client::save::item_from_nbt(&t));
        if let (Some(stack), Some(item)) = (stack, handle(game, id).and_then(|e| game.item_entity(e)))
            && game.item_entity_set_stack(item, stack)
        {
            after(game, id, ITEM);
        }
    }

    "net/minecraft/world/entity/item/ItemEntity".setPickUpDelay
    "(I)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_set_pickup_delay(game, id: Ent, ticks: i32) -> () {
        if handle(game, id).and_then(|e| game.item_entity(e)).is_some_and(|i| game.item_entity_set_pickup_delay(i, ticks)) {
            after(game, id, ITEM);
        }
    }

    "net/minecraft/world/entity/item/ItemEntity".setNoPickUpDelay
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_set_no_pickup_delay(game, id: Ent) -> () {
        if handle(game, id).and_then(|e| game.item_entity(e)).is_some_and(|i| game.item_entity_set_no_pickup_delay(i)) {
            after(game, id, ITEM);
        }
    }

    "net/minecraft/world/entity/item/ItemEntity".setDefaultPickUpDelay
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_set_default_pickup_delay(game, id: Ent) -> () {
        if handle(game, id).and_then(|e| game.item_entity(e)).is_some_and(|i| game.item_entity_set_default_pickup_delay(i)) {
            after(game, id, ITEM);
        }
    }

    "net/minecraft/world/entity/item/ItemEntity".makeFakeItem
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_make_fake(game, id: Ent) -> () {
        patch(game, id, ITEM, |map| {
            map.insert("PickupDelay".into(), Tag::Short(32767));
            map.insert("Age".into(), Tag::Short(5999));
        });
    }

    "net/minecraft/world/entity/item/ItemEntity".setTarget
    "(Ljava/util/UUID;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn item_set_target(game, id: Ent, target: Text) -> () {
        let target = Game::java_uuid_tag(&target.0).and_then(|t| match t {
            Tag::IntArray(u) => <[i32; 4]>::try_from(u).ok().map(uuid_bits),
            _ => None,
        });
        if handle(game, id).and_then(|e| game.item_entity(e)).is_some_and(|i| game.item_entity_set_target(i, target)) {
            after(game, id, ITEM);
        }
    }

    "net/minecraft/world/entity/Entity".startRiding
    "(Lnet/minecraft/world/entity/Entity;ZZ)Z" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_start_riding(game, id: Ent, vehicle: Ent, force: bool, _events: bool) -> bool {
        let (Some(rider), Some(ridden)) = (handle(game, id), handle(game, vehicle)) else {
            return false;
        };
        let rode = game.entity_start_riding(rider, ridden, force);
        if rode {
            after(game, id, RIDING | POSITION);
            after(game, vehicle, RIDING);
        }
        rode
    }

    "net/minecraft/world/entity/Entity".stopRiding
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_stop_riding(game, id: Ent) -> () {
        stop_riding(game, id);
    }

    "net/minecraft/world/entity/LivingEntity".stopRiding
    "()V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_stop_riding(game, id: Ent) -> () {
        stop_riding(game, id);
    }

    "net/minecraft/world/entity/projectile/Projectile".setOwner
    "(Lnet/minecraft/world/entity/Entity;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn projectile_set_owner(game, id: Ent, owner: EntUuid) -> () {
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
        if let Some(p) = handle(game, id).and_then(|e| game.projectile(e)) {
            game.projectile_set_base_damage(p, damage);
        }
        after(game, id, SPECIES);
    }

    "net/minecraft/world/entity/Mob".setPathfindingMalus
    "(Lnet/minecraft/world/level/pathfinder/PathType;F)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn mob_set_pathfinding_malus(game, id: Ent, path_type: i32, cost: f32) -> () {
        let kind = PATH_TYPES.get(path_type as usize).map(|name| PathType::from_name(name));
        if let (Some(kind), Some(mob)) = (kind, handle(game, id).and_then(|e| game.mob(e))) {
            game.mob_set_pathfinding_malus(mob, kind, cost);
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
        if let Some(e) = handle(game, id) {
            game.entity_set_air_supply(e, air);
        }
        after(game, id, AIR);
    }

    "net/minecraft/world/entity/LivingEntity".setNoActionTime
    "(I)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn living_set_no_action_time(game, id: Ent, ticks: i32) -> () {
        if let Some(mob) = handle(game, id).and_then(|e| game.mob(e)) {
            game.mob_set_no_action_time(mob, ticks);
        }
    }

    "net/minecraft/world/entity/Entity".setSilent
    "(Z)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_silent(game, id: Ent, silent: bool) -> () {
        if handle(game, id).is_some_and(|e| game.entity_set_silent(e, silent)) {
            after(game, id, FLAGS);
        }
    }

    "net/minecraft/world/entity/animal/sheep/Sheep".setColor
    "(Lnet/minecraft/world/item/DyeColor;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn sheep_set_color(game, id: Ent, color: i32) -> () {
        patch(game, id, FLAGS, |map| {
            map.insert("Color".into(), Tag::Byte(color as i8));
        });
    }

    "net/minecraft/world/entity/Entity".setCustomName
    "(Lnet/minecraft/network/chat/Component;)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_custom_name(game, id: Ent, name: Bytes) -> () {
        let name = java_mods::read_tag(&name.0).and_then(|t| t.get("text").cloned());
        patch(game, id, NAME, |map| match name {
            Some(name) => {
                map.insert("CustomName".into(), name);
            }
            None => {
                map.remove(&"CustomName".into());
            }
        });
    }

    "net/minecraft/world/entity/Entity".setCustomNameVisible
    "(Z)V" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_set_custom_name_visible(game, id: Ent, visible: bool) -> () {
        if handle(game, id).is_some_and(|e| game.entity_set_custom_name_visible(e, visible)) {
            after(game, id, NAME);
        }
    }

    "net/minecraft/world/entity/Entity".addTag
    "(Ljava/lang/String;)Z" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_add_tag(game, id: Ent, tag: Text) -> bool {
        let added = handle(game, id).is_some_and(|e| game.entity_add_tag(e, &tag.0));
        if added {
            after(game, id, NAME);
        }
        added
    }

    "net/minecraft/world/entity/Entity".removeTag
    "(Ljava/lang/String;)Z" if "@dev/horizonlab/javamods/boot/EntityViews.backed" =>
    fn entity_remove_tag(game, id: Ent, tag: Text) -> bool {
        let removed = handle(game, id).is_some_and(|e| game.entity_remove_tag(e, &tag.0));
        if removed {
            after(game, id, NAME);
        }
        removed
    }
}}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_java_path_type_ordinal_names_the_same_kind() {
        assert_eq!(PathType::from_name(PATH_TYPES[0]), PathType::Blocked);
        assert_eq!(PathType::from_name(PATH_TYPES[9]), PathType::Water);
        assert_eq!(
            PathType::from_name(PATH_TYPES[26]),
            PathType::BigMobsCloseToDanger
        );
        assert!(
            PATH_TYPES[1..]
                .iter()
                .all(|n| PathType::from_name(n) != PathType::Blocked)
        );
    }
}
