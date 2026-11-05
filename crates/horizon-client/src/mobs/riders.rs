//! Mobs riding mobs (`Entity.getPassengers`): where a rider sits on its
//! vehicle (`EntityAttachments`), the rider steering the vehicle toward its
//! target (`Mob.getControllingPassenger`, whose navigation and move control
//! the rider's goals use), and the saved jockey fields. The game links the
//! two (`game/passengers.rs`) and spawns the jockeys.
use super::{
    Mob, MobWorld, Seen, Species, attachments,
    goals::{JUMP, LOOK, MOVE},
};
use horizon_core::geometry::{Aabb, Vec3, trig};
use horizon_world::nbt::{Compound, Tag};

/// The goal flags a controlling passenger takes from its vehicle.
const CONTROLLED: u8 = MOVE | JUMP | LOOK;
/// `Entity.removePassenger`: a passenger that got off cannot board for 60 ticks.
const BOARDING_COOLDOWN: i32 = 60;
/// `Piglin.startRiding`: how many baby piglins stack on one hoglin.
const MAX_PASSENGERS_ON_ONE_HOGLIN: i32 = 3;

/// `getPassengerAttachmentPoint` of a mob vehicle for its passenger number
/// `index`, turned by `yaw`: the kind's override, else `getClamped(PASSENGER,
/// index, yaw)` of the box's attachments. `client` is set where the level is
/// the client's (a `Strider` bobs its rider only there).
pub fn passenger_attachment(v: &Mob, index: usize, yaw: f32, client: bool) -> [f64; 3] {
    let species = v.species();
    if super::camel::is_camel(species) {
        return rotated(super::camel::passenger_attachment(v), yaw);
    }
    if matches!(
        species,
        Species::Slime | Species::MagmaCube | Species::SulfurCube
    ) {
        // AbstractCubeMob: a hair below the top of its box.
        return [
            0.0,
            f64::from(v.height()) - 0.015625 * f64::from(v.extra.size),
            0.0,
        ];
    }
    let [x, y, z] = attachments::of(v).passenger_point(index, yaw);
    // AbstractHorse: a rearing horse slides it.
    let rear = rotated(super::equine::rear_offset(v), yaw);
    let mut at = [x + rear[0], y + rear[1], z + rear[2]];
    if client && species == Species::Strider {
        let speed = v.limb_speed.min(0.25);
        let offset = 0.12 * trig::cos(f64::from(v.limb_position * 1.5)) * 2.0 * speed;
        let scale = if v.is_baby() { 0.5_f32 } else { 1.0 };
        at[1] += f64::from(offset * scale);
    }
    at
}
/// `getVehicleAttachmentPoint` of a rider that sits on a vehicle `vehicle_width`
/// wide: how far below the vehicle's attachment point its feet hang.
pub fn vehicle_attachment(r: &Mob, vehicle_width: f32) -> [f64; 3] {
    // Spider and CaveSpider: a vehicle no wider than they are carries them lower.
    let low = match r.species() {
        Species::Spider => Some(0.3125),
        Species::CaveSpider => Some(0.21875),
        _ => None,
    };
    match low {
        Some(y) if vehicle_width <= r.width() => [0.0, y, 0.0],
        _ => attachments::of(r).vehicle_point(r.yaw),
    }
}
/// `Vec3.yRot(-yaw * (float) (Math.PI / 180))` of an attachment point.
fn rotated([x, y, z]: [f64; 3], yaw: f32) -> [f64; 3] {
    let v = Vec3::new(x, y, z).rotate_y(-yaw * (std::f64::consts::PI / 180.0) as f32);
    [v.x, v.y, v.z]
}
/// `Entity.positionRider`: the vehicle's position plus its rotated passenger
/// attachment, less the rider's own vehicle attachment.
pub fn seat(attachment: [f64; 3], rider_offset: f64, at: Vec3, yaw: f32) -> Vec3 {
    ride_position(at, rotated(attachment, yaw), [0.0, rider_offset, 0.0])
}
/// `Entity.positionRider` with both points known.
pub fn ride_position(at: Vec3, passenger: [f64; 3], vehicle: [f64; 3]) -> Vec3 {
    Vec3::new(
        at.x + passenger[0] - vehicle[0],
        at.y + passenger[1] - vehicle[1],
        at.z + passenger[2] - vehicle[2],
    )
}

