//! Mobs riding mobs (`Entity.getPassengers`): where a rider sits on its
//! vehicle (`EntityAttachments`), the rider steering the vehicle toward its
//! target (`Mob.getControllingPassenger`, whose navigation and move control
//! the rider's goals use), and the saved jockey fields. The game links the
//! two (`game/passengers.rs`) and spawns the jockeys.
use super::{
    Mob, MobWorld, Seen, Species, attachments,
    goals::{JUMP, LOOK, MOVE, TARGET},
};
use crate::riding::{Ent, Riding};
use horizon_core::geometry::{Aabb, Vec3, trig};
use horizon_world::nbt::{Compound, Tag};
use std::cell::RefCell;

/// The goal flags a controlling passenger takes from its vehicle.
const CONTROLLED: u8 = MOVE | JUMP | LOOK;
/// `Piglin.startRiding`: how many baby piglins stack on one hoglin.
pub const MAX_PASSENGERS_ON_ONE_HOGLIN: i32 = 3;

/// `LivingEntity.getScale`: the `minecraft:scale` the server sent on any
/// living entity, 1 when unset (`Mob::model_scale`).
pub(super) fn scale(m: &Mob) -> f64 {
    f64::from(m.model_scale())
}
/// `Piglin.getTopPassenger`: down the chain of first passengers from `vehicle`, stopping at the `counter`th seat.
pub fn top_passenger(riding: &Riding, vehicle: Ent, counter: i32) -> Ent {
    match riding.first_passenger(vehicle) {
        Some(p) if counter != 1 => top_passenger(riding, p, counter - 1),
        _ => vehicle,
    }
}
/// `Piglin.startRiding` then `Entity.startRiding(vehicle, false, ..)` in the turn of the piglin `rider`:
/// the table links at once. A hoglin or piglin takes no one else (`canAddPassenger`); returns the vehicle boarded.
pub fn board_in_turn(
    riding: &RefCell<Riding>,
    rider: u32,
    baby_piglin: bool,
    vehicle: u32,
    vehicle_species: Species,
) -> Option<u32> {
    if !matches!(vehicle_species, Species::Hoglin | Species::Piglin) {
        return None;
    }
    let r = Ent::Id(rider);
    let mut table = riding.borrow_mut();
    let mut v = Ent::Id(vehicle);
    if baby_piglin && vehicle_species == Species::Hoglin {
        v = top_passenger(&table, v, MAX_PASSENGERS_ON_ONE_HOGLIN);
    }
    if r == v
        || table.vehicle_of(r) == Some(v)
        || table.rides_through(v, r)
        || table.boarding_cooldown(r) > 0
        || table.is_vehicle(v)
    {
        return None;
    }
    if table.is_passenger(r) {
        table.unlink(r);
    }
    table.link(r, v);
    v.id()
}
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
        Some(y) if vehicle_width <= r.width() => [0.0, y * scale(r), 0.0],
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
/// `Mob.getControllingPassenger`: NoAI belongs to the vehicle, not its rider.
fn controller<'a>(m: &Mob, w: &'a MobWorld) -> Option<&'a Seen> {
    if m.no_ai {
        return None;
    }
    w.first_passenger(m.id)
        .and_then(Ent::id)
        .and_then(|id| w.seen(id))
        .filter(|r| {
            r.alive
                && !w.data.entity_has(
                    "minecraft:non_controlling_rider",
                    &format!("minecraft:{}", super::species::def(r.species).name),
                )
        })
}
/// Vehicle goal flags are independent of whether the rider has a combat target.
/// Ravager.updateControlFlags permits raiders, unlike Mob.updateControlFlags.
pub fn drive(m: &mut Mob, w: &MobWorld) {
    let rider = controller(m, w);
    let disabled = rider
        .is_some_and(|r| m.species() != Species::Ravager || !super::raider::is_raider(r.species));
    let flags = CONTROLLED
        | if m.species() == Species::Ravager {
            TARGET
        } else {
            0
        };
    if disabled {
        m.ai.goals.disabled |= flags;
    } else {
        m.ai.goals.disabled &= !flags;
    }
    // Keep the existing combat steering and ranged stopping distance.
    let Some((seen, t)) = rider.and_then(|r| {
        r.target
            .and_then(|target| w.target_info(target))
            .filter(|t| t.alive && !(t.is_player && t.creative))
            .map(|t| (r, t))
    }) else {
        return;
    };
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
    controller(m, w).is_some()
}

