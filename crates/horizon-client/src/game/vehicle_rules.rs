//! What a vehicle answers about its passengers, one dispatch per vanilla method:
//! `canAddPassenger`, `couldAcceptPassenger`, `getControllingPassenger`,
//! `getDismountLocationForPassenger` and `getPassengerAttachmentPoint`.
use super::Game;
use horizon_client::{
    blocks::Fluid,
    mobs::{
        Mob, Species,
        riders::{self, First},
    },
    motion::PlayerMotion,
    pose::{Dimensions, Pose},
    riding::Ent,
};
use horizon_core::geometry::{Aabb, Vec3, VoxelShape, trig};

/// What a passenger asks of its seat: its own vehicle attachment, and whether
/// it sits as an animal does (further back in a boat).
#[derive(Clone, Copy, Debug)]
pub(super) struct Seat {
    pub(super) offset: f64,
    pub(super) animal: bool,
}
impl Seat {
    /// `Player.DEFAULT_VEHICLE_ATTACHMENT`.
    pub(super) const PLAYER: Self = Self {
        offset: 0.6,
        animal: false,
    };
}

/// A vehicle, by its place in the `Game` vector of its kind.
#[derive(Clone, Copy, Debug)]
pub(super) enum Vehicle {
    Boat(usize),
    Cart(usize),
    Cushion(usize),
    Mob(usize),
    /// Any other kind, by entity id.
    Plain(u32),
}