/// `Entity.startRiding`: the link it makes, `rider` on `vehicle` as its first
/// passenger (`addPassenger`).
pub fn link(mobs: &mut [Mob], rider: u32, vehicle: u32) {
    if rider == vehicle {
        return;
    }
    if let Some(m) = mobs.iter_mut().find(|m| m.id == rider) {
        m.extra.mount = Some(vehicle);
        m.extra.passenger = true;
    }
    if let Some(m) = mobs.iter_mut().find(|m| m.id == vehicle) {
        m.extra.rider = Some(rider);
    }
}
/// `Piglin.getTopPassenger`: down the chain of first passengers from
/// `vehicle`, stopping at the `counter`th seat.
fn top_passenger(mobs: &[Mob], vehicle: u32, counter: i32) -> u32 {
    let first = mobs
        .iter()
        .find(|m| m.id == vehicle)
        .and_then(|m| m.extra.rider);
    match first {
        Some(passenger) if counter != 1 => top_passenger(mobs, passenger, counter - 1),
        _ => vehicle,
    }
}
/// `Piglin.startRiding` over `Entity.startRiding` for two mobs: a baby piglin
/// that boards a hoglin sits on the top of up to three piglins stacked on it.
/// Not the vehicle it is on already, nor one it carries itself, and only
/// after its boarding cooldown, onto a vehicle with no passenger
/// (`canRide`, `canAddPassenger`). Returns whether it boarded.
pub fn start_riding(mobs: &mut [Mob], rider: u32, vehicle: u32) -> bool {
    let Some(r) = mobs.iter().find(|m| m.id == rider) else {
        return false;
    };
    let on_hoglin = mobs
        .iter()
        .any(|m| m.id == vehicle && m.species() == Species::Hoglin);
    let vehicle = if r.species() == Species::Piglin && r.is_baby() && on_hoglin {
        top_passenger(mobs, vehicle, MAX_PASSENGERS_ON_ONE_HOGLIN)
    } else {
        vehicle
    };
    if rider == vehicle || r.extra.mount == Some(vehicle) {
        return false;
    }
    // Nothing that rides the rider (through any number of vehicles) can be
    // its vehicle.
    let mut up = Some(vehicle);
    while let Some(v) = up {
        up = mobs.iter().find(|m| m.id == v).and_then(|m| m.extra.mount);
        if up == Some(rider) {
            return false;
        }
    }
    let Some(v) = mobs.iter().find(|m| m.id == vehicle) else {
        return false;
    };
    if r.extra.boarding_cooldown > 0 || v.extra.rider.is_some() || v.rider.is_some() {
        return false;
    }
    link(mobs, rider, vehicle);
    true
}
/// `LivingEntity.stopRiding` for the mob itself: it leaves its vehicle (which
/// forgets it when the riders are next synced), cannot board again for 60
/// ticks (`Entity.removePassenger`) and is set down at `at`, where
/// `dismountVehicle` puts it (`dismountTo`), or stays where it is.
pub fn stop_riding(m: &mut Mob, at: Option<Vec3>) {
    if m.extra.mount.take().is_none() {
        return;
    }
    m.extra.passenger = false;
    m.extra.boarding_cooldown = BOARDING_COOLDOWN;
    if let Some(at) = at {
        m.position = at;
        m.previous = at;
    }
}
/// `Animal.getDismountLocationForPassenger` for a standing passenger of
/// `width` by `height`: on the floor of the first block around the
/// vehicle's, going right, left, behind right, behind left, ahead right,
/// ahead left, behind and ahead of its facing, where the passenger's box
/// fits (`DismountHelper.canDismountTo`, without the world border); else on
/// top of the vehicle. A piglin is no `Animal`: it keeps
/// `Entity.getDismountLocationForPassenger`, on top of it.
pub fn dismount_location(w: &MobWorld, vehicle: &Seen, width: f32, height: f32) -> Vec3 {
    // `Entity.getDismountLocationForPassenger`.
    let on_top = Vec3::new(
        vehicle.position.x,
        vehicle.bounds.max_position().y,
        vehicle.position.z,
    );
    if vehicle.species == Species::Piglin {
        return on_top;
    }
    let (bx, by, bz) = (
        vehicle.position.x.floor() as i32,
        vehicle.position.y.floor() as i32,
        vehicle.position.z.floor() as i32,
    );
    // `Direction.fromYRot` of the vehicle's yaw: south, west, north, east.
    let facing = (f64::from(vehicle.yaw) / 90.0 + 0.5).floor() as i32;
    let (fx, fz) = [(0, 1), (-1, 0), (0, -1), (1, 0)][(facing & 3) as usize];
    // `DismountHelper.offsetsForDirection`; `right` is `Direction.getClockWise`.
    let (rx, rz) = (-fz, fx);
    let offsets = [
        (rx, rz),
        (-rx, -rz),
        (rx - fx, rz - fz),
        (-rx - fx, -rz - fz),
        (fx + rx, fz + rz),
        (fx - rx, fz - rz),
        (-fx, -fz),
        (fx, fz),
    ];
    // `EntityDimensions.makeBoundingBox`: the half width is a float.
    let half = f64::from(width / 2.0);
    for (dx, dz) in offsets {
        let (x, z) = (bx + dx, bz + dz);
        let floor = w.block_floor_height(x, by, z);
        // `DismountHelper.isBlockFloorValid`.
        if floor.is_infinite() || floor >= 1.0 {
            continue;
        }
        // `Vec3.upFromBottomCenterOf`.
        let at = Vec3::new(
            f64::from(x) + 0.5,
            f64::from(by) + floor,
            f64::from(z) + 0.5,
        );
        let bounds = Aabb::new(
            Vec3::new(at.x - half, at.y, at.z - half),
            Vec3::new(at.x + half, at.y + f64::from(height), at.z + half),
        );
        if w.no_collision(bounds) {
            return at;
        }
    }
    on_top
}

