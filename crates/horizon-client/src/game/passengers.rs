//! Mobs riding mobs: boarding and dismounting, keeping riders on their
//! vehicles, the jockeys that spawn mounted (chicken jockeys, spider
//! jockeys, strider jockeys, zombie horsemen) and the skeleton horse trap.
//! Saving riders under `Passengers` lives in `persist.rs`.
use super::{
    Game,
    entity_life::Removal,
    vehicle_rules::{Rider, Seat, Vehicle},
};
use horizon_client::{
    mobs::{Mob, Species, Who, riders},
    riding::Ent,
};
use horizon_core::{geometry::Vec3, random::RandomSource};
use std::collections::HashMap;

/// `Zombie.finalizeSpawn`: a baby zombie takes a chicken one time in twenty,
/// and another one time in twenty of the rest.
const CHICKEN_JOCKEY_ODDS: f32 = 0.05;
/// The seconds a skeleton horse trap waits for a player: 15 minutes.
const TRAP_LIFETIME: i32 = 18000;
/// `SkeletonTrapGoal`: a player this close springs the trap.
const TRAP_RANGE: f64 = 10.0;
/// Ticks of invulnerability the trap's riders and horses start with.
const TRAP_INVULNERABLE: i32 = 60;

fn zombie_kind(s: Species) -> bool {
    matches!(
        s,
        Species::Zombie | Species::Husk | Species::Drowned | Species::ZombieVillager
    )
}