/// The mount a rider gets off: its kind, where it stands, its box,
/// `getBbWidth`, `getBbHeight` and body yaw.
impl Mount {
    fn of(m: &Mob) -> Self {
        Self {
            position: m.position,
            bounds: m.bounds(),
            width: m.width(),
            yaw: m.yaw,
        }
    }
}
struct Mount {
    position: Vec3,
    bounds: Aabb,
    width: f32,
    yaw: f32,
}
/// The rider getting off: `getYRot`, `getBbWidth`, and
/// `getLocalBoundsForPose` for each of `Player.getDismountPoses`.
pub(super) struct Rider {
    pub(super) yaw: f32,
    /// `getMainArm() == HumanoidArm.LEFT`.
    pub(super) main_arm_left: bool,
    pub(super) width: f32,
    pub(super) poses: Vec<(Pose, Dimensions)>,
}
impl Rider {
    /// A player getting off, facing `yaw`.
    pub(super) fn player(p: &PlayerMotion, yaw: f32, main_arm_left: bool) -> Self {
        Self {
            yaw,
            main_arm_left,
            width: p.dimensions.width,
            poses: rider_poses(p),
        }
    }
    /// A mob getting off: `LivingEntity.getDismountPoses` is standing alone.
    pub(super) fn mob(m: &Mob) -> Self {
        Self {
            yaw: m.yaw,
            main_arm_left: m.main_arm_left(),
            width: m.width(),
            poses: vec![(
                Pose::Standing,
                Dimensions {
                    width: m.width(),
                    height: m.height(),
                    eye_height: m.eye_height() as f32,
                },
            )],
        }
    }
}
/// `Player.getDismountPoses`: standing, crouching, then crawling.
const DISMOUNT_POSES: [Pose; 3] = [Pose::Standing, Pose::Crouching, Pose::Swimming];
/// The rider's box in each dismount pose, at their `minecraft:scale`.
fn rider_poses(p: &PlayerMotion) -> Vec<(Pose, Dimensions)> {
    // EntityDimensions.scale: every player pose scales alike, so the
    // standing width gives the scale without a mutable attribute lookup.
    let scale = p.dimensions.width / Pose::Standing.dimensions(1.0).width;
    DISMOUNT_POSES
        .iter()
        .map(|&pose| (pose, pose.dimensions(scale)))
        .collect()
}
/// The level queries a dismount makes.
trait DismountWorld {
    /// `getFluidState(pos).is(FluidTags.LAVA)`.
    fn lava(&self, p: [i32; 3]) -> bool;
    /// `BlockGetter.getBlockFloorHeight(pos)`.
    fn floor_height(&self, p: [i32; 3]) -> f64;
    /// `DismountHelper.canDismountTo(level, passenger, box)`.
    fn can_dismount_to(&self, bounds: Aabb) -> bool;
}
struct GameDismountWorld<'a, 'g>(&'a Game<'g>);
impl DismountWorld for GameDismountWorld<'_, '_> {
    fn lava(&self, p: [i32; 3]) -> bool {
        let g = self.0;
        g.res.facts().get(g.world.block(p[0], p[1], p[2])).fluid == Fluid::Lava
    }
    fn floor_height(&self, p: [i32; 3]) -> f64 {
        let shape = self.0.block_collision_shape(p);
        let below = std::cell::OnceCell::<VoxelShape>::new();
        horizon_core::geometry::block_floor_height(&shape, || {
            below.get_or_init(|| self.0.block_collision_shape([p[0], p[1] - 1, p[2]]))
        })
    }
    fn can_dismount_to(&self, bounds: Aabb) -> bool {
        self.0.can_dismount_to(bounds)
    }
}
/// `Entity.getCollisionHorizontalEscapeVector`: out of the vehicle's box
/// along `yaw`, to the edge of the square it spans.
fn escape_vector(vehicle_width: f64, passenger_width: f64, yaw: f32) -> Vec3 {
    // 1.0E-5F: a float literal widened to double.
    let distance = (vehicle_width + passenger_width + f64::from(1.0e-5_f32)) / 2.0;
    let radians = f64::from(yaw * 0.017_453_292);
    let (sin, cos) = (-trig::sin(radians), trig::cos(radians));
    let max = f64::from(sin.abs().max(cos.abs()));
    Vec3::new(
        f64::from(sin) * distance / max,
        0.0,
        f64::from(cos) * distance / max,
    )
}
/// `Strider.getDismountLocationForPassenger`: the columns along the rider's
/// facing and 22.5 and 45 degrees either side of it, each from the top of
/// the strider's box down to half a block below its feet. The first block
/// with no lava, a valid floor (`DismountHelper.isBlockFloorValid`) and room
/// for one of the rider's dismount poses (`canDismountTo`) wins, with that
/// pose; without one, on top of the strider with the pose unchanged.
fn strider_dismount_location(
    strider: &Mount,
    rider: &Rider,
    world: &impl DismountWorld,
) -> (Vec3, Option<Pose>) {
    let (vehicle, passenger) = (f64::from(strider.width), f64::from(rider.width));
    let top = strider.bounds.max_position().y;
    let bottom = strider.bounds.min_position().y - 0.5;
    // A LinkedHashSet: each block once, in the order first reached.
    let mut blocks: Vec<[i32; 3]> = Vec::new();
    for turn in [0.0, -22.5, 22.5, -45.0, 45.0] {
        let offset = escape_vector(vehicle, passenger, rider.yaw + turn);
        let (x, z) = (
            (strider.position.x + offset.x).floor() as i32,
            (strider.position.z + offset.z).floor() as i32,
        );
        let mut y = top;
        let mut block_y = top.floor() as i32;
        while y > bottom {
            if !blocks.contains(&[x, block_y, z]) {
                blocks.push([x, block_y, z]);
            }
            block_y -= 1;
            y -= 1.0;
        }
    }
    for p in blocks {
        if world.lava(p) {
            continue;
        }
        // DismountHelper.isBlockFloorValid.
        let floor = world.floor_height(p);
        if floor.is_infinite() || floor >= 1.0 {
            continue;
        }
        // Vec3.upFromBottomCenterOf.
        let at = Vec3::new(
            f64::from(p[0]) + 0.5,
            f64::from(p[1]) + floor,
            f64::from(p[2]) + 0.5,
        );
        for (pose, dimensions) in &rider.poses {
            if world.can_dismount_to(dimensions.bounds(at)) {
                return (at, Some(*pose));
            }
        }
    }
    (Vec3::new(strider.position.x, top, strider.position.z), None)
}