/// Ranged riders keep this far from their target instead of closing in.
const RANGED_DISTANCE: f64 = 8.0;
fn ranged(s: Species) -> bool {
    matches!(
        s,
        Species::Skeleton
            | Species::Stray
            | Species::Bogged
            | Species::Parched
            | Species::Pillager
            | Species::Witch
            | Species::Piglin
    )
}
/// A mob rider steers: while it has a target, its vehicle gives up its own
/// movement goals and carries it toward the target (melee riders to contact,
/// archers to a shooting distance). Without a target the vehicle wanders.
pub fn drive(m: &mut Mob, w: &MobWorld) {
    let Some(rider) = m.extra.rider else {
        m.ai.goals.disabled &= !CONTROLLED;
        return;
    };
    let seen = w.seen(rider).filter(|s| s.alive);
    let info = seen
        .and_then(|s| s.target)
        .and_then(|t| w.target_info(t))
        .filter(|t| t.alive && !(t.is_player && t.creative));
    let (Some(seen), Some(t)) = (seen, info) else {
        m.ai.goals.disabled &= !CONTROLLED;
        return;
    };
    m.ai.goals.disabled |= CONTROLLED;
    m.look_at_speed([t.position.x, t.eye_y, t.position.z], 30.0, 30.0);
    let close =
        ranged(seen.species) && m.distance_sqr(t.position) < RANGED_DISTANCE * RANGED_DISTANCE;
    if close {
        m.nav_stop();
    } else if m.ai.nav.is_done() || (m.age + m.id).is_multiple_of(10) {
        m.nav_move_to_entity(w, [t.position.x, t.position.y, t.position.z], 0, 1.0);
    }
}
/// The first passenger, as `getControllingPassenger` asks about it.
#[derive(Clone, Copy, Debug)]
pub enum First {
    /// A player; `steer` is holding the vehicle's steering stick.
    Player {
        steer: bool,
    },
    Mob(Species),
}
/// `getControllingPassenger` of mob `v` with `first` its first passenger: every
/// override (`Pig`, `Strider`, `HappyGhast`, `AbstractHorse`, `AbstractNautilus`),
/// else `Mob`'s (a mob rider that is no `#non_controlling_rider`, on a mob with AI).
pub fn controlling_passenger(v: &Mob, first: First) -> bool {
    let species = v.species();
    if let First::Player { steer } = first {
        match species {
            Species::Pig | Species::Strider if v.extra.saddled && steer => return true,
            Species::HappyGhast
                if super::happy_ghast::wearing_harness(v)
                    && !super::happy_ghast::on_still_timeout(v) =>
            {
                return true;
            }
            Species::Nautilus | Species::ZombieNautilus if v.extra.saddled => return true,
            s if super::equine::is_equine(s) && v.extra.saddled => return true,
            _ => {}
        }
    }
    // Entity.canControlVehicle: #non_controlling_rider.
    !v.no_ai
        && matches!(first, First::Mob(s) if !matches!(
            s,
            Species::Slime | Species::MagmaCube | Species::SulfurCube
        ))
}
/// `Mob.getControllingPassenger` for a mob on its back.
pub fn mob_controlled(m: &Mob, w: &MobWorld) -> bool {
    m.extra
        .rider
        .and_then(|r| w.seen(r))
        .is_some_and(|r| controlling_passenger(m, First::Mob(r.species)))
}
/// `Mob.requiresCustomPersistence`: a passenger never despawns.
pub fn keeps_alive(m: &Mob) -> bool {
    m.extra.mount.is_some()
}