impl Game<'_> {
    /// `Player.getMainArm` of the player this game acts as: a guest's `ClientInformation`, else the window's option.
    pub(super) fn acting_main_arm_left(&self) -> bool {
        if self.lan_acting.is_some() {
            self.lan_acting_main_arm_left
        } else {
            self.settings.main_hand_left
        }
    }
    /// What a renderer asks of mob `m`'s riding: from `SetPassengers` on a client, else from the table.
    pub(super) fn ride_state(&self, m: &Mob) -> horizon_client::mobs::anim::Ride {
        use horizon_client::mobs::anim::Ride;
        match self.remote.as_ref() {
            Some(r) => {
                let e = r.client_ent(m.id as i32);
                Ride {
                    is_passenger: r.passengers.is_passenger(e),
                    is_ridden: r.passengers.is_vehicle(e),
                }
            }
            None => Ride {
                is_passenger: self.is_passenger(Ent::Id(m.id)),
                is_ridden: m.rider.is_some() || self.is_vehicle(Ent::Id(m.id)),
            },
        }
    }
    /// `Entity.getVehicle` of mob `id`: from `SetPassengers` on a client, else from the table.
    pub(super) fn mob_vehicle(&self, id: u32) -> Option<u32> {
        match self.remote.as_ref() {
            Some(r) => r.passengers.vehicle_of(Ent::Id(id)).and_then(Ent::id),
            None => self.vehicle_of(Ent::Id(id)).and_then(Ent::id),
        }
    }
    /// `Entity.getFirstPassenger` of mob `id`, as an entity id.
    pub(super) fn mob_first_passenger(&self, id: u32) -> Option<u32> {
        match self.remote.as_ref() {
            Some(r) => match r.passengers.first_passenger(Ent::Id(id))? {
                Ent::Id(p) => Some(p),
                _ => Some(r.entity_id as u32),
            },
            None => self.first_passenger(Ent::Id(id)).and_then(Ent::id),
        }
    }
    /// `Entity.isVehicle` of vehicle `id` (a cart, say): something rides it.
    pub(super) fn has_passengers(&self, id: u32) -> bool {
        match self.remote.as_ref() {
            Some(r) => r.passengers.is_vehicle(Ent::Id(id)),
            None => self.is_vehicle(Ent::Id(id)),
        }
    }
    /// `isPassenger` and `isVehicle` of mob `id` in the level's table, for a damage that asks.
    pub(super) fn mob_ride(&self, id: u32) -> horizon_client::mobs::anim::Ride {
        horizon_client::mobs::anim::Ride::of(&self.riding_table, id)
    }
    /// `Entity.getVehicle` in the level's table.
    pub(super) fn vehicle_of(&self, e: Ent) -> Option<Ent> {
        self.riding_table.borrow().vehicle_of(e)
    }
    /// `Entity.getPassengers` in the level's table.
    pub(super) fn passengers_of(&self, e: Ent) -> Vec<Ent> {
        self.riding_table.borrow().passengers_of(e).to_vec()
    }
    pub(super) fn first_passenger(&self, e: Ent) -> Option<Ent> {
        self.riding_table.borrow().first_passenger(e)
    }
    pub(super) fn is_passenger(&self, e: Ent) -> bool {
        self.riding_table.borrow().is_passenger(e)
    }
    pub(super) fn is_vehicle(&self, e: Ent) -> bool {
        self.riding_table.borrow().is_vehicle(e)
    }
    /// `Entity.getRootVehicle`.
    pub(super) fn root_vehicle(&self, e: Ent) -> Ent {
        self.riding_table.borrow().root_vehicle(e)
    }
    /// The vehicle with entity id `id`, by its place in the vector of its kind.
    fn vehicle_target(&self, id: u32) -> Option<Vehicle> {
        if let Some(i) = self.mobs.iter().position(|m| m.id == id) {
            Some(Vehicle::Mob(i))
        } else if let Some(i) = self.boats.iter().position(|b| b.id == id) {
            Some(Vehicle::Boat(i))
        } else {
            self.carts
                .iter()
                .position(|c| c.id == id)
                .map(Vehicle::Cart)
        }
    }
    /// What `Mob.startRiding` does after the link, for a mob that linked itself in its turn.
    pub(super) fn boarded(&mut self, rider: u32, vehicle: u32) {
        self.break_leash(rider, true, false);
        if let Some(at) = self.vehicle_position(vehicle) {
            let ctx = self.who_context(Some(Who::Mob(rider)), None);
            self.post_game_event("minecraft:entity_mount", at, ctx);
        }
    }
    /// `Entity.startRiding(v, force, events)` for mob `r`; whether it boarded.
    pub(super) fn start_riding(&mut self, r: Ent, v: Ent, force: bool, events: bool) -> bool {
        let (Ent::Id(rider), Ent::Id(mut vehicle_id)) = (r, v) else {
            return false;
        };
        let mut v = v;
        // Piglin.startRiding: a baby piglin boards the top of the piglins on a hoglin.
        if let Some(m) = self.mobs.iter().find(|m| m.id == rider)
            && m.species() == Species::Piglin
            && m.is_baby()
            && self
                .mobs
                .iter()
                .any(|o| o.id == vehicle_id && o.species() == Species::Hoglin)
        {
            v = riders::top_passenger(
                &self.riding_table.borrow(),
                v,
                riders::MAX_PASSENGERS_ON_ONE_HOGLIN,
            );
            vehicle_id = v.id().unwrap_or(vehicle_id);
        }
        if r == v || self.vehicle_of(r) == Some(v) {
            return false;
        }
        let Some(target) = self.vehicle_target(vehicle_id) else {
            return false;
        };
        if !self.could_accept_passenger(target) {
            return false;
        }
        if self.riding_table.borrow().rides_through(v, r) {
            return false;
        }
        let can_ride = self.riding_table.borrow().boarding_cooldown(r) <= 0;
        if !(force || can_ride && self.can_add_passenger(target, &self.passengers_of(v))) {
            return false;
        }
        if self.is_passenger(r) {
            self.stop_riding(r);
        }
        self.riding_table.borrow_mut().link(r, v);
        // Mob.startRiding: a leashed mob lets go of its lead.
        self.break_leash(rider, true, false);
        if events && let Some(at) = self.vehicle_position(vehicle_id) {
            let ctx = self.who_context(Some(Who::Mob(rider)), None);
            self.post_game_event("minecraft:entity_mount", at, ctx);
        }
        true
    }
    /// `Entity.stopRiding` (`removeVehicle`, then `LivingEntity.dismountVehicle`).
    pub(super) fn stop_riding(&mut self, r: Ent) {
        self.remove_vehicle(r, None, false);
    }
    /// `removeVehicle` of `r`, which is being removed for `removal` or, with None, is staying: it gets
    /// off, and a rider that stays is put down where its vehicle lets it out.
    fn remove_vehicle(&mut self, r: Ent, removal: Option<Removal>, vehicle_gone: bool) {
        let Some(old) = self.riding_table.borrow_mut().unlink(r) else {
            return;
        };
        let (Ent::Id(rider), Ent::Id(old_id)) = (r, old) else {
            return;
        };
        // HappyGhast.removePassenger.
        if removal.is_none()
            && self
                .mobs
                .iter()
                .any(|m| m.id == old_id && m.species() == Species::HappyGhast)
        {
            self.happy_ghast_passenger_left(old_id);
        }
        if removal.is_none_or(Removal::destroys)
            && let Some(at) = self.vehicle_position(old_id)
        {
            let ctx = self.who_context(Some(Who::Mob(rider)), None);
            self.post_game_event("minecraft:entity_dismount", at, ctx);
        }
        if removal.is_some() {
            return;
        }
        // LivingEntity.dismountVehicle.
        let (Some(i), Some(target)) = (
            self.mobs.iter().position(|m| m.id == rider),
            self.vehicle_target(old_id),
        ) else {
            return;
        };
        let at = if vehicle_gone {
            // The vehicle is being removed: the rider stays, no lower than the vehicle.
            let (p, vehicle_y) = (self.mobs[i].position, self.vehicle_position(old_id));
            Vec3::new(p.x, vehicle_y.map_or(p.y, |v| p.y.max(v[1])), p.z)
        } else {
            self.dismount_location(target, &Rider::mob(&self.mobs[i])).0
        };
        let m = &mut self.mobs[i];
        m.position = at;
        m.previous = at;
    }
    /// `Entity.ejectPassengers`: last to first.
    pub(super) fn eject_passengers(&mut self, v: Ent) {
        for p in self.passengers_of(v).into_iter().rev() {
            self.stop_riding(p);
        }
    }
    /// `Entity.setRemoved(why)`: the entity gets off its vehicle when `why` destroys it, its passengers
    /// get off it, and the table forgets it.
    pub(super) fn riding_removed(&mut self, e: Ent, why: Removal) {
        if !self.is_passenger(e) && !self.is_vehicle(e) {
            return;
        }
        if why.destroys() {
            self.remove_vehicle(e, Some(why), false);
        }
        for p in self.passengers_of(e) {
            self.remove_vehicle(p, None, true);
        }
        self.riding_table.borrow_mut().forget(e);
    }
    /// The level's table dropped the entities of a whole kind at once.
    pub(super) fn riding_forget(&mut self, e: Ent) {
        self.riding_table.borrow_mut().forget(e);
    }
    /// Riders sit on their vehicles (`Entity.positionRider`), mobs or boats.
    pub(super) fn tick_passengers(&mut self) {
        // Entity.baseTick.
        self.riding_table.borrow_mut().tick_cooldowns();
        self.position_riders();
    }
    /// `Entity.positionRider` for every mob passenger: in its boat, then on
    /// its mob. A server's client places them itself too, as the server sends
    /// no passenger's position.
    pub(super) fn position_riders(&mut self) {
        self.seat_boat_mobs();
        let index: HashMap<u32, usize> = self
            .mobs
            .iter()
            .enumerate()
            .map(|(i, m)| (m.id, i))
            .collect();
        if let Some(r) = self.remote.as_ref() {
            // Roots first, so a rider of a rider is placed after its vehicle.
            let pairs: Vec<(usize, usize, usize)> = r
                .passengers
                .riders_in_order()
                .into_iter()
                .filter_map(|(rider, vehicle)| {
                    let (Ent::Id(rider_id), Ent::Id(vehicle_id)) = (rider, vehicle) else {
                        return None;
                    };
                    let n = r
                        .passengers
                        .passengers_of(vehicle)
                        .iter()
                        .position(|&p| p == rider)?;
                    Some((*index.get(&rider_id)?, *index.get(&vehicle_id)?, n))
                })
                .collect();
            for (i, j, n) in pairs {
                self.seat_on_mob(i, j, n, true);
            }
        } else {
            // Roots first, so a rider of a rider is placed after its vehicle.
            let pairs: Vec<(usize, usize, usize)> = {
                let table = self.riding_table.borrow();
                table
                    .riders_in_order()
                    .into_iter()
                    .filter_map(|(rider, vehicle)| {
                        let (Ent::Id(rider_id), Ent::Id(vehicle_id)) = (rider, vehicle) else {
                            return None;
                        };
                        let n = table
                            .passengers_of(vehicle)
                            .iter()
                            .position(|&p| p == rider)?;
                        Some((*index.get(&rider_id)?, *index.get(&vehicle_id)?, n))
                    })
                    .collect()
            };
            for (i, j, n) in pairs {
                self.seat_on_mob(i, j, n, false);
            }
        }
        // Displays and interaction boxes riding anything the client tracks,
        // as NPC plugins put a name over a head or a model's bones on a
        // pivot: `Entity.rideTick` -> `positionRider`, the vehicle's passenger
        // attachment (the top of its box in its pose) less the rider's own,
        // which is 0 for both (Display and Interaction are 0 by 0). On the
        // local player, the top of its box in its pose and scale
        // (`Avatar.POSES`).
        let Some(r) = self.remote.as_ref() else {
            return;
        };
        let riding = |id: i32| r.passengers.vehicle_of(Ent::Id(id as u32));
        let displays: Vec<(i32, Ent)> = self
            .text_displays
            .keys()
            .filter_map(|&id| riding(id).map(|v| (id, v)))
            .collect();
        let interactions: Vec<(i32, Ent)> = self
            .interactions
            .keys()
            .filter_map(|&id| riding(id).map(|v| (id, v)))
            .collect();
        let own_seat = self.player.as_ref().map(|p| {
            p.position
                .add_vector(Vec3::new(0.0, f64::from(p.dimensions.height), 0.0))
        });
        // A rider of a rider settles over a few passes, as the mobs above.
        for _ in 0..3 {
            let mut vehicles: Vec<Ent> = displays
                .iter()
                .chain(&interactions)
                .map(|&(_, v)| v)
                .collect();
            vehicles.sort_unstable();
            vehicles.dedup();
            let seats: HashMap<Ent, glam::DVec3> = vehicles
                .into_iter()
                .filter_map(|v| {
                    self.passenger_riding_position(v, own_seat, &index)
                        .map(|at| (v, at))
                })
                .collect();
            for (id, v) in &displays {
                if let (Some(at), Some(display)) = (seats.get(v), self.text_displays.get_mut(id)) {
                    display.ride(*at);
                }
            }
            for (id, v) in &interactions {
                if let (Some(at), Some(interaction)) = (seats.get(v), self.interactions.get_mut(id))
                {
                    interaction.ride_to(*at);
                }
            }
        }
    }
    /// `Entity.positionRider` of mob `i` on mob `j`, as its passenger number `n`.
    fn seat_on_mob(&mut self, i: usize, j: usize, n: usize, client: bool) {
        let vehicle = &self.mobs[j];
        let attach = |yaw| riders::passenger_attachment(vehicle, n, yaw, client);
        let (pos, prev) = (vehicle.position, vehicle.previous);
        let (yaw, prev_yaw) = (vehicle.yaw, vehicle.previous_yaw);
        let (at, prev_at) = (attach(yaw), attach(prev_yaw));
        let width = vehicle.width();
        let (body, prev_body) = (vehicle.body_yaw, vehicle.previous_body_yaw);
        let m = &mut self.mobs[i];
        let offset = riders::vehicle_attachment(m, width);
        m.position = riders::ride_position(pos, at, offset);
        m.previous = riders::ride_position(prev, prev_at, offset);
        m.velocity = Vec3::ZERO;
        m.fall_distance = 0.0;
        m.on_ground = false;
        m.body_yaw = body;
        m.previous_body_yaw = prev_body;
        // A rider never walks off on its own.
        m.ai.nav.stop();
    }

    /// `Entity.getPassengerRidingPosition` of a vehicle: where the feet of a
    /// passenger whose own vehicle attachment is 0 land.
    fn passenger_riding_position(
        &self,
        vehicle: Ent,
        own_seat: Option<Vec3>,
        index: &HashMap<u32, usize>,
    ) -> Option<glam::DVec3> {
        let Ent::Id(vehicle) = vehicle else {
            let at = own_seat?;
            return Some(glam::DVec3::new(at.x, at.y, at.z));
        };
        if let Some(&j) = index.get(&vehicle) {
            let v = &self.mobs[j];
            let attachment = riders::passenger_attachment(v, 0, v.yaw, self.remote.is_some());
            let at = riders::ride_position(v.position, attachment, [0.0; 3]);
            return Some(glam::DVec3::new(at.x, at.y, at.z));
        }
        if let Some(d) = self.text_displays.get(&(vehicle as i32)) {
            return Some(d.position());
        }
        if let Some(i) = self.interactions.get(&(vehicle as i32)) {
            let at = i.position;
            return Some(glam::DVec3::new(at.x, at.y, at.z));
        }
        if let Some(b) = self.boats.iter().find(|b| b.id == vehicle) {
            let count = self.boat_players(vehicle).max(1) + self.boat_mobs(vehicle).len();
            let at = b.seat(0, count, false, 0.0, false);
            return Some(glam::DVec3::new(at.x, at.y, at.z));
        }
        if let Some(i) = self.carts.iter().position(|c| c.id == vehicle) {
            let at = self.passenger_point(Vehicle::Cart(i), 0, 1, Seat::PLAYER, false);
            return Some(glam::DVec3::new(at.x, at.y, at.z));
        }
        if let Some(a) = self.armor_stands.iter().find(|a| a.id == vehicle) {
            // ArmorStand.getDefaultDimensions: a marker has no box to sit on.
            let height = if a.marker {
                0.0
            } else if a.small {
                0.9875
            } else {
                1.975
            };
            return Some(glam::DVec3::new(
                a.position.x,
                a.position.y + height,
                a.position.z,
            ));
        }
        // Any other tracked entity sits on its feet; an area effect cloud is
        // 0.5 tall, and ModelEngine parks a model's pivot half a block below
        // its feet for exactly that.
        let (kind, position) = self.remote.as_ref()?.tracked(vehicle as i32)?;
        let lift = if kind == "minecraft:area_effect_cloud" {
            0.5
        } else {
            0.0
        };
        Some(glam::DVec3::new(
            position[0],
            position[1] + lift,
            position[2],
        ))
    }

    /// `finalizeSpawn` jockeys of a naturally spawned mob: baby zombies on
    /// chickens, skeletons on spiders, piglins and baby striders on striders,
    /// zombies on zombie horses.
    pub(super) fn spawn_jockeys(&mut self, id: u32, natural: bool) {
        let Some(m) = self.mobs.iter().find(|m| m.id == id) else {
            return;
        };
        let (species, baby) = (m.species(), m.is_baby());
        let at = [m.position.x, m.position.y, m.position.z];
        match species {
            s if zombie_kind(s) && baby => {
                if self.world.random.next_float() < CHICKEN_JOCKEY_ODDS {
                    if let Some(chicken) = self.free_chicken_near(at) {
                        self.make_chicken_jockey(chicken);
                        // Zombie.finalizeSpawn: this.startRiding(chicken, false, false).
                        self.start_riding(Ent::Id(id), Ent::Id(chicken), false, false);
                    }
                } else if self.world.random.next_float() < CHICKEN_JOCKEY_ODDS
                    && let Some(chicken) = self.spawn_mob("minecraft:chicken", at, false)
                {
                    self.make_chicken_jockey(chicken);
                    // Zombie.finalizeSpawn: this.startRiding(chicken, false, false).
                    self.start_riding(Ent::Id(id), Ent::Id(chicken), false, false);
                }
            }
            // Spider.finalizeSpawn: one in a hundred carries a skeleton.
            Species::Spider if self.world.random.next_int_bound(100).unwrap_or(1) == 0 => {
                if let Some(skeleton) = self.spawn_mob("minecraft:skeleton", at, false) {
                    // Spider.finalizeSpawn: skeleton.startRiding(this, false, false).
                    self.start_riding(Ent::Id(skeleton), Ent::Id(id), false, false);
                }
            }
            Species::Strider if !baby => self.spawn_strider_jockey(id, at),
            // ZombieHorse.finalizeSpawn: a zombie with an iron spear. The rider
            // comes from EntityType.create, which makes no zombie in peaceful
            // (canSpawn), so a peaceful zombie horse spawns without one.
            Species::ZombieHorse
                if natural
                    && (self.ui.difficulty != 0
                        || horizon_client::mobs::spawn_rules::allowed_in_peaceful(
                            Species::Zombie,
                        )) =>
            {
                if let Some(zombie) = self.spawn_mob("minecraft:zombie", at, false) {
                    if let Some(z) = self.mobs.iter_mut().find(|m| m.id == zombie) {
                        z.extra.set_main_hand(Some("minecraft:iron_spear".into()));
                    }
                    // ZombieHorse.finalizeSpawn: zombie.startRiding(this, false, false).
                    self.start_riding(Ent::Id(zombie), Ent::Id(id), false, false);
                }
            }
            _ => {}
        }
        // After Zombie.finalizeSpawn's chickens: Husk and Drowned mounts.
        match species {
            Species::Husk if natural => self.spawn_camel_husk_jockey(id, at),
            Species::Drowned if natural && !baby => self.spawn_zombie_nautilus_jockey(id, at),
            _ => {}
        }
    }
    /// `Husk.finalizeSpawn`: where a camel husk fits, one natural husk in ten
    /// takes an iron spear and rides one (with a parched; Horizon seats one
    /// rider, so the parched stands beside it).
    fn spawn_camel_husk_jockey(&mut self, id: u32, at: [f64; 3]) {
        let (bx, by, bz) = (at[0].floor(), at[1].floor(), at[2].floor());
        let room = {
            let shared = self.mob_shared();
            let w = self.mob_world(&shared, &[]);
            w.no_collision(horizon_core::geometry::Aabb::new(
                Vec3::new(bx + 0.5 - 0.85, by, bz + 0.5 - 0.85),
                Vec3::new(bx + 0.5 + 0.85, by + 2.375, bz + 0.5 + 0.85),
            ))
        };
        if !room || self.world.random.next_float() >= 0.1 {
            return;
        }
        if let Some(h) = self.mobs.iter_mut().find(|m| m.id == id) {
            h.extra.set_main_hand(Some("minecraft:iron_spear".into()));
        }
        let Some(camel) = self.spawn_mob("minecraft:camel_husk", at, false) else {
            return;
        };
        // Husk.finalizeSpawn: this.startRiding(camelHusk, true, true).
        self.start_riding(Ent::Id(id), Ent::Id(camel), true, true);
        let _ = self.spawn_mob("minecraft:parched", at, false);
    }
    /// `Drowned.finalizeSpawn`: half the natural drowned holding a trident,
    /// outside the rivers and oceans that breed more drowned, ride a zombie
    /// nautilus.
    fn spawn_zombie_nautilus_jockey(&mut self, id: u32, at: [f64; 3]) {
        let Some(d) = self.mobs.iter().find(|m| m.id == id) else {
            return;
        };
        if d.extra.main_hand.as_deref() != Some("minecraft:trident")
            || self.world.random.next_float() >= 0.5
        {
            return;
        }
        let (x, y, z) = (
            at[0].floor() as i32,
            at[1].floor() as i32,
            at[2].floor() as i32,
        );
        let frequent = self.world.chunks.get(&(x >> 4, z >> 4)).is_some_and(|c| {
            let b = self.res.biomes().get(c.biome(x, y, z));
            self.res
                .mob_data()
                .biome_has("minecraft:more_frequent_drowned_spawns", &b.name)
        });
        if frequent {
            return;
        }
        if let Some(n) = self.spawn_mob("minecraft:zombie_nautilus", at, false) {
            // Drowned.finalizeSpawn: this.startRiding(zombieNautilus, false, false).
            self.start_riding(Ent::Id(id), Ent::Id(n), false, false);
        }
    }
    fn free_chicken_near(&self, at: [f64; 3]) -> Option<u32> {
        self.mobs
            .iter()
            .find(|c| {
                c.species() == Species::Chicken
                    && c.alive()
                    && !self.is_vehicle(Ent::Id(c.id))
                    && !self.is_passenger(Ent::Id(c.id))
                    && (c.position.x - at[0]).abs() <= 5.3
                    && (c.position.y - at[1]).abs() <= 3.0 + 1.0
                    && (c.position.z - at[2]).abs() <= 5.3
            })
            .map(|c| c.id)
    }
    fn make_chicken_jockey(&mut self, chicken: u32) {
        if let Some(c) = self.mobs.iter_mut().find(|m| m.id == chicken) {
            c.extra.jockey = true;
        }
    }
    /// `Strider.finalizeSpawn`: one strider in thirty carries a zombified
    /// piglin holding a warped fungus on a stick (and wears a saddle), one in
    /// ten of the rest a baby strider.
    fn spawn_strider_jockey(&mut self, id: u32, at: [f64; 3]) {
        if self.world.random.next_int_bound(30).unwrap_or(1) == 0 {
            if !self.ui.rules.bool("spawn_mobs") {
                return;
            }
            if let Some(piglin) = self.spawn_mob("minecraft:zombified_piglin", at, false) {
                let baby = self.world.random.next_float() < CHICKEN_JOCKEY_ODDS;
                if let Some(p) = self.mobs.iter_mut().find(|m| m.id == piglin) {
                    p.extra
                        .set_main_hand(Some("minecraft:warped_fungus_on_a_stick".into()));
                    p.extra.age = if baby { i32::MIN / 2 } else { 0 };
                }
                if let Some(s) = self.mobs.iter_mut().find(|m| m.id == id) {
                    s.extra.saddled = true;
                }
                // Strider.spawnJockey: jockey.startRiding(this, true, false).
                self.start_riding(Ent::Id(piglin), Ent::Id(id), true, false);
            }
        } else if self.world.random.next_int_bound(10).unwrap_or(1) == 0
            && let Some(baby) = self.spawn_mob("minecraft:strider", at, false)
        {
            if let Some(b) = self.mobs.iter_mut().find(|m| m.id == baby) {
                b.extra.age = -24000;
            }
            // Strider.spawnJockey: jockey.startRiding(this, true, false).
            self.start_riding(Ent::Id(baby), Ent::Id(id), true, false);
        }
    }

    /// `SkeletonHorse.customServerAiStep` and `SkeletonTrapGoal`: a trap
    /// horse is a trap for fifteen minutes; a player within ten blocks springs
    /// it.
    pub(super) fn tick_skeleton_traps(&mut self) {
        // `Level.hasNearbyAlivePlayer`: alive, not a spectator, in the level.
        let players: Vec<_> = self
            .level_players()
            .into_iter()
            .filter(|p| p.alive && !p.spectator)
            .map(|p| p.position)
            .collect();
        let mut sprung = Vec::new();
        for m in &mut self.mobs {
            if m.species() != Species::SkeletonHorse || !m.extra.trap || !m.alive() {
                continue;
            }
            if !m.persistent {
                m.extra.trap_time += 1;
                if m.extra.trap_time > TRAP_LIFETIME {
                    m.removed = true;
                    continue;
                }
            }
            if players
                .iter()
                .any(|&p| m.distance_sqr(p) < TRAP_RANGE * TRAP_RANGE)
            {
                sprung.push(m.id);
            }
        }
        for id in sprung {
            self.spring_skeleton_trap(id);
        }
    }
    fn spring_skeleton_trap(&mut self, id: u32) {
        let Some(horse) = self.mobs.iter_mut().find(|m| m.id == id) else {
            return;
        };
        horse.extra.trap = false;
        horse.extra.tame = true;
        horse.extra.age = 0;
        let at = [horse.position.x, horse.position.y, horse.position.z];
        self.spawn_bolt(at, true);
        let Some(skeleton) = self.trap_skeleton(at) else {
            return;
        };
        // SkeletonTrapGoal.tick: skeleton.startRiding(horse), force false with events.
        self.start_riding(Ent::Id(skeleton), Ent::Id(id), false, true);
        for _ in 0..3 {
            let Some(other) = self.spawn_mob("minecraft:skeleton_horse", at, true) else {
                continue;
            };
            let (dx, dz) = (
                self.world.random.triangle(0.0, 1.1485),
                self.world.random.triangle(0.0, 1.1485),
            );
            if let Some(h) = self.mobs.iter_mut().find(|m| m.id == other) {
                h.invulnerable = TRAP_INVULNERABLE;
                h.extra.tame = true;
                h.extra.age = 0;
                h.velocity = Vec3::new(h.velocity.x + dx, h.velocity.y, h.velocity.z + dz);
            }
            if let Some(rider) = self.trap_skeleton(at) {
                // SkeletonTrapGoal.tick: otherSkeleton.startRiding(otherHorse), force false with events.
                self.start_riding(Ent::Id(rider), Ent::Id(other), false, true);
            }
        }
    }
    /// `SkeletonTrapGoal.createSkeleton`: an iron helmet unless it already
    /// has a helm.
    fn trap_skeleton(&mut self, at: [f64; 3]) -> Option<u32> {
        let id = self.spawn_mob("minecraft:skeleton", at, true)?;
        let m = self.mobs.iter_mut().find(|m| m.id == id)?;
        m.invulnerable = TRAP_INVULNERABLE;
        if m.extra.head.is_none() {
            m.extra.head = Some("minecraft:iron_helmet".into());
        }
        let (weapon, helmet) = (m.extra.main_hand.clone(), m.extra.head.clone());
        let special = self.local_difficulty(at).special();
        // `SkeletonTrapGoal.enchant`: the weapon and the helmet, always.
        for (slot, item) in [
            (horizon_client::mobs::gear::MAIN, weapon),
            (horizon_client::mobs::gear::HEAD, helmet),
        ] {
            if let Some(item) = item {
                self.enchant_item(id, slot, &item, special);
            }
        }
        Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::super::crossing_tests::flat_game;
    use super::*;

    fn spawn(game: &mut Game<'_>, kind: &str, at: [f64; 3]) -> u32 {
        game.spawn_mob(kind, at, true).unwrap()
    }
    fn at(game: &Game<'_>, id: u32) -> Vec3 {
        game.mobs.iter().find(|m| m.id == id).unwrap().position
    }

    #[test]
    fn start_riding_refuses_a_loop_a_player_vehicle_and_a_full_vehicle() {
        let Some(mut game) = flat_game() else { return };
        let pig = spawn(&mut game, "pig", [2.5, 61.0, 2.5]);
        let zombie = spawn(&mut game, "zombie", [2.5, 61.0, 2.5]);
        let skeleton = spawn(&mut game, "skeleton", [2.5, 61.0, 2.5]);
        let (pig, zombie, skeleton) = (Ent::Id(pig), Ent::Id(zombie), Ent::Id(skeleton));
        assert!(game.start_riding(zombie, pig, false, false));
        assert!(
            !game.start_riding(zombie, pig, false, false),
            "already riding it"
        );
        assert!(!game.start_riding(pig, zombie, false, false), "a loop");
        assert!(!game.start_riding(pig, pig, false, false), "itself");
        assert!(
            !game.start_riding(skeleton, pig, false, false),
            "a pig takes one"
        );
        assert!(
            !game.start_riding(skeleton, Ent::Host, true, false),
            "players are no vehicle"
        );
        assert_eq!(game.passengers_of(pig), vec![zombie]);
    }

    #[test]
    fn force_skips_can_ride_and_can_add() {
        let Some(mut game) = flat_game() else { return };
        let pig = Ent::Id(spawn(&mut game, "pig", [2.5, 61.0, 2.5]));
        let zombie = Ent::Id(spawn(&mut game, "zombie", [2.5, 61.0, 2.5]));
        let skeleton = Ent::Id(spawn(&mut game, "skeleton", [2.5, 61.0, 2.5]));
        assert!(game.start_riding(zombie, pig, false, false));
        assert!(game.start_riding(skeleton, pig, true, false));
        assert_eq!(game.passengers_of(pig), vec![zombie, skeleton]);
        // Getting off starts the 60 ticks no unforced boarding waits out.
        game.stop_riding(skeleton);
        assert!(!game.start_riding(skeleton, pig, false, false));
        game.stop_riding(zombie);
        assert!(!game.start_riding(skeleton, pig, false, false));
        assert!(game.start_riding(skeleton, pig, true, false));
    }

    #[test]
    fn a_killed_vehicle_drops_its_riders_and_an_unloaded_one_keeps_quiet() {
        let Some(mut game) = flat_game() else { return };
        let pig = spawn(&mut game, "pig", [2.5, 61.0, 2.5]);
        let zombie = spawn(&mut game, "zombie", [2.5, 60.0, 2.5]);
        assert!(game.start_riding(Ent::Id(zombie), Ent::Id(pig), false, false));
        game.riding_removed(Ent::Id(pig), Removal::Killed);
        assert!(!game.is_passenger(Ent::Id(zombie)));
        assert!(
            at(&game, zombie).y >= 61.0,
            "set down no lower than the pig"
        );
        // A rider removed because its chunk unloads is not put anywhere.
        let cow = spawn(&mut game, "cow", [8.5, 61.0, 8.5]);
        let skeleton = spawn(&mut game, "skeleton", [8.5, 61.0, 8.5]);
        assert!(game.start_riding(Ent::Id(skeleton), Ent::Id(cow), false, false));
        let before = at(&game, skeleton);
        game.riding_removed(Ent::Id(skeleton), Removal::UnloadedToChunk);
        assert!(!game.is_vehicle(Ent::Id(cow)));
        assert_eq!(at(&game, skeleton), before);
    }

    #[test]
    fn riders_of_riders_sit_on_the_new_position() {
        let Some(mut game) = flat_game() else { return };
        let pig = spawn(&mut game, "pig", [2.5, 61.0, 2.5]);
        let zombie = spawn(&mut game, "zombie", [2.5, 61.0, 2.5]);
        let skeleton = spawn(&mut game, "skeleton", [2.5, 61.0, 2.5]);
        assert!(game.start_riding(Ent::Id(zombie), Ent::Id(pig), false, false));
        assert!(game.start_riding(Ent::Id(skeleton), Ent::Id(zombie), false, false));
        let m = game.mobs.iter_mut().find(|m| m.id == pig).unwrap();
        m.position = Vec3::new(10.5, 61.0, 7.5);
        m.previous = m.position;
        game.tick_passengers();
        let (p, z, s) = (at(&game, pig), at(&game, zombie), at(&game, skeleton));
        assert!((z.x - p.x).abs() < 0.5 && (z.z - p.z).abs() < 0.5);
        assert!((s.x - p.x).abs() < 0.5 && (s.z - p.z).abs() < 0.5);
        assert!(z.y > p.y && s.y > z.y, "{p:?} {z:?} {s:?}");
    }

    #[test]
    fn piglin_rides_the_top_of_a_hoglin_stack() {
        let Some(mut game) = flat_game() else { return };
        let hoglin = Ent::Id(spawn(&mut game, "hoglin", [2.5, 61.0, 2.5]));
        let mut piglins = Vec::new();
        for _ in 0..4 {
            let id = spawn(&mut game, "piglin", [2.5, 61.0, 2.5]);
            game.mobs.iter_mut().find(|m| m.id == id).unwrap().extra.age = -24000;
            piglins.push(Ent::Id(id));
        }
        for &p in &piglins {
            game.start_riding(p, hoglin, false, false);
        }
        // Down the chain from the hoglin, three seats: the fourth finds the third's seat taken.
        assert_eq!(game.passengers_of(hoglin), vec![piglins[0]]);
        assert_eq!(game.passengers_of(piglins[0]), vec![piglins[1]]);
        assert_eq!(game.passengers_of(piglins[1]), vec![piglins[2]]);
        assert!(
            game.passengers_of(piglins[2]).is_empty()
                || game.passengers_of(piglins[2]) == vec![piglins[3]]
        );
    }
}