/// Tick a passenger with Mob.getNavigation's recursive vehicle redirection.
/// Navigation is lent, not copied: goal start/stop/path progress all address
/// the same object the vehicle ticks. The pathfinder sees the vehicle body.
pub fn tick(
    mobs: &mut [Mob],
    index: usize,
    w: &MobWorld,
    random: &mut horizon_core::random::LegacyRandom,
    events: &mut Vec<super::MobEvent>,
) -> bool {
    let mut owner = index;
    if !mobs[index].no_ai {
        for _ in 0..mobs.len() {
            let Some(vehicle) = w
                .vehicle_id(mobs[owner].id)
                .and_then(|id| mobs.iter().position(|m| m.id == id))
            else {
                break;
            };
            if vehicle == index
                || controller(&mobs[vehicle], w).is_none_or(|r| r.id != mobs[owner].id)
            {
                break;
            }
            owner = vehicle;
        }
    }
    if owner == index {
        return mobs[index].tick(w, random, events);
    }
    let (rider, vehicle) = if index < owner {
        let (left, right) = mobs.split_at_mut(owner);
        (&mut left[index], &mut right[0])
    } else {
        let (left, right) = mobs.split_at_mut(index);
        (&mut right[0], &mut left[owner])
    };
    let body = vehicle.path_mob();
    std::mem::swap(&mut rider.ai.nav, &mut vehicle.ai.nav);
    rider.ai.navigation_redirect = Some(Box::new(super::nav::Redirect {
        own: std::mem::take(&mut vehicle.ai.nav),
        body,
        follow_range: vehicle.attrs.follow_range as f32,
        passenger: w.is_passenger(vehicle.id),
        in_liquid: vehicle.is_in_liquid(),
    }));
    let kept = rider.tick(w, random, events);
    let redirect = rider
        .ai
        .navigation_redirect
        .take()
        .expect("navigation loan lasts for the tick");
    vehicle.ai.nav = std::mem::replace(&mut rider.ai.nav, redirect.own);
    kept
}
/// `Mob.requiresCustomPersistence`: a passenger never despawns.
pub fn keeps_alive(m: &Mob, w: &MobWorld) -> bool {
    w.is_passenger(m.id)
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
    fn a_piglin_boards_at_once_in_its_own_turn() {
        let table = RefCell::new(Riding::default());
        let boarded = board_in_turn(&table, 1, false, 2, Species::Hoglin);
        assert_eq!(boarded, Some(2));
        assert_eq!(table.borrow().vehicle_of(Ent::Id(1)), Some(Ent::Id(2)));
    }
    #[test]
    fn a_baby_piglin_boards_the_top_of_the_stack_and_a_full_seat_refuses() {
        let table = RefCell::new(Riding::default());
        table.borrow_mut().link(Ent::Id(3), Ent::Id(2));
        table.borrow_mut().link(Ent::Id(4), Ent::Id(3));
        table.borrow_mut().link(Ent::Id(5), Ent::Id(4));
        // The third seat down the chain holds a rider already: nothing to board.
        assert_eq!(board_in_turn(&table, 1, true, 2, Species::Hoglin), None);
        assert_eq!(board_in_turn(&table, 1, false, 2, Species::Hoglin), None);
        assert_eq!(table.borrow().vehicle_of(Ent::Id(1)), None);
        // With one rider fewer, the baby takes the top seat.
        table.borrow_mut().unlink(Ent::Id(5));
        assert_eq!(board_in_turn(&table, 1, true, 2, Species::Hoglin), Some(4));
    }
    #[test]
    fn a_piglin_in_its_cooldown_does_not_board() {
        let table = RefCell::new(Riding::default());
        table.borrow_mut().link(Ent::Id(1), Ent::Id(9));
        table.borrow_mut().unlink(Ent::Id(1));
        assert_eq!(board_in_turn(&table, 1, false, 2, Species::Hoglin), None);
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
    /// An NPC plugin's enlarged fake player: the name it carries sits on the
    /// top of its scaled box, and on a seat it hangs `Avatar`'s 0.6 lower, scaled too.
    #[test]
    fn a_scaled_player_carries_and_sits_in_its_scale() {
        let Some(types) = types() else { return };
        if !types.types.contains_key("minecraft:player") {
            return;
        }
        let npc = |scale: f32| {
            let mut m = mob(&types, "player", false);
            m.extra.model_scale = scale;
            m
        };
        let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
        assert!(close(
            passenger_attachment(&npc(0.0), 0, 0.0, false)[1],
            1.8
        ));
        assert!(close(
            passenger_attachment(&npc(1.5), 0, 0.0, false)[1],
            2.7
        ));
        assert!(close(vehicle_attachment(&npc(0.0), 1.375)[1], 0.6));
        assert!(close(vehicle_attachment(&npc(1.5), 1.375)[1], 0.9));
        // On a seat at y 55 (a text display, whose attachment is its feet):
        // a 1.5 NPC's feet at 54.1, where the server puts it.
        let seat = seat(
            [0.0; 3],
            vehicle_attachment(&npc(1.5), 1.375)[1],
            Vec3::new(0.0, 55.0, 0.0),
            0.0,
        );
        assert!(close(seat.y, 54.1));
    }
    /// A scaled vehicle that is not an avatar: `remote_mob_attributes` puts
    /// `minecraft:scale` on any living entity, so the seat rides at the scaled
    /// box height on a cow as on a fake player.
    #[test]
    fn a_scaled_vehicle_seat_stays_scaled() {
        let Some(types) = types() else { return };
        let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
        let mut cow = mob(&types, "cow", false);
        // `EntityTypes.COW`: passengerAttachments(1.36875F).
        assert!(close(passenger_attachment(&cow, 0, 0.0, false)[1], 1.36875));
        cow.extra.model_scale = 2.0;
        assert!(close(passenger_attachment(&cow, 0, 0.0, false)[1], 2.7375));
    }
    /// The attachments are `EntityTypes` floats widened to double, and a
    /// zombie's seat (2.0125) is above its box (1.95).
    #[test]
    fn seats_are_the_entity_type_floats() {
        let Some(types) = types() else { return };
        let close = |a: f64, b: f64| (a - b).abs() < 1e-12;
        let pig = mob(&types, "pig", false);
        assert!(close(
            passenger_attachment(&pig, 0, 0.0, false)[1],
            0.868_749_976_158_142_1
        ));
        let zombie = mob(&types, "zombie", false);
        assert!(close(
            passenger_attachment(&zombie, 0, 0.0, false)[1] - vehicle_attachment(&zombie, 100.0)[1],
            1.312_500_059_604_645
        ));
    }
    /// `Pig.getDefaultDimensions`: BABY_DIMENSIONS overrides PASSENGER.
    #[test]
    fn a_baby_pig_uses_its_own_passenger_attachment() {
        let Some(types) = types() else { return };
        let rider = mob(&types, "pig", false);
        for (scale, baby_y, adult_y) in [
            (0.0, 0.5, 0.868_749_976_158_142_1),
            (1.0, 0.5, 0.868_749_976_158_142_1),
            (2.0, 1.0, 1.737_499_952_316_284_2),
        ] {
            for (age, expected) in [(-24000, baby_y), (-1, baby_y), (0, adult_y), (42, adult_y)] {
                let mut vehicle = mob(&types, "pig", false);
                vehicle.position = Vec3::new(4.5, 200.0, 4.5);
                vehicle.extra.model_scale = scale;
                vehicle.extra.age = age;
                let attachment = passenger_attachment(&vehicle, 0, vehicle.yaw, false);
                assert_eq!(attachment, [0.0, expected, 0.0], "age={age}, scale={scale}");
                let offset = vehicle_attachment(&rider, vehicle.width())[1];
                assert_eq!(offset, 0.0);
                assert_eq!(
                    seat(attachment, offset, vehicle.position, vehicle.yaw),
                    Vec3::new(4.5, 200.0 + expected, 4.5)
                );
            }
        }
    }

    #[test]
    fn archers_keep_their_distance_and_brawlers_close_in() {
        assert!(ranged(Species::Skeleton) && ranged(Species::Pillager));
        assert!(!ranged(Species::Zombie) && !ranged(Species::Vindicator));
    }
    /// `Spider.getVehicleAttachmentPoint` and
    /// `CaveSpider.getVehicleAttachmentPoint`: a vehicle no wider than the
    /// spider takes the spider's fixed seat — 0.3125 of its scale, and a cave
    /// spider's 0.21875 — and a wider one the type's default.
    #[test]
    fn a_spider_sits_by_its_own_rule_on_a_narrow_vehicle() {
        // World-gated: without the extracted data the body never runs.
        let _ = crate::mobs::world::with_test_world(&[], |_, types| {
            let new = |kind: &str| {
                Mob::new(
                    1,
                    std::sync::Arc::new(types.types[kind].clone()),
                    Vec3::ZERO,
                    0.0,
                )
            };
            let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
            let chicken = 0.4_f32;
            // Wider than either spider (the boat's 1.375 is not).
            let wide = 2.0_f32;
            let spider = new("minecraft:spider");
            let cave = new("minecraft:cave_spider");
            assert!(spider.width() > chicken && cave.width() > chicken);
            assert!(spider.width() < wide && cave.width() < wide);
            assert!(close(vehicle_attachment(&spider, chicken)[1], 0.3125));
            assert!(close(vehicle_attachment(&cave, chicken)[1], 0.21875));
            assert!(close(vehicle_attachment(&spider, wide)[1], 0.0));
            assert!(close(vehicle_attachment(&cave, wide)[1], 0.0));
            let mut big = new("minecraft:cave_spider");
            big.extra.model_scale = 2.0;
            assert!(close(vehicle_attachment(&big, chicken)[1], 0.4375));
        });
    }
}

#[cfg(test)]
mod happy_ghast_seat_tests {
    use super::*;

    #[test]
    fn passengers_take_separate_seats() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client")
            .join(horizon_core::VANILLA_VERSION);
        if !root.join("entity-types.json").exists() {
            return;
        }
        let types = crate::mobs::MobTypes::load(&root).unwrap();
        let kind = std::sync::Arc::new(types.types["minecraft:happy_ghast"].clone());
        let ghast = Mob::new(1, kind, Vec3::ZERO, 0.0);
        let seat = |i| passenger_attachment(&ghast, i, 0.0, false);
        assert_eq!(seat(0), [0.0, 4.0, 1.7]);
        let seats: Vec<_> = (0..4).map(seat).collect();
        for (i, a) in seats.iter().enumerate() {
            assert!(seats[i + 1..].iter().all(|b| a != b));
        }
        assert_eq!(seat(9), seats[3]);
    }
}