/// `IsChickenJockey`, `SkeletonTrap` and `SkeletonTrapTime`.
pub fn write_nbt(m: &Mob, map: &mut Compound) {
    match m.species() {
        Species::Chicken => {
            map.insert(
                "IsChickenJockey".into(),
                Tag::Byte(i8::from(m.extra.jockey)),
            );
        }
        Species::SkeletonHorse => {
            map.insert("SkeletonTrap".into(), Tag::Byte(i8::from(m.extra.trap)));
            map.insert("SkeletonTrapTime".into(), Tag::Int(m.extra.trap_time));
        }
        _ => {}
    }
}
pub fn read_nbt(m: &mut Mob, tag: &Tag) {
    let int = |key: &str| match tag.get(key) {
        Some(Tag::Int(v)) => *v,
        Some(Tag::Byte(v)) => i32::from(*v),
        Some(Tag::Short(v)) => i32::from(*v),
        _ => 0,
    };
    match m.species() {
        Species::Chicken => m.extra.jockey = int("IsChickenJockey") != 0,
        Species::SkeletonHorse => {
            m.extra.trap = int("SkeletonTrap") != 0;
            m.extra.trap_time = int("SkeletonTrapTime");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::mobs::{MobTypes, attachments::Attachments};
    use std::sync::Arc;

    fn types() -> Option<MobTypes> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client")
            .join(horizon_core::VANILLA_VERSION);
        root.join("entity-types.json")
            .exists()
            .then(|| MobTypes::load(&root).unwrap())
    }
    fn mob(types: &MobTypes, name: &str, baby: bool) -> Mob {
        let kind = Arc::new(types.types[&format!("minecraft:{name}")].clone());
        let mut m = Mob::new(1, kind, Vec3::ZERO, 0.0);
        if baby {
            m.extra.age = -24000;
        }
        m
    }

    #[test]
    fn attachments_turn_with_the_vehicle() {
        // A ravager's seat sits a sixteenth behind its centre.
        let behind = [0.0, 2.2625, -0.0625];
        let [x, y, z] = rotated(behind, 0.0);
        assert!(x.abs() < 1e-6 && (z + 0.0625).abs() < 1e-6 && y == 2.2625);
        // Facing west (yaw 90), behind is to the east.
        let [x, _, z] = rotated(behind, 90.0);
        assert!((x - 0.0625).abs() < 1e-6 && z.abs() < 1e-6);
    }
    #[test]
    fn attachments_rotate_and_clamp() {
        let a = Attachments {
            passenger: vec![[0.0, 1.0, 0.5], [0.0, 2.0, -0.5]],
            vehicle: [0.0; 3],
            fixed: false,
        };
        let near = |p: [f64; 3], q: [f64; 3]| p.iter().zip(q).all(|(a, b)| (a - b).abs() < 1e-6);
        // Facing west (yaw 90), a point ahead of the centre (+z) is to the west (-x).
        assert!(near(a.passenger_point(0, 90.0), [-0.5, 1.0, 0.0]));
        assert!(near(a.passenger_point(1, 90.0), [0.5, 2.0, 0.0]));
        // An index past the end takes the last point.
        assert!(near(a.passenger_point(5, 90.0), a.passenger_point(1, 90.0)));
        assert!(near(a.passenger_point(0, 0.0), [0.0, 1.0, 0.5]));
    }
    #[test]
    fn scaling_leaves_fixed_dimensions_alone() {
        let a = Attachments {
            passenger: vec![[0.0, 1.0, 0.5]],
            vehicle: [0.0, 0.2, 0.0],
            fixed: false,
        };
        assert_eq!(a.scaled(0.5).passenger, vec![[0.0, 0.5, 0.25]]);
        assert!((a.scaled(0.5).vehicle[1] - 0.1).abs() < 1e-9);
        let fixed = Attachments { fixed: true, ..a };
        assert_eq!(fixed.scaled(0.5).passenger, vec![[0.0, 1.0, 0.5]]);
    }
    #[test]
    fn the_exported_points_match_the_hand_tables_they_replace() {
        let Some(types) = types() else { return };
        // The old `base_attachment` rows: vanilla's float values, so a tolerance.
        let seats: [(&str, bool, [f64; 3]); 12] = [
            ("chicken", false, [0.0, 0.7, -0.1]),
            ("spider", false, [0.0, 0.765, 0.0]),
            ("ravager", false, [0.0, 2.2625, -0.0625]),
            ("horse", false, [0.0, 1.44375, 0.0]),
            ("donkey", false, [0.0, 1.1125, 0.0]),
            ("mule", false, [0.0, 1.2125, 0.0]),
            ("skeleton_horse", false, [0.0, 1.31875, 0.0]),
            ("zombie_horse", false, [0.0, 1.31875, 0.0]),
            ("pig", false, [0.0, 0.86875, 0.0]),
            ("hoglin", true, [0.0, 0.875, 0.0]),
            ("hoglin", false, [0.0, 1.49375, 0.0]),
            ("nautilus", false, [0.0, 1.1375, 0.0]),
        ];
        for (name, baby, row) in seats {
            let m = mob(&types, name, baby);
            let got = attachments::of(&m).passenger_point(0, 0.0);
            assert!(
                got.iter().zip(row).all(|(a, b)| (a - b).abs() < 1e-6),
                "{name}: {got:?} vs {row:?}"
            );
        }
        // The old `vehicle_attachment` rows (a wide vehicle: spiders sit at the default).
        let rows: [(&str, bool, f64); 17] = [
            ("zombie", true, 0.1875),
            ("husk", true, 0.1875),
            ("drowned", true, 0.1875),
            ("zombie_villager", true, 0.125),
            ("piglin", true, 0.1875),
            ("zombie", false, 0.7),
            ("zombie_villager", false, 0.7),
            ("zombified_piglin", false, 0.7),
            ("piglin", false, 0.7),
            ("piglin_brute", false, 0.7),
            ("skeleton", false, 0.7),
            ("stray", false, 0.7),
            ("bogged", false, 0.7),
            ("pillager", false, 0.6),
            ("vindicator", false, 0.6),
            ("evoker", false, 0.6),
            ("chicken", false, 0.0),
        ];
        for (name, baby, y) in rows {
            let m = mob(&types, name, baby);
            let got = vehicle_attachment(&m, 100.0)[1];
            assert!((got - y).abs() < 1e-6, "{name}: {got} vs {y}");
        }
    }
    #[test]
    fn a_baby_uses_the_attachments_of_its_baby_dimensions() {
        let Some(types) = types() else { return };
        // `Horse` and `Chicken` declare their own `BABY_DIMENSIONS` attachments.
        let horse = mob(&types, "horse", true);
        assert!((attachments::of(&horse).passenger[0][1] - 1.0325).abs() < 1e-6);
        let chicken = mob(&types, "chicken", true);
        assert!((attachments::of(&chicken).passenger[0][1] - 0.375).abs() < 1e-6);
        // A baby sheep scales the adult's: its class has no attachments of its own beyond the box.
        let sheep = mob(&types, "sheep", true);
        assert_eq!(attachments::of(&sheep).vehicle, [0.0; 3]);
    }
    #[test]
    fn a_spider_sits_lower_on_a_vehicle_no_wider_than_it() {
        let Some(types) = types() else { return };
        let spider = mob(&types, "spider", false);
        assert_eq!(vehicle_attachment(&spider, 1.4)[1], 0.3125);
        assert_eq!(vehicle_attachment(&spider, 1.5)[1], 0.0);
        let cave = mob(&types, "cave_spider", false);
        assert_eq!(vehicle_attachment(&cave, 0.7)[1], 0.21875);
    }
    #[test]
    fn a_cube_mob_carries_its_rider_a_hair_below_its_top() {
        let Some(types) = types() else { return };
        let mut slime = mob(&types, "slime", false);
        crate::mobs::cube::set_size(&mut slime, 4);
        let at = passenger_attachment(&slime, 0, 0.0, false);
        assert!((at[1] - (f64::from(slime.height()) - 0.015625 * 4.0)).abs() < 1e-9);
    }
    #[test]
    fn archers_keep_their_distance_and_brawlers_close_in() {
        assert!(ranged(Species::Skeleton) && ranged(Species::Pillager));
        assert!(!ranged(Species::Zombie) && !ranged(Species::Vindicator));
    }
}
