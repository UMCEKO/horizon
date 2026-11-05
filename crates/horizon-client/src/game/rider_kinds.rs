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

impl Game<'_> {
    /// `EntityType` attachments of `ty`, read once from `entity-types.json`.
    pub(super) fn type_attachments(&self, ty: &str) -> Option<Attachments> {
        static TABLE: std::sync::OnceLock<HashMap<String, Attachments>> =
            std::sync::OnceLock::new();
        TABLE
            .get_or_init(|| {
                std::fs::read(self.options.resources.join("entity-types.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                    .and_then(|v| {
                        v.as_object().map(|o| {
                            o.iter()
                                .filter_map(|(k, t)| Some((k.clone(), Attachments::parse(t)?)))
                                .collect()
                        })
                    })
                    .unwrap_or_default()
            })
            .get(ty)
            .cloned()
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
        } else if let Some(o) = self.others.iter_mut().find(|o| o.id == id) {
            o.position = at;
        }
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
        }
    }

    /// `Entity.rideTick`'s `setDeltaMovement(ZERO)`, before the passenger's own tick.
    pub(super) fn zero_rider_motion(&mut self) {
        let riders: Vec<u32> = self
            .riding_table
            .borrow()
            .riders_in_order()
            .into_iter()
            .filter_map(|(rider, _)| rider.id())
            .collect();
        for id in riders {
            self.zero_plain_motion(id);
        }
    }

    /// `Entity.positionRider` for the passengers that are no mob, on a vehicle of any kind.
    pub(super) fn position_plain_riders(&mut self) {
        let pairs: Vec<(u32, Ent, usize, usize)> = {
            let table = self.riding_table.borrow();
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
        for (rider, vehicle, n, count) in pairs {
            let Some(body) = self.plain_body(rider) else {
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
                self.plain_body(vehicle_id).and_then(|v| {
                    let attach = self.type_attachments(&v.ty)?.passenger_point(n, v.yaw);
                    Some(Vec3::new(
                        v.at.x + attach[0] - own[0],
                        v.at.y + attach[1] - own[1],
                        v.at.z + attach[2] - own[2],
                    ))
                })
            };
            if let Some(at) = target {
                self.set_plain_position(rider, at);
            }
        }
    }
}