/// `DismountHelper.offsetsForDirection` of a facing: right, left, behind right,
/// behind left, ahead right, ahead left, behind, ahead.
fn dismount_offsets(yaw: f32) -> [(i32, i32); 8] {
    // `Direction.fromYRot`: south, west, north, east.
    let facing = (f64::from(yaw) / 90.0 + 0.5).floor() as i32;
    let (fx, fz) = [(0, 1), (-1, 0), (0, -1), (1, 0)][(facing & 3) as usize];
    let (rx, rz) = (-fz, fx);
    [
        (rx, rz),
        (-rx, -rz),
        (rx - fx, rz - fz),
        (-rx - fx, -rz - fz),
        (fx + rx, fz + rz),
        (fx - rx, fz - rz),
        (-fx, -fz),
        (fx, fz),
    ]
}
/// `Animal.getDismountLocationForPassenger`: in each dismount pose, the first
/// block around the animal, in `offsetsForDirection` order, with a valid floor
/// and room for the pose; else on top of it.
fn animal_dismount_location(
    animal: &Mount,
    rider: &Rider,
    world: &impl DismountWorld,
) -> (Vec3, Option<Pose>) {
    let base = [
        animal.position.x.floor() as i32,
        animal.position.y.floor() as i32,
        animal.position.z.floor() as i32,
    ];
    for (pose, dimensions) in &rider.poses {
        for (dx, dz) in dismount_offsets(animal.yaw) {
            let p = [base[0] + dx, base[1], base[2] + dz];
            let floor = world.floor_height(p);
            if floor.is_infinite() || floor >= 1.0 {
                continue;
            }
            let at = Vec3::new(
                f64::from(p[0]) + 0.5,
                f64::from(p[1]) + floor,
                f64::from(p[2]) + 0.5,
            );
            if world.can_dismount_to(dimensions.bounds(at)) {
                return (at, Some(*pose));
            }
        }
    }
    (on_top(animal), None)
}
/// `Entity.getDismountLocationForPassenger`.
fn on_top(v: &Mount) -> Vec3 {
    Vec3::new(v.position.x, v.bounds.max_position().y, v.position.z)
}
/// `AbstractHorse.getDismountLocationInDirection`.
fn horse_dismount_in_direction(
    horse: &Mount,
    direction: Vec3,
    rider: &Rider,
    world: &impl DismountWorld,
) -> Option<(Vec3, Pose)> {
    let (x, z) = (
        horse.position.x + direction.x,
        horse.position.z + direction.z,
    );
    let y = horse.bounds.min_position().y;
    let limit = horse.bounds.max_position().y + 0.75;
    for (pose, dimensions) in &rider.poses {
        let mut block_y = y.floor() as i32;
        loop {
            let p = [x.floor() as i32, block_y, z.floor() as i32];
            let floor = world.floor_height(p);
            if f64::from(block_y) + floor > limit {
                break;
            }
            if !floor.is_infinite() && floor < 1.0 {
                let at = Vec3::new(x, f64::from(block_y) + floor, z);
                if world.can_dismount_to(dimensions.bounds(at)) {
                    return Some((at, *pose));
                }
            }
            block_y += 1;
            if f64::from(block_y) >= limit {
                break;
            }
        }
    }
    None
}
/// `AbstractHorse.getDismountLocationForPassenger`: to the side of the main
/// hand, then of the other hand, else where the horse stands.
fn horse_dismount_location(
    horse: &Mount,
    rider: &Rider,
    world: &impl DismountWorld,
) -> (Vec3, Option<Pose>) {
    let (vehicle, passenger) = (f64::from(horse.width), f64::from(rider.width));
    let sides = if rider.main_arm_left {
        [-90.0, 90.0]
    } else {
        [90.0, -90.0]
    };
    for side in sides {
        let direction = escape_vector(vehicle, passenger, horse.yaw + side);
        if let Some((at, pose)) = horse_dismount_in_direction(horse, direction, rider, world) {
            return (at, Some(pose));
        }
    }
    (horse.position, None)
}

/// The `canAddPassenger` override of a vehicle, with what it reads.
#[derive(Clone, Copy, Debug)]
enum Room {
    /// `Entity`: empty.
    Default,
    Boat {
        max: usize,
        eyes_in_water: bool,
    },
    Camel,
    HappyGhast,
    Nautilus,
    Strider {
        eyes_in_lava: bool,
    },
}
/// `canAddPassenger` with `n` passengers aboard.
fn has_room(room: Room, n: usize) -> bool {
    match room {
        Room::Default | Room::Nautilus => n == 0,
        Room::Boat { max, eyes_in_water } => n < max && !eyes_in_water,
        // Camel: vanilla writes `<= 2`, so three fit.
        Room::Camel => n <= 2,
        Room::HappyGhast => n < 4,
        Room::Strider { eyes_in_lava } => n == 0 && !eyes_in_lava,
    }
}

