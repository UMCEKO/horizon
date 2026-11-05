//! Riders and vehicles of any kind but a mob or a player: `Entity.rideTick` (zero motion) and the default
//! `Entity.positionRider` over the type's exported attachment points.
use super::{Game, vehicle_rules::Seat, vehicle_rules::Vehicle};
use horizon_client::{mobs::attachments::Attachments, riding::Ent};
use horizon_core::geometry::Vec3;
use std::collections::HashMap;

/// What placing needs of an entity of a kind that is no mob: where it is, which way it faces, its type.
struct Body {
    at: Vec3,
    yaw: f32,
    ty: String,
}

/// The middle of a box, where a block-attached entity stands.
fn centre(lo: [f64; 3], hi: [f64; 3]) -> Vec3 {
    Vec3::new(
        (lo[0] + hi[0]) / 2.0,
        (lo[1] + hi[1]) / 2.0,
        (lo[2] + hi[2]) / 2.0,
    )
}

/// `BlockAttachedEntity.setPos`: the block holding the point.
fn block_of(at: Vec3) -> [i32; 3] {
    [
        at.x.floor() as i32,
        at.y.floor() as i32,
        at.z.floor() as i32,
    ]
}

impl Game<'_> {
    /// `EntityType` attachments of `ty`, read once from `entity-types.json`.
    pub(super) fn type_attachments(&self, ty: &str) -> Option<Attachments> {
        self.type_entry(ty).map(|e| e.0)
    }
    /// `EntityType.getWidth` and `getHeight` of `ty`.
    pub(super) fn type_size(&self, ty: &str) -> [f64; 2] {
        self.type_entry(ty).map_or([0.0; 2], |e| [e.2, e.1])
    }
    /// `EntityType.getHeight` of `ty`.
    fn type_height(&self, ty: &str) -> f64 {
        self.type_entry(ty).map_or(0.0, |e| e.1)
    }
    fn type_entry(&self, ty: &str) -> Option<(Attachments, f64, f64)> {
        static TABLE: std::sync::OnceLock<HashMap<String, (Attachments, f64, f64)>> =
            std::sync::OnceLock::new();
        TABLE
            .get_or_init(|| {
                std::fs::read(self.options.resources.join("entity-types.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                    .and_then(|v| {
                        v.as_object().map(|o| {
                            o.iter()
                                .filter_map(|(k, t)| {
                                    let height = f64::from(t["height"].as_f64()? as f32);
                                    let width = f64::from(t["width"].as_f64()? as f32);
                                    Some((k.clone(), (Attachments::parse(t)?, height, width)))
                                })
                                .collect()
                        })
                    })
                    .unwrap_or_default()
            })
            .get(ty)
            .cloned()
    }

    /// Where the entity `id` of a kind that is no mob is.
    pub(super) fn plain_position(&self, id: u32) -> Option<Vec3> {
        self.plain_body(id).map(|b| b.at)
    }

    /// Whether `id` is an entity of a kind that is no mob, boat, cart or cushion.
    pub(super) fn is_plain_vehicle(&self, id: u32) -> bool {
        self.plain_body(id).is_some()
    }

    /// `Entity.getDismountLocationForPassenger`: the top of the vehicle's box.
    pub(super) fn plain_dismount(&self, id: u32) -> Vec3 {
        self.plain_body(id).map_or(Vec3::ZERO, |b| {
            Vec3::new(b.at.x, b.at.y + self.type_height(&b.ty), b.at.z)
        })
    }

    /// The passenger point of the plain vehicle `id` for the passenger at `index`, less `own`.
    pub(super) fn plain_seat(&self, id: u32, index: usize, own: [f64; 3]) -> Option<Vec3> {
        let v = self.plain_body(id)?;
        let attach = self.type_attachments(&v.ty)?.passenger_point(index, v.yaw);
        Some(Vec3::new(
            v.at.x + attach[0] - own[0],
            v.at.y + attach[1] - own[1],
            v.at.z + attach[2] - own[2],
        ))
    }

    /// The place and type of the entity `id` when it is of a kind that is no mob.
    fn plain_body(&self, id: u32) -> Option<Body> {
        let body = |at: Vec3, yaw: f32, ty: &str| {
            Some(Body {
                at,
                yaw,
                ty: ty.into(),
            })
        };
        if let Some(i) = self.dropped.iter().find(|i| i.id == id) {
            return body(i.position, 0.0, "minecraft:item");
        }
        if let Some(o) = self.orbs.iter().find(|o| o.id == id) {
            return body(o.position, 0.0, "minecraft:experience_orb");
        }
        if let Some(p) = self.creatures.projectiles.iter().find(|p| p.id == id) {
            return body(p.position, p.yaw, p.kind.id());
        }
        if let Some(a) = self.armor_stands.iter().find(|a| a.id == id) {
            return body(a.position, a.yaw, "minecraft:armor_stand");
        }
        if let Some(b) = self.boats.iter().find(|b| b.id == id) {
            return body(b.position, b.yaw, &b.entity_type());
        }
        if let Some(c) = self.carts.iter().find(|c| c.id == id) {
            return body(c.position, c.yaw, c.kind.id());
        }
        if let Some(c) = self.cushions.iter().find(|c| c.id == id) {
            return body(c.position, c.yaw, "minecraft:cushion");
        }
        if let Some(t) = self.tnts.iter().find(|t| t.id == id) {
            return body(t.position, 0.0, "minecraft:tnt");
        }
        if let Some(c) = self.crystals.iter().find(|c| c.id == id) {
            return body(c.position, 0.0, "minecraft:end_crystal");
        }
        if let Some(b) = self.bolts.iter().find(|b| b.id == id) {
            return body(
                Vec3::new(b.at[0], b.at[1], b.at[2]),
                0.0,
                "minecraft:lightning_bolt",
            );
        }
        if let Some(f) = self.item_frames.iter().find(|f| f.id == id) {
            let (lo, hi) = f.bounds();
            let yaw = [0.0, 0.0, 180.0, 0.0, 90.0, 270.0][usize::from(f.facing.min(5))];
            return body(centre(lo, hi), yaw, f.kind());
        }
        if let Some(p) = self.paintings.iter().find(|p| p.id == id) {
            let (lo, hi) = p.bounds();
            return body(
                centre(lo, hi),
                f32::from(p.facing & 3) * 90.0,
                "minecraft:painting",
            );
        }
        if let Some(d) = self.dragon.as_ref().filter(|d| d.id == id) {
            return body(d.position, d.yaw, "minecraft:ender_dragon");
        }
        let o = self.others.iter().find(|o| o.id == id)?;
        body(o.position, o.yaw, &o.kind)
    }

    /// `Entity.setPos` of an entity of a kind that is no mob: the position, the previous one untouched.
    fn set_plain_position(&mut self, id: u32, at: Vec3) {
        if let Some(i) = self.dropped.iter_mut().find(|i| i.id == id) {
            i.position = at;
        } else if let Some(o) = self.orbs.iter_mut().find(|o| o.id == id) {
            o.position = at;
        } else if let Some(p) = self.creatures.projectiles.iter_mut().find(|p| p.id == id) {
            p.position = at;
        } else if let Some(a) = self.armor_stands.iter_mut().find(|a| a.id == id) {
            a.position = at;
        } else if let Some(b) = self.boats.iter_mut().find(|b| b.id == id) {
            b.position = at;
        } else if let Some(c) = self.carts.iter_mut().find(|c| c.id == id) {
            c.position = at;
        } else if let Some(c) = self.cushions.iter_mut().find(|c| c.id == id) {
            c.position = at;
        } else if let Some(t) = self.tnts.iter_mut().find(|t| t.id == id) {
            t.position = at;
        } else if let Some(c) = self.crystals.iter_mut().find(|c| c.id == id) {
            c.position = at;
        } else if let Some(b) = self.bolts.iter_mut().find(|b| b.id == id) {
            b.at = [at.x, at.y, at.z];
        } else if let Some(f) = self.item_frames.iter_mut().find(|f| f.id == id) {
            f.pos = block_of(at);
        } else if let Some(p) = self.paintings.iter_mut().find(|p| p.id == id) {
            p.pos = block_of(at);
        } else if let Some(d) = self.dragon.as_mut().filter(|d| d.id == id) {
            d.position = at;
        } else if let Some(o) = self.others.iter_mut().find(|o| o.id == id) {
            o.position = at;
        }
    }

    /// The yaw of a plain entity that has one.
    fn plain_yaw_mut(&mut self, id: u32) -> Option<&mut f32> {
        if let Some(p) = self.creatures.projectiles.iter_mut().find(|p| p.id == id) {
            return Some(&mut p.yaw);
        }
        if let Some(a) = self.armor_stands.iter_mut().find(|a| a.id == id) {
            return Some(&mut a.yaw);
        }
        if let Some(b) = self.boats.iter_mut().find(|b| b.id == id) {
            return Some(&mut b.yaw);
        }
        if let Some(c) = self.carts.iter_mut().find(|c| c.id == id) {
            return Some(&mut c.yaw);
        }
        if let Some(c) = self.cushions.iter_mut().find(|c| c.id == id) {
            return Some(&mut c.yaw);
        }
        self.others
            .iter_mut()
            .find(|o| o.id == id)
            .map(|o| &mut o.yaw)
    }

    /// `AbstractBoat.positionRider` for a rider that cannot turn in boats: it turns with the boat, within 105 degrees of it.
    fn turn_plain_in_boat(&mut self, rider: u32, boat_yaw: f32, delta_rotation: f32) {
        let server = self.remote.is_none();
        let Some(yaw) = self.plain_yaw_mut(rider) else {
            return;
        };
        if server {
            *yaw += delta_rotation;
        }
        let delta = horizon_client::camera::wrap_degrees(*yaw - boat_yaw);
        *yaw += delta.clamp(-105.0, 105.0) - delta;
    }

    /// `Entity.setDeltaMovement(ZERO)` of an entity of a kind that is no mob.
    fn zero_plain_motion(&mut self, id: u32) {
        if let Some(i) = self.dropped.iter_mut().find(|i| i.id == id) {
            i.velocity = Vec3::ZERO;
        } else if let Some(o) = self.orbs.iter_mut().find(|o| o.id == id) {
            o.velocity = Vec3::ZERO;
        } else if let Some(p) = self.creatures.projectiles.iter_mut().find(|p| p.id == id) {
            p.velocity = Vec3::ZERO;
        } else if let Some(a) = self.armor_stands.iter_mut().find(|a| a.id == id) {
            a.velocity = Vec3::ZERO;
        } else if let Some(b) = self.boats.iter_mut().find(|b| b.id == id) {
            b.velocity = Vec3::ZERO;
        } else if let Some(c) = self.carts.iter_mut().find(|c| c.id == id) {
            c.velocity = Vec3::ZERO;
        } else if let Some(t) = self.tnts.iter_mut().find(|t| t.id == id) {
            t.velocity = Vec3::ZERO;
        } else if let Some(d) = self.dragon.as_mut().filter(|d| d.id == id) {
            d.stop();
        }
    }

    /// `Entity.rideTick`'s `setDeltaMovement(ZERO)`, before the passenger's own tick.
    pub(super) fn zero_rider_motion(&mut self) {
        let ids = |table: &horizon_client::riding::Riding| -> Vec<u32> {
            table
                .riders_in_order()
                .into_iter()
                .filter_map(|(rider, _)| rider.id())
                .collect()
        };
        let riders = match self.remote.as_ref() {
            Some(r) => ids(&r.passengers),
            None => ids(&self.riding_table.borrow()),
        };
        for id in riders {
            self.zero_plain_motion(id);
        }
    }

    /// `Entity.positionRider` of a mob on a vehicle of a kind that is no mob or boat.
    fn position_mob_on_plain(&mut self, rider: u32, vehicle: Ent, n: usize, count: usize) {
        let (Some(vehicle), Some(i)) = (vehicle.id(), self.mobs.iter().position(|m| m.id == rider))
        else {
            return;
        };
        if self.mobs.iter().any(|m| m.id == vehicle) || self.boats.iter().any(|b| b.id == vehicle) {
            return;
        }
        // A mob on a text display (NPC plugins seat their sitting NPCs on one): the server's
        // position gives way to the display's seat, as `rideTick` places it.
        if let Some((at, before)) = self.text_displays.get(&(vehicle as i32)).map(|d| d.seat()) {
            let m = &mut self.mobs[i];
            // EntityType.TEXT_DISPLAY's box is zero wide.
            let offset = horizon_client::mobs::riders::vehicle_attachment(m, 0.0)[1];
            m.position = Vec3::new(at.x, at.y - offset, at.z);
            m.previous = Vec3::new(before.x, before.y - offset, before.z);
            m.velocity = Vec3::ZERO;
            m.fall_distance = 0.0;
            m.on_ground = false;
            m.ai.nav.stop();
            return;
        }
        let m = &self.mobs[i];
        let width = self.plain_width(vehicle);
        let own = horizon_client::mobs::riders::vehicle_attachment(m, width);
        let at = if let Some(c) = self.carts.iter().position(|c| c.id == vehicle) {
            let seat = Seat {
                offset: own[1],
                animal: false,
            };
            Some(self.passenger_point(Vehicle::Cart(c), n, count, seat, false))
        } else if let Some(c) = self.cushions.iter().position(|c| c.id == vehicle) {
            let seat = Seat {
                offset: own[1],
                animal: false,
            };
            Some(self.passenger_point(Vehicle::Cushion(c), n, count, seat, false))
        } else {
            self.plain_seat(vehicle, n, own)
        };
        if let Some(at) = at {
            let m = &mut self.mobs[i];
            m.position = at;
            m.previous = at;
            m.velocity = Vec3::ZERO;
            m.fall_distance = 0.0;
            m.ai.nav.stop();
        }
    }

    /// `EntityType.getWidth` of the plain entity `id`.
    fn plain_width(&self, id: u32) -> f32 {
        let Some(b) = self.plain_body(id) else {
            return 0.0;
        };
        self.type_entry(&b.ty).map_or(0.0, |e| e.2) as f32
    }

    /// `Entity.positionRider` for the passengers that are no mob, on a vehicle of any kind.
    pub(super) fn position_plain_riders(&mut self) {
        let place = |table: &horizon_client::riding::Riding| -> Vec<(u32, Ent, usize, usize)> {
            table
                .riders_in_order()
                .into_iter()
                .filter_map(|(rider, vehicle)| {
                    let list = table.passengers_of(vehicle);
                    let n = list.iter().position(|&p| p == rider)?;
                    Some((rider.id()?, vehicle, n, list.len()))
                })
                .collect()
        };
        let pairs = match self.remote.as_ref() {
            Some(r) => place(&r.passengers),
            None => place(&self.riding_table.borrow()),
        };
        for (rider, vehicle, n, count) in pairs {
            let Some(body) = self.plain_body(rider) else {
                self.position_mob_on_plain(rider, vehicle, n, count);
                continue;
            };
            let Some(vehicle_id) = vehicle.id() else {
                continue;
            };
            let own = self
                .type_attachments(&body.ty)
                .map_or([0.0; 3], |a| a.vehicle_point(body.yaw));
            let seat = Seat {
                offset: own[1],
                animal: false,
            };
            let target = if let Some(i) = self.mobs.iter().position(|m| m.id == vehicle_id) {
                Some(self.passenger_point(Vehicle::Mob(i), n, count, seat, false))
            } else if let Some(i) = self.boats.iter().position(|b| b.id == vehicle_id) {
                Some(self.passenger_point(Vehicle::Boat(i), n, count, seat, false))
            } else if let Some(i) = self.carts.iter().position(|c| c.id == vehicle_id) {
                Some(self.passenger_point(Vehicle::Cart(i), n, count, seat, false))
            } else if let Some(i) = self.cushions.iter().position(|c| c.id == vehicle_id) {
                Some(self.passenger_point(Vehicle::Cushion(i), n, count, seat, false))
            } else {
                self.plain_seat(vehicle_id, n, own)
            };
            if let Some(at) = target {
                self.set_plain_position(rider, at);
            }
            if let Some(i) = self.boats.iter().position(|b| b.id == vehicle_id) {
                let (yaw, delta) = (self.boats[i].yaw, self.boats[i].delta_rotation);
                self.turn_plain_in_boat(rider, yaw, delta);
            }
        }
    }
}
