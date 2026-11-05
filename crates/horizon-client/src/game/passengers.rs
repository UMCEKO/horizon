//! Mobs riding mobs: boarding and dismounting, keeping riders on their
//! vehicles, the jockeys that spawn mounted (chicken jockeys, spider
//! jockeys, strider jockeys, zombie horsemen) and the skeleton horse trap.
//! Saving riders under `Passengers` lives in `persist.rs`.
use super::{
    Game,
    entity_life::Removal,
    lan_actions::Outgoing,
    vehicle_rules::{Rider, Seat, Vehicle},
};
use horizon_client::{
    mobs::{Mob, Species, Who, riders},
    riding::Ent,
};
use horizon_core::{geometry::Vec3, random::RandomSource};
use std::collections::HashMap;
use winit::keyboard::KeyCode;

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
                is_ridden: self.is_vehicle(Ent::Id(m.id)),
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
    pub(super) fn vehicle_target(&self, id: u32) -> Option<Vehicle> {
        if let Some(i) = self.mobs.iter().position(|m| m.id == id) {
            Some(Vehicle::Mob(i))
        } else if let Some(i) = self.boats.iter().position(|b| b.id == id) {
            Some(Vehicle::Boat(i))
        } else {
            self.carts
                .iter()
                .position(|c| c.id == id)
                .map(Vehicle::Cart)
                .or_else(|| {
                    self.cushions
                        .iter()
                        .position(|c| c.id == id)
                        .map(Vehicle::Cushion)
                })
                .or_else(|| self.is_plain_vehicle(id).then_some(Vehicle::Plain(id)))
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
    /// The `canRide` overrides that say no: the wither, the warden and the dragon.
    fn never_rides(&self, r: Ent) -> bool {
        let Some(id) = r.id() else {
            return false;
        };
        self.dragon.as_ref().is_some_and(|d| d.id == id)
            || self.mobs.iter().any(|m| {
                m.id == id
                    && matches!(
                        &*m.kind.id,
                        "minecraft:wither" | "minecraft:warden" | "minecraft:ender_dragon"
                    )
            })
    }
    /// `Entity.startRiding(v, force, events)`; whether it boarded.
    pub(super) fn start_riding(&mut self, r: Ent, v: Ent, force: bool, events: bool) -> bool {
        let Ent::Id(mut vehicle_id) = v else {
            return false;
        };
        let mut v = v;
        // Piglin.startRiding: a baby piglin boards the top of the piglins on a hoglin.
        if let Some(m) = r.id().and_then(|id| self.mobs.iter().find(|m| m.id == id))
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
        let target = self.vehicle_target(vehicle_id);
        if target.is_none() {
            return false;
        }
        if target.is_some_and(|t| !self.could_accept_passenger(t)) {
            return false;
        }
        if self.riding_table.borrow().rides_through(v, r) {
            return false;
        }
        // Entity.canRide: not crouching (the acting player's sneak key), and not just off a vehicle.
        let crouching =
            r.is_player() && r == self.acting_ent() && self.keys.contains(&KeyCode::ShiftLeft);
        let can_ride = !crouching
            && self.riding_table.borrow().boarding_cooldown(r) <= 0
            && !self.never_rides(r);
        let room = target.is_none_or(|t| self.can_add_passenger(t, &self.passengers_of(v)));
        if !(force || can_ride && room) {
            return false;
        }
        if self.is_passenger(r) {
            self.stop_riding(r);
        }
        self.riding_table.borrow_mut().link(r, v);
        if let Some(rider) = r.id() {
            // Mob.startRiding: a leashed mob lets go of its lead.
            self.break_leash(rider, true, false);
        }
        if events {
            if let Some(at) = self.vehicle_position(vehicle_id) {
                let who = r.id().map_or(Who::Player, Who::Mob);
                let ctx = self.who_context(Some(who), None);
                self.post_game_event("minecraft:entity_mount", at, ctx);
            }
            // START_RIDING_TRIGGER for every player on the vehicle, indirect ones included.
            let riders = self.riding_table.borrow().indirect_passengers(v);
            for e in riders.into_iter().filter(|e| e.is_player()) {
                self.trigger_started_riding(e);
            }
        }
        if r.is_player() {
            self.player_boarded(r, vehicle_id);
        }
        true
    }
    /// `CriteriaTriggers.START_RIDING_TRIGGER` for the player `e`, in its own turn when it is not acting.
    fn trigger_started_riding(&mut self, e: Ent) {
        if e == self.acting_ent() {
            self.trigger_player("minecraft:started_riding");
        } else if let Ent::Guest(c) = e
            && let Some(mut host) = self.lan.take()
        {
            self.as_guest(&mut host, c, |g| {
                g.trigger_player("minecraft:started_riding")
            });
            self.lan = Some(host);
        }
    }
    /// `ServerPlayer.startRiding` after the link: the player sits, its client is moved to the seat, told
    /// the vehicle's effects and its passengers.
    fn player_boarded(&mut self, r: Ent, vehicle: u32) {
        let seat = self.player_seat(r, vehicle);
        if let Some(at) = seat {
            self.send_to_player(r, Outgoing::TeleportKeepingLook([at.x, at.y, at.z]));
        }
        if let Some(m) = self.mobs.iter().find(|m| m.id == vehicle) {
            let effects = m.extra.effects.clone();
            if !effects.is_empty() {
                self.send_to_player(
                    r,
                    Outgoing::MobEffects {
                        entity: vehicle,
                        add: true,
                        effects,
                    },
                );
            }
        }
        let passengers = self.passengers_of(Ent::Id(vehicle));
        self.send_to_player(
            r,
            Outgoing::SetPassengers {
                vehicle,
                passengers,
            },
        );
    }
    /// Where player `r` sits in `vehicle` (`Entity.positionRider`); the acting player is moved there.
    fn player_seat(&mut self, r: Ent, vehicle: u32) -> Option<Vec3> {
        let list = self.passengers_of(Ent::Id(vehicle));
        let (index, count) = (
            list.iter().position(|e| *e == r).unwrap_or(0),
            list.len().max(1),
        );
        let at = if let Some(i) = self.boats.iter().position(|b| b.id == vehicle) {
            self.passenger_point(Vehicle::Boat(i), index, count, Seat::PLAYER, false)
        } else if let Some(i) = self.carts.iter().position(|c| c.id == vehicle) {
            self.passenger_point(Vehicle::Cart(i), index, count, Seat::PLAYER, false)
        } else if let Some(i) = self.mobs.iter().position(|m| m.id == vehicle) {
            self.passenger_point(Vehicle::Mob(i), index, count, Seat::PLAYER, false)
        } else if let Some(i) = self.cushions.iter().position(|c| c.id == vehicle) {
            self.passenger_point(Vehicle::Cushion(i), index, count, Seat::PLAYER, false)
        } else {
            self.is_plain_vehicle(vehicle).then(|| {
                self.passenger_point(Vehicle::Plain(vehicle), index, count, Seat::PLAYER, false)
            })?
        };
        if r == self.acting_ent()
            && let Some(p) = self.player.as_mut()
        {
            self.previous_position = p.position;
            p.position = at;
            p.velocity = Vec3::ZERO;
            p.fall_distance = 0.0;
        }
        Some(at)
    }
    /// What `ServerPlayer`'s connection is sent: queued in the acting guest's turn outbox, or held for the
    /// next flush when that guest is not acting. The window's player has no connection.
    pub(super) fn send_to_player(&mut self, e: Ent, item: Outgoing) {
        match e {
            Ent::Guest(c) if self.lan_acting == Some(c) => self.guest_packets.push(item),
            Ent::Guest(c) => self.guest_deferred.push((c, item)),
            _ => {}
        }
    }
    /// `Entity.stopRiding` (`removeVehicle`, then `LivingEntity.dismountVehicle`).
    pub(super) fn stop_riding(&mut self, r: Ent) {
        // A guest that is not acting is set down in its own turn, which tells its client.
        if let Ent::Guest(c) = r
            && self.lan_acting != Some(c)
            && self.is_passenger(r)
            && let Some(mut host) = self.lan.take()
        {
            let done = self.as_guest(&mut host, c, |g| g.remove_vehicle(r, None, false));
            self.lan = Some(host);
            if done.is_some() {
                return;
            }
        }
        self.remove_vehicle(r, None, false);
    }
    /// `Entity.removeVehicle` alone: `r` gets off and stays where it is (a teleport sends it elsewhere).
    pub(super) fn leave_vehicle_in_place(&mut self, r: Ent) {
        self.remove_vehicle(r, None, true);
    }
    /// `removeVehicle` of `r`, which is being removed for `removal` or, with None, is staying: it gets
    /// off, and a rider that stays is put down where its vehicle lets it out.
    fn remove_vehicle(&mut self, r: Ent, removal: Option<Removal>, vehicle_gone: bool) {
        let Some(old) = self.riding_table.borrow_mut().unlink(r) else {
            return;
        };
        let who = r.id().map_or(Who::Player, Who::Mob);
        let Ent::Id(old_id) = old else {
            return;
        };
        // Player.removeVehicle: a player may board again at once.
        if r.is_player() {
            self.riding_table.borrow_mut().clear_cooldown(r);
        }
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
            let ctx = self.who_context(Some(who), None);
            self.post_game_event("minecraft:entity_dismount", at, ctx);
        }
        // ServerPlayer.removeVehicle: the vehicle's effects go, then its passengers are sent.
        if r.is_player() {
            self.player_unboarded(r, old_id);
        }
        if removal.is_some() {
            return;
        }
        // LivingEntity.dismountVehicle.
        let Some(target) = self.vehicle_target(old_id) else {
            return;
        };
        if let Ent::Id(rider) = r {
            let Some(i) = self.mobs.iter().position(|m| m.id == rider) else {
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
        } else if r == self.acting_ent() && !vehicle_gone {
            let Some(p) = self.player.as_ref() else {
                return;
            };
            let rider = Rider::player(p, self.yaw, self.acting_main_arm_left());
            let (spot, pose) = self.dismount_location(target, &rider);
            if let Some(p) = self.player.as_mut() {
                p.position = spot;
                p.velocity = Vec3::ZERO;
                // LivingEntity.setPose: the dismount pose that fit.
                if let Some(pose) = pose
                    && let Err(e) = p.set_pose(pose)
                {
                    log::warn!("dismount pose: {e}");
                }
            }
        }
    }
    /// `ServerPlayer.removeVehicle` after the unlink.
    fn player_unboarded(&mut self, r: Ent, old: u32) {
        if let Some(m) = self.mobs.iter().find(|m| m.id == old) {
            let effects = m.extra.effects.clone();
            if !effects.is_empty() {
                self.send_to_player(
                    r,
                    Outgoing::MobEffects {
                        entity: old,
                        add: false,
                        effects,
                    },
                );
            }
        }
        let passengers = self.passengers_of(Ent::Id(old));
        self.send_to_player(
            r,
            Outgoing::SetPassengers {
                vehicle: old,
                passengers,
            },
        );
        self.send_to_player(r, Outgoing::ClearVehicleGood);
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
        self.position_plain_riders();
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
    use super::super::{crossing_tests::flat_game, entity_life::Kind};
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

    #[test]
    fn a_guest_boarding_gets_set_passengers_and_its_effects() {
        let Some(mut game) = flat_game() else { return };
        let pig = spawn(&mut game, "pig", [2.5, 61.0, 2.5]);
        game.mobs
            .iter_mut()
            .find(|m| m.id == pig)
            .unwrap()
            .extra
            .effects = vec![horizon_client::mobs::effect::MobEffect::new(
            "minecraft:speed",
            100,
            1,
        )];
        game.lan_acting = Some(7);
        assert!(game.start_riding(Ent::Guest(7), Ent::Id(pig), true, false));
        assert_eq!(game.vehicle(), Some(pig));
        let kinds: Vec<&str> = game
            .guest_packets
            .iter()
            .map(|o| match o {
                Outgoing::TeleportKeepingLook(_) => "teleport",
                Outgoing::MobEffects { add: true, .. } => "effects",
                Outgoing::SetPassengers { .. } => "passengers",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["teleport", "effects", "passengers"]);
        game.guest_packets.clear();
        game.stop_riding(Ent::Guest(7));
        let kinds: Vec<&str> = game
            .guest_packets
            .iter()
            .map(|o| match o {
                Outgoing::MobEffects { add: false, .. } => "effects",
                Outgoing::SetPassengers { passengers, .. } if passengers.is_empty() => "passengers",
                Outgoing::ClearVehicleGood => "good",
                _ => "other",
            })
            .collect();
        assert_eq!(kinds, ["effects", "passengers", "good"]);
    }

    #[test]
    fn a_player_leaving_has_no_boarding_cooldown() {
        let Some(mut game) = flat_game() else { return };
        let pig = Ent::Id(spawn(&mut game, "pig", [2.5, 61.0, 2.5]));
        assert!(game.start_riding(Ent::Host, pig, true, false));
        game.stop_riding(Ent::Host);
        assert_eq!(game.riding_table.borrow().boarding_cooldown(Ent::Host), 0);
        assert!(game.start_riding(Ent::Host, pig, false, false));
    }

    #[test]
    fn logout_removes_a_vehicle_with_only_that_player() {
        let Some(mut game) = flat_game() else { return };
        let pig = spawn(&mut game, "pig", [2.5, 61.0, 2.5]);
        assert!(game.start_riding(Ent::Guest(3), Ent::Id(pig), true, false));
        assert!(
            game.riding_table
                .borrow()
                .exactly_one_player_passenger(Ent::Id(pig))
        );
        game.leave_vehicle_in_place(Ent::Guest(3));
        game.level_remove(Kind::Mob, pig, Removal::UnloadedWithPlayer);
        assert!(game.mobs.iter().all(|m| m.id != pig));
        assert!(!game.is_vehicle(Ent::Id(pig)));
    }

    #[test]
    fn a_player_and_a_mob_in_a_boat_keep_the_player_first() {
        let Some(mut game) = flat_game() else { return };
        let pig = Ent::Id(spawn(&mut game, "pig", [2.5, 61.0, 2.5]));
        let zombie = Ent::Id(spawn(&mut game, "zombie", [2.5, 61.0, 2.5]));
        assert!(game.start_riding(zombie, pig, true, false));
        assert!(game.start_riding(Ent::Guest(3), pig, true, false));
        assert_eq!(game.passengers_of(pig), vec![Ent::Guest(3), zombie]);
    }

    #[test]
    fn a_sneaking_player_cannot_board() {
        let Some(mut game) = flat_game() else { return };
        let pig = Ent::Id(spawn(&mut game, "pig", [2.5, 61.0, 2.5]));
        game.keys.insert(KeyCode::ShiftLeft);
        assert!(!game.start_riding(Ent::Host, pig, false, true));
        assert!(!game.is_passenger(Ent::Host));
        game.keys.remove(&KeyCode::ShiftLeft);
        assert!(game.start_riding(Ent::Host, pig, false, true));
    }

    #[test]
    fn a_full_happy_ghast_refuses_the_fifth() {
        let Some(mut game) = flat_game() else { return };
        let ghast = Ent::Id(spawn(&mut game, "happy_ghast", [2.5, 61.0, 2.5]));
        for _ in 0..4 {
            let zombie = Ent::Id(spawn(&mut game, "zombie", [2.5, 61.0, 2.5]));
            assert!(game.start_riding(zombie, ghast, false, false));
        }
        let fifth = Ent::Id(spawn(&mut game, "zombie", [2.5, 61.0, 2.5]));
        assert!(!game.start_riding(fifth, ghast, false, false));
        assert_eq!(game.passengers_of(ghast).len(), 4);
    }

    #[test]
    fn a_camel_takes_three() {
        let Some(mut game) = flat_game() else { return };
        let camel = Ent::Id(spawn(&mut game, "camel", [2.5, 61.0, 2.5]));
        for _ in 0..3 {
            let zombie = Ent::Id(spawn(&mut game, "zombie", [2.5, 61.0, 2.5]));
            assert!(game.start_riding(zombie, camel, false, false));
        }
        let fourth = Ent::Id(spawn(&mut game, "zombie", [2.5, 61.0, 2.5]));
        assert!(!game.start_riding(fourth, camel, false, false));
    }

    #[test]
    fn a_cushion_goes_through_the_vehicle_rules() {
        let Some(mut game) = flat_game() else { return };
        game.next_entity += 1;
        let id = game.next_entity;
        let cushion = horizon_client::entities::Cushion {
            id,
            position: Vec3::new(2.5, 61.0, 2.5),
            pos: [2, 61, 2],
            yaw: 0.0,
            color: 0,
            check_interval: 0,
        };
        game.level_add(
            super::super::entity_life::NewEntity::Cushion(cushion),
            super::super::entity_life::Origin::Fresh,
            None,
        );
        let first = Ent::Id(spawn(&mut game, "zombie", [2.5, 61.0, 2.5]));
        let second = Ent::Id(spawn(&mut game, "zombie", [2.5, 61.0, 2.5]));
        assert!(game.start_riding(first, Ent::Id(id), false, false));
        assert!(!game.start_riding(second, Ent::Id(id), false, false));
        game.keys.insert(KeyCode::ShiftLeft);
        game.stop_riding(first);
        assert!(!game.start_riding(Ent::Host, Ent::Id(id), false, false));
        game.keys.remove(&KeyCode::ShiftLeft);
        assert!(game.start_riding(Ent::Host, Ent::Id(id), false, false));
    }

    #[test]
    fn a_periodic_save_writes_the_root_vehicle_tree_only_for_a_lone_player() {
        let Some(mut game) = flat_game() else { return };
        let pig = spawn(&mut game, "pig", [2.5, 61.0, 2.5]);
        assert!(game.start_riding(Ent::Guest(3), Ent::Id(pig), true, false));
        let (attach, tree) = game.parent_vehicle_save(Ent::Guest(3));
        assert!(attach.is_some());
        let tree = tree.expect("the root vehicle's tree");
        assert_eq!(
            horizon_client::entity_nbt::kind(&tree).as_deref(),
            Some("minecraft:pig")
        );
        assert!(game.start_riding(Ent::Guest(4), Ent::Id(pig), true, false));
        assert_eq!(game.parent_vehicle_save(Ent::Guest(3)), (None, None));
        game.stop_riding(Ent::Guest(3));
        assert_eq!(game.parent_vehicle_save(Ent::Guest(3)), (None, None));
    }

    fn add_stand(game: &mut Game<'_>, at: [f64; 3]) -> u32 {
        game.next_entity += 1;
        let id = game.next_entity;
        let stand =
            horizon_client::entities::ArmorStand::new(id, Vec3::new(at[0], at[1], at[2]), 0.0);
        game.level_add(
            super::super::entity_life::NewEntity::Stand(stand),
            super::super::entity_life::Origin::Fresh,
            None,
        );
        id
    }

    #[test]
    fn an_armor_stand_on_a_pig_sits_at_the_pigs_point() {
        let Some(mut game) = flat_game() else { return };
        let pig = spawn(&mut game, "pig", [2.5, 61.0, 2.5]);
        let stand = add_stand(&mut game, [9.5, 70.0, 9.5]);
        assert!(game.start_riding(Ent::Id(stand), Ent::Id(pig), true, false));
        game.tick_passengers();
        let m = game.mobs.iter().find(|m| m.id == pig).unwrap();
        let [x, y, z] = riders::passenger_attachment(m, 0, m.yaw, false);
        let want = Vec3::new(m.position.x + x, m.position.y + y, m.position.z + z);
        let got = game
            .armor_stands
            .iter()
            .find(|a| a.id == stand)
            .unwrap()
            .position;
        assert!(
            (got.x - want.x).abs() < 1e-9 && (got.y - want.y).abs() < 1e-9,
            "{got:?} {want:?}"
        );
        assert!((got.z - want.z).abs() < 1e-9);
    }

    #[test]
    fn a_rider_has_zero_velocity_before_its_tick() {
        let Some(mut game) = flat_game() else { return };
        let pig = spawn(&mut game, "pig", [2.5, 61.0, 2.5]);
        let rider = add_stand(&mut game, [2.5, 61.0, 2.5]);
        let free = add_stand(&mut game, [6.5, 61.0, 6.5]);
        for s in &mut game.armor_stands {
            s.velocity = Vec3::new(1.0, 2.0, 3.0);
        }
        assert!(game.start_riding(Ent::Id(rider), Ent::Id(pig), true, false));
        game.zero_rider_motion();
        let v = |game: &Game<'_>, id| {
            game.armor_stands
                .iter()
                .find(|a| a.id == id)
                .unwrap()
                .velocity
        };
        assert_eq!(v(&game, rider), Vec3::ZERO);
        assert_eq!(v(&game, free), Vec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn an_armor_stand_in_a_boat_turns_with_it() {
        let Some(mut game) = flat_game() else { return };
        game.next_entity += 1;
        let boat_id = game.next_entity;
        let mut boat =
            super::super::boats::Boat::new(boat_id, "oak".into(), Vec3::new(4.5, 61.0, 4.5), 0.0);
        boat.delta_rotation = 20.0;
        game.level_add(
            super::super::entity_life::NewEntity::Boat(boat),
            super::super::entity_life::Origin::Fresh,
            None,
        );
        let stand = add_stand(&mut game, [9.5, 70.0, 9.5]);
        assert!(game.start_riding(Ent::Id(stand), Ent::Id(boat_id), true, false));
        let yaw = |game: &Game<'_>| {
            game.armor_stands
                .iter()
                .find(|a| a.id == stand)
                .unwrap()
                .yaw
        };
        game.tick_passengers();
        assert_eq!(yaw(&game), 20.0);
        for _ in 0..10 {
            game.tick_passengers();
        }
        assert_eq!(yaw(&game), 105.0);
    }

    #[test]
    fn a_painting_rides_by_its_block_and_a_dragon_refuses_unless_forced() {
        let Some(mut game) = flat_game() else { return };
        let pig = spawn(&mut game, "pig", [2.5, 61.0, 2.5]);
        game.next_entity += 1;
        let id = game.next_entity;
        let painting = horizon_client::entities::Painting {
            id,
            pos: [20, 61, 20],
            facing: 0,
            variant: "minecraft:kebab".into(),
            width: 1,
            height: 1,
            check_interval: 0,
        };
        game.level_add(
            super::super::entity_life::NewEntity::Painting(painting),
            super::super::entity_life::Origin::Fresh,
            None,
        );
        assert!(game.start_riding(Ent::Id(id), Ent::Id(pig), true, false));
        game.tick_passengers();
        let m = game.mobs.iter().find(|m| m.id == pig).unwrap();
        let p = game.paintings.iter().find(|p| p.id == id).unwrap();
        assert_eq!(p.pos[0], m.position.x.floor() as i32);
    }

    #[test]
    fn a_mob_in_a_cart_sits_in_it_through_the_table() {
        let Some(mut game) = flat_game() else { return };
        game.next_entity += 1;
        let cart_id = game.next_entity;
        let cart = super::super::minecarts::Minecart::new(
            cart_id,
            super::super::minecarts::CartKind::Rideable,
            Vec3::new(4.5, 61.0, 4.5),
        );
        game.level_add(
            super::super::entity_life::NewEntity::Cart(cart),
            super::super::entity_life::Origin::Fresh,
            None,
        );
        let zombie = spawn(&mut game, "zombie", [9.5, 61.0, 9.5]);
        assert!(game.start_riding(Ent::Id(zombie), Ent::Id(cart_id), true, false));
        game.tick_passengers();
        let m = game.mobs.iter().find(|m| m.id == zombie).unwrap();
        assert!((m.position.x - 4.5).abs() < 1e-9 && (m.position.z - 4.5).abs() < 1e-9);
    }

    fn top_level(game: &mut Game<'_>, remove: bool) -> Vec<horizon_world::nbt::Tag> {
        let keys = std::collections::HashSet::from([(0, 0)]);
        game.entity_chunks(&keys, remove)
            .into_iter()
            .flat_map(|(_, tags)| tags)
            .collect()
    }

    fn passengers_in(tag: &horizon_world::nbt::Tag) -> usize {
        match tag.get("Passengers") {
            Some(horizon_world::nbt::Tag::List(l)) => l.len(),
            _ => 0,
        }
    }

    #[test]
    fn only_roots_are_saved_at_top_level() {
        let Some(mut game) = flat_game() else { return };
        let pig = spawn(&mut game, "pig", [2.5, 61.0, 2.5]);
        let stand = add_stand(&mut game, [2.5, 61.0, 2.5]);
        assert!(game.start_riding(Ent::Id(stand), Ent::Id(pig), true, false));
        let tags = top_level(&mut game, false);
        let kinds: Vec<_> = tags
            .iter()
            .filter_map(horizon_client::entity_nbt::kind)
            .collect();
        assert!(kinds.contains(&"minecraft:pig".to_owned()));
        assert!(!kinds.contains(&"minecraft:armor_stand".to_owned()));
        let pig_tag = tags.iter().find(|t| passengers_in(t) == 1).unwrap();
        assert_eq!(
            horizon_client::entity_nbt::kind(&pig_tag.get("Passengers").map_or(
                horizon_world::nbt::Tag::End,
                |l| match l {
                    horizon_world::nbt::Tag::List(l) => l[0].clone(),
                    _ => horizon_world::nbt::Tag::End,
                }
            ))
            .as_deref(),
            Some("minecraft:armor_stand")
        );
    }

    #[test]
    fn a_player_passenger_is_not_saved_in_its_vehicle() {
        let Some(mut game) = flat_game() else { return };
        let pig = spawn(&mut game, "pig", [2.5, 61.0, 2.5]);
        assert!(game.start_riding(Ent::Guest(3), Ent::Id(pig), true, false));
        let tags = top_level(&mut game, false);
        assert!(tags.iter().all(|t| passengers_in(t) == 0));
    }

    #[test]
    fn a_tree_of_any_kinds_round_trips() {
        let Some(mut game) = flat_game() else { return };
        let pig = spawn(&mut game, "pig", [2.5, 61.0, 2.5]);
        let low = add_stand(&mut game, [2.5, 61.0, 2.5]);
        let high = add_stand(&mut game, [2.5, 61.0, 2.5]);
        assert!(game.start_riding(Ent::Id(low), Ent::Id(pig), true, false));
        assert!(game.start_riding(Ent::Id(high), Ent::Id(low), true, false));
        let tags = top_level(&mut game, true);
        assert!(game.mobs.iter().all(|m| m.id != pig) && game.armor_stands.is_empty());
        game.world.loaded_entities.push(((0, 0), tags));
        game.spawn_loaded_entities(super::super::entity_life::Origin::FromDisk);
        let pig = game
            .mobs
            .iter()
            .find(|m| &*m.kind.id == "minecraft:pig")
            .unwrap()
            .id;
        let [first] = game.passengers_of(Ent::Id(pig))[..] else {
            panic!("one stand rides the pig");
        };
        let first = first.id().unwrap();
        assert!(game.armor_stands.iter().any(|a| a.id == first));
        let [second] = game.passengers_of(Ent::Id(first))[..] else {
            panic!("one stand rides the stand");
        };
        assert!(game.armor_stands.iter().any(|a| Some(a.id) == second.id()));
    }

    #[test]
    fn a_tnt_riding_a_cart_keeps_its_fuse() {
        let Some(mut game) = flat_game() else { return };
        game.next_entity += 1;
        let cart_id = game.next_entity;
        let cart = super::super::minecarts::Minecart::new(
            cart_id,
            super::super::minecarts::CartKind::Rideable,
            Vec3::new(4.5, 61.0, 4.5),
        );
        game.level_add(
            super::super::entity_life::NewEntity::Cart(cart),
            super::super::entity_life::Origin::Fresh,
            None,
        );
        game.next_entity += 1;
        let tnt_id = game.next_entity;
        let air = game.res.facts().air;
        let tnt = super::super::tnt::PrimedTnt::new(
            tnt_id,
            Vec3::new(9.5, 70.0, 9.5),
            Vec3::new(0.0, 0.2, 0.0),
            80,
            air,
        );
        game.level_add(
            super::super::entity_life::NewEntity::Tnt(tnt),
            super::super::entity_life::Origin::Fresh,
            None,
        );
        assert!(game.start_riding(Ent::Id(tnt_id), Ent::Id(cart_id), true, false));
        game.zero_rider_motion();
        game.tick_tnt().unwrap();
        assert_eq!(game.tnts.iter().find(|t| t.id == tnt_id).unwrap().fuse, 79);
        game.tick_passengers();
        let at = game.tnts.iter().find(|t| t.id == tnt_id).unwrap().position;
        assert!((at.x - 4.5).abs() < 1e-9 && (at.z - 4.5).abs() < 1e-9);
        assert!(
            (at.y - (61.0 + super::super::minecarts::PASSENGER_HEIGHT)).abs() < 1e-9,
            "{at:?}"
        );
    }

    fn run(game: &mut Game<'_>, command: &str) -> String {
        game.chat_capture = Some(Vec::new());
        game.run_command(command);
        let said = game.chat_capture.take().unwrap_or_default();
        said.iter().map(|t| t.plain()).collect::<Vec<_>>().join("|")
    }

    #[test]
    fn summon_passengers_of_any_kind_ride_the_summoned_entity() {
        let Some(mut game) = flat_game() else { return };
        game.build_registries();
        game.ui.registries.permission = 4;
        let said = run(
            &mut game,
            "summon pig 2.5 61 2.5 {Passengers:[{id:armor_stand,Passengers:[{id:armor_stand}]}]}",
        );
        let pig = game
            .mobs
            .iter()
            .find(|m| &*m.kind.id == "minecraft:pig")
            .unwrap()
            .id;
        let [first] = game.passengers_of(Ent::Id(pig))[..] else {
            panic!("{said}");
        };
        assert_eq!(game.passengers_of(first).len(), 1);
        let said = run(
            &mut game,
            "execute as @e[type=armor_stand] on vehicle run tp @s 20 61 20",
        );
        let at = game.mobs.iter().find(|m| m.id == pig).unwrap().position;
        assert_eq!(at.x, 20.5, "{said}");
    }

    #[test]
    fn ride_mounts_any_entity_on_any_entity() {
        let Some(mut game) = flat_game() else { return };
        let pig = spawn(&mut game, "pig", [2.5, 61.0, 2.5]);
        let stand = add_stand(&mut game, [2.5, 61.0, 2.5]);
        game.build_registries();
        game.ui.registries.permission = 4;
        let said = run(
            &mut game,
            "ride @e[type=armor_stand,limit=1] mount @e[type=pig,limit=1]",
        );
        assert_eq!(
            game.vehicle_of(Ent::Id(stand)),
            Some(Ent::Id(pig)),
            "{said}"
        );
        game.run_command("ride @e[type=armor_stand,limit=1] mount @e[type=pig,limit=1]");
        assert_eq!(
            game.passengers_of(Ent::Id(pig)),
            vec![Ent::Id(stand)],
            "already riding"
        );
        game.run_command("ride @e[type=pig,limit=1] mount @e[type=armor_stand,limit=1]");
        assert_eq!(game.vehicle_of(Ent::Id(pig)), None, "a loop");
        game.run_command("ride @e[type=armor_stand,limit=1] dismount");
        assert_eq!(game.vehicle_of(Ent::Id(stand)), None);
        let boat = {
            game.next_entity += 1;
            let id = game.next_entity;
            let b = super::super::boats::Boat::new(
                id,
                std::sync::Arc::from("oak"),
                Vec3::new(2.5, 61.0, 2.5),
                0.0,
            );
            game.level_add(
                super::super::entity_life::NewEntity::Boat(b),
                super::super::entity_life::Origin::Fresh,
                None,
            );
            id
        };
        game.run_command("ride @e[type=pig,limit=1] mount @e[type=oak_boat,limit=1]");
        assert_eq!(game.vehicle_of(Ent::Id(pig)), Some(Ent::Id(boat)));
        game.run_command("ride @e[type=oak_boat,limit=1] mount @e[type=armor_stand,limit=1]");
        assert_eq!(
            game.vehicle_of(Ent::Id(boat)),
            Some(Ent::Id(stand)),
            "a boat rides a stand"
        );
    }
}