impl Game<'_> {
    /// `isEyeInFluid(LAVA)` of mob `m`: its eyes inside the lava of their block.
    fn eye_in_lava(&self, m: &Mob) -> bool {
        let eye_y = m.eye_y();
        let (x, y, z) = (
            m.position.x.floor() as i32,
            eye_y.floor() as i32,
            m.position.z.floor() as i32,
        );
        self.fluid_height_of(x, y, z, Some(Fluid::Lava))
            .is_some_and(|(h, _)| eye_y <= f64::from(y) + f64::from(h))
    }
    /// `canAddPassenger` of vehicle `v` that carries `passengers` now.
    pub(super) fn can_add_passenger(&self, v: Vehicle, passengers: &[Ent]) -> bool {
        let room = match v {
            Vehicle::Boat(i) => Room::Boat {
                max: self.boats[i].max_passengers(),
                eyes_in_water: self.boats[i].under_water(),
            },
            Vehicle::Cart(_) | Vehicle::Cushion(_) | Vehicle::Plain(_) => Room::Default,
            Vehicle::Mob(i) => match self.mobs[i].species() {
                Species::Camel | Species::CamelHusk => Room::Camel,
                Species::HappyGhast => Room::HappyGhast,
                Species::Nautilus | Species::ZombieNautilus => Room::Nautilus,
                Species::Strider => Room::Strider {
                    eyes_in_lava: self.eye_in_lava(&self.mobs[i]),
                },
                _ => Room::Default,
            },
        };
        has_room(room, passengers.len())
    }
    /// `couldAcceptPassenger`: false for the entities that are no seat at all.
    pub(super) fn could_accept_passenger(&self, v: Vehicle) -> bool {
        match v {
            Vehicle::Mob(i) => !matches!(
                &*self.mobs[i].kind.id,
                "minecraft:ominous_item_spawner" | "minecraft:marker"
            ),
            _ => true,
        }
    }
    /// `getControllingPassenger` of `v` with `passengers` aboard: the passenger
    /// that steers, if any.
    pub(super) fn controlling_passenger(&self, v: Vehicle, passengers: &[Ent]) -> Option<Ent> {
        let Vehicle::Mob(i) = v else {
            // AbstractBoat: the first passenger when it is a player.
            return match v {
                Vehicle::Boat(_) => passengers.first().copied().filter(|p| p.is_player()),
                Vehicle::Cart(_) | Vehicle::Cushion(_) | Vehicle::Mob(_) | Vehicle::Plain(_) => {
                    None
                }
            };
        };
        let first = *passengers.first()?;
        let kind = match first {
            Ent::Id(id) => First::Mob(self.mobs.iter().find(|m| m.id == id)?.species()),
            _ => First::Player {
                steer: self.mobs[i].rider.is_some_and(|r| r.steer),
            },
        };
        riders::controlling_passenger(&self.mobs[i], kind).then_some(first)
    }
    /// `getDismountLocationForPassenger` of `v` for `rider`, and the pose the
    /// rider takes there.
    pub(super) fn dismount_location(&self, v: Vehicle, rider: &Rider) -> (Vec3, Option<Pose>) {
        match v {
            Vehicle::Boat(i) => (
                self.boat_dismount(i, f64::from(rider.width), rider.yaw),
                None,
            ),
            Vehicle::Cart(i) => (self.cart_dismount(i), None),
            Vehicle::Cushion(i) => (self.cushions[i].dismount(), None),
            Vehicle::Plain(id) => (self.plain_dismount(id), None),
            Vehicle::Mob(i) => {
                let m = &self.mobs[i];
                let mount = Mount::of(m);
                let world = GameDismountWorld(self);
                match m.species() {
                    Species::Strider => strider_dismount_location(&mount, rider, &world),
                    s if horizon_client::mobs::equine::is_equine(s) => {
                        horse_dismount_location(&mount, rider, &world)
                    }
                    Species::HappyGhast => (on_top(&mount), None),
                    s if horizon_client::mobs::drops::is_animal(s) => {
                        animal_dismount_location(&mount, rider, &world)
                    }
                    _ => (on_top(&mount), None),
                }
            }
        }
    }
    /// `getPassengerRidingPosition` of `v` for the passenger at `index` of
    /// `count`, less the passenger's own vehicle attachment: where its feet
    /// land, at the vehicle's current place or, with `previous`, its last.
    pub(super) fn passenger_point(
        &self,
        v: Vehicle,
        index: usize,
        count: usize,
        seat: Seat,
        previous: bool,
    ) -> Vec3 {
        match v {
            Vehicle::Boat(i) => {
                self.boats[i].seat(index, count, seat.animal, seat.offset, previous)
            }
            // AbstractMinecart: the default attachment, 0.1875 above its feet.
            Vehicle::Cart(i) => self.carts[i].position.add_vector(Vec3::new(
                0.0,
                super::minecarts::PASSENGER_Y - seat.offset,
                0.0,
            )),
            Vehicle::Cushion(i) => self.cushions[i].seat(),
            Vehicle::Plain(id) => self
                .plain_seat(id, index, [0.0, seat.offset, 0.0])
                .unwrap_or(Vec3::ZERO),
            Vehicle::Mob(i) => {
                let m = &self.mobs[i];
                let (at, yaw) = if previous {
                    (m.previous, m.previous_yaw)
                } else {
                    (m.position, m.yaw)
                };
                let attachment = riders::passenger_attachment(m, index, yaw, self.remote.is_some());
                riders::ride_position(at, attachment, [0.0, seat.offset, 0.0])
            }
        }
    }
    /// `AbstractBoat.getDismountLocationForPassenger` for a passenger `width`
    /// wide facing `yaw`: in front of it if there is floor there, else on top.
    fn boat_dismount(&self, i: usize, width: f64, yaw: f32) -> Vec3 {
        let b = &self.boats[i];
        let r = yaw.to_radians();
        let distance = (super::boats::WIDTH * std::f64::consts::SQRT_2 + width + 1.0e-5) / 2.0;
        let (s, c) = (-(r.sin()) as f64, r.cos() as f64);
        let m = s.abs().max(c.abs());
        let (x, z) = (
            b.position.x + s * distance / m,
            b.position.z + c * distance / m,
        );
        let top = b.bounds().max_position().y;
        let (bx, by, bz) = (x.floor() as i32, top.floor() as i32, z.floor() as i32);
        let below = self.res.facts().get(self.world.block(bx, by - 1, bz));
        let here = self.res.facts().get(self.world.block(bx, by, bz));
        if below.fluid != Fluid::Water
            && !here.full_collision
            && (below.full_collision || below.legacy_solid)
        {
            Vec3::new(x, f64::from(by), z)
        } else {
            Vec3::new(b.position.x, top, b.position.z)
        }
    }
    /// `AbstractMinecart.getDismountLocationForPassenger`: beside the cart
    /// (across its facing), else on top.
    fn cart_dismount(&self, i: usize) -> Vec3 {
        let c = &self.carts[i];
        let (bx, by, bz) = (
            c.position.x.floor() as i32,
            c.position.y.floor() as i32,
            c.position.z.floor() as i32,
        );
        for dy in [0, 1, -1] {
            for (dx, dz) in dismount_offsets(c.yaw) {
                let (x, y, z) = (bx + dx, by + dy, bz + dz);
                let below = self.res.facts().get(self.world.block(x, y - 1, z));
                let here = self.res.facts().get(self.world.block(x, y, z));
                let head = self.res.facts().get(self.world.block(x, y + 1, z));
                if (below.full_collision || below.legacy_solid)
                    && !here.full_collision
                    && !head.full_collision
                    && here.fluid == Fluid::None
                {
                    return Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5);
                }
            }
        }
        Vec3::new(c.position.x, c.bounds().max_position().y, c.position.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// A lava sea up to y 30 with full blocks placed in it; air above.
    struct Sea {
        blocks: HashMap<[i32; 3], ()>,
    }
    impl Sea {
        fn with(blocks: &[[i32; 3]]) -> Self {
            Self {
                blocks: blocks.iter().map(|&b| (b, ())).collect(),
            }
        }
        fn solid(&self, p: [i32; 3]) -> bool {
            self.blocks.contains_key(&p)
        }
    }
    impl DismountWorld for Sea {
        fn lava(&self, p: [i32; 3]) -> bool {
            p[1] <= 30 && !self.solid(p)
        }
        fn floor_height(&self, p: [i32; 3]) -> f64 {
            if self.solid(p) {
                1.0
            } else if self.solid([p[0], p[1] - 1, p[2]]) {
                0.0
            } else {
                f64::NEG_INFINITY
            }
        }
        fn can_dismount_to(&self, bounds: Aabb) -> bool {
            let (lo, hi) = (bounds.min_position(), bounds.max_position());
            let cells = |a: f64, b: f64| a.floor() as i32..=(b - 1.0e-7).floor() as i32;
            !cells(lo.x, hi.x).any(|x| {
                cells(lo.y, hi.y).any(|y| cells(lo.z, hi.z).any(|z| self.solid([x, y, z])))
            })
        }
    }
    /// The top of a lava source at y 30 (`FlowingFluid.getOwnHeight`, 8/9).
    const SURFACE: f64 = 30.0 + 8.0 / 9.0;
    /// A strider (0.9 by 1.7) standing on the lava at x, z, facing south.
    fn strider_at(x: f64, z: f64) -> Mount {
        Mount {
            position: Vec3::new(x, SURFACE, z),
            bounds: Aabb::new(
                Vec3::new(x - 0.45, SURFACE, z - 0.45),
                Vec3::new(x + 0.45, SURFACE + 1.7, z + 0.45),
            ),
            width: 0.9,
            yaw: 0.0,
        }
    }
    fn rider(yaw: f32) -> Rider {
        Rider {
            yaw,
            main_arm_left: false,
            width: 0.6,
            poses: DISMOUNT_POSES
                .iter()
                .map(|&pose| (pose, pose.dimensions(1.0)))
                .collect(),
        }
    }
    fn block_of(at: Vec3) -> [i32; 3] {
        [
            at.x.floor() as i32,
            at.y.floor() as i32,
            at.z.floor() as i32,
        ]
    }

    #[test]
    fn a_strider_in_open_lava_puts_its_rider_on_its_back() {
        // #3513: getting off in a lava sea used to put the player beside the
        // strider, in the lava; with no floor in reach vanilla puts the rider
        // on top of the strider instead.
        let sea = Sea::with(&[]);
        let (at, pose) = strider_dismount_location(&strider_at(0.5, 0.5), &rider(0.0), &sea);
        assert!(!sea.lava(block_of(at)), "dismounted into lava at {at:?}");
        assert_eq!(at, Vec3::new(0.5, SURFACE + 1.7, 0.5));
        assert_eq!(pose, None);
    }

    #[test]
    fn a_strider_by_the_shore_puts_its_rider_on_the_shore() {
        // Netherrack 45 degrees to the right of a rider facing south (+z);
        // the straight-ahead and 22.5 degree columns are lava.
        let sea = Sea::with(&[[-1, 30, 1]]);
        let (at, pose) = strider_dismount_location(&strider_at(0.5, 0.5), &rider(0.0), &sea);
        assert_eq!(at, Vec3::new(-0.5, 31.0, 1.5));
        assert_eq!(pose, Some(Pose::Standing));
    }

    #[test]
    fn a_low_ceiling_on_the_shore_makes_the_rider_crawl_off() {
        let sea = Sea::with(&[[0, 30, 1], [0, 32, 1]]);
        let (at, pose) = strider_dismount_location(&strider_at(0.5, 0.5), &rider(0.0), &sea);
        assert_eq!(at, Vec3::new(0.5, 31.0, 1.5));
        assert_eq!(pose, Some(Pose::Swimming));
    }

    #[test]
    fn the_escape_vector_reaches_the_edge_of_the_square() {
        let edge = (0.9 + 0.6 + f64::from(1.0e-5_f32)) / 2.0;
        let v = escape_vector(0.9, 0.6, 0.0);
        assert!((v.z - edge).abs() < 1.0e-6 && v.x.abs() < 1.0e-6);
        // Facing west of south: the corner of the square.
        let v = escape_vector(0.9, 0.6, 45.0);
        assert!((v.x + edge).abs() < 1.0e-3 && (v.z - edge).abs() < 1.0e-3);
    }

    #[test]
    fn can_add_passenger_matches_each_vanilla_override() {
        let boat = |max, eyes_in_water| Room::Boat { max, eyes_in_water };
        for n in 0..=4 {
            assert_eq!(has_room(Room::Default, n), n == 0, "default {n}");
            assert_eq!(has_room(boat(2, false), n), n < 2, "boat {n}");
            assert_eq!(has_room(boat(1, false), n), n < 1, "chest boat {n}");
            assert!(!has_room(boat(2, true), n), "boat under water {n}");
            assert_eq!(has_room(Room::Camel, n), n <= 2, "camel {n}");
            assert_eq!(has_room(Room::HappyGhast, n), n < 4, "happy ghast {n}");
            assert_eq!(has_room(Room::Nautilus, n), n == 0, "nautilus {n}");
            let strider = |eyes_in_lava| Room::Strider { eyes_in_lava };
            assert_eq!(has_room(strider(false), n), n == 0, "strider {n}");
            assert!(!has_room(strider(true), n), "strider in lava {n}");
        }
    }

    #[test]
    fn default_dismount_is_the_top_of_the_box() {
        let m = strider_at(2.5, -3.5);
        assert_eq!(on_top(&m), Vec3::new(2.5, SURFACE + 1.7, -3.5));
    }

    #[test]
    fn dismount_offsets_turn_with_the_facing() {
        // Facing south (+z): right is west (-x).
        assert_eq!(dismount_offsets(0.0)[0], (-1, 0));
        assert_eq!(dismount_offsets(0.0)[7], (0, 1));
        // Facing west: right is north.
        assert_eq!(dismount_offsets(90.0)[0], (0, -1));
        assert_eq!(dismount_offsets(90.0)[7], (-1, 0));
    }

    #[test]
    fn a_horse_puts_its_rider_beside_it_else_where_it_stands() {
        let sea = Sea::with(&[[1, 30, 0], [1, 29, 0], [-2, 30, 0]]);
        let horse = strider_at(0.5, 0.5);
        // Facing south, the right side (+90) is west: x -0.5 ... a column with floor.
        let (at, pose) = horse_dismount_location(&horse, &rider(0.0), &sea);
        assert!(pose.is_some(), "stands on one side: {at:?}");
        let open = Sea::with(&[]);
        let (at, pose) = horse_dismount_location(&horse, &rider(0.0), &open);
        assert_eq!((at, pose), (horse.position, None));
    }

    #[test]
    fn a_horse_puts_a_left_handed_rider_on_the_other_side_first() {
        let sea = Sea::with(&[[-1, 29, 0], [1, 29, 0]]);
        let horse = strider_at(0.5, 0.5);
        let right = horse_dismount_location(&horse, &rider(0.0), &sea).0;
        let left_handed = Rider {
            main_arm_left: true,
            ..rider(0.0)
        };
        let left = horse_dismount_location(&horse, &left_handed, &sea).0;
        assert!(right.x < horse.position.x && left.x > horse.position.x);
        // With only the right-handed side open, a left-handed rider falls back to it.
        let west = Sea::with(&[[-1, 29, 0]]);
        assert!(horse_dismount_location(&horse, &left_handed, &west).0.x < horse.position.x);
    }

    #[test]
    fn an_animal_puts_its_rider_on_the_first_block_with_floor() {
        let sea = Sea::with(&[[-1, 29, 0]]);
        let animal = strider_at(0.5, 0.5);
        let (at, pose) = animal_dismount_location(&animal, &rider(0.0), &sea);
        assert_eq!(pose, Some(Pose::Standing));
        assert_eq!(at, Vec3::new(-0.5, 30.0, 0.5));
        let (at, pose) = animal_dismount_location(&animal, &rider(0.0), &Sea::with(&[]));
        assert_eq!((at, pose), (on_top(&animal), None));
    }

    #[test]
    fn boat_seats_by_index() {
        let at = |yaw: f32, index: usize| {
            super::super::boats::Boat::new(1, "oak".into(), Vec3::ZERO, yaw)
                .seat(index, 2, false, 0.0, false)
        };
        // AbstractBoat.getPassengerAttachmentPoint: 0.2 ahead for the first, 0.6 behind the second.
        let (front, back) = (at(0.0, 0), at(0.0, 1));
        assert!((front.z - 0.2).abs() < 1e-6 && front.x.abs() < 1e-6);
        assert!((back.z + 0.6).abs() < 1e-6 && back.x.abs() < 1e-6);
        let (front, back) = (at(90.0, 0), at(90.0, 1));
        assert!((front.x + 0.2).abs() < 1e-6 && front.z.abs() < 1e-6);
        assert!((back.x - 0.6).abs() < 1e-6 && back.z.abs() < 1e-6);
    }
}
