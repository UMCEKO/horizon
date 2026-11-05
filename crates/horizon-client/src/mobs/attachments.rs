//! `EntityAttachments`: the passenger and vehicle points each `EntityType`
//! declares (`passengerAttachments`, `vehicleAttachment`), exported with the
//! type's dimensions, and the set a mob's box uses now.
use super::{Mob, Species, camel, species::Baby};
use crate::pose::Pose;
use horizon_core::geometry::Vec3;
use serde_json::Value;

/// One `EntityAttachments`, with whether its dimensions are `fixed`
/// (`EntityDimensions.scale` leaves those alone).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attachments {
    pub passenger: Vec<[f64; 3]>,
    /// Index 0 only: `getVehicleAttachmentPoint` reads `get(VEHICLE, 0, yRot)`.
    pub vehicle: [f64; 3],
    pub fixed: bool,
}

/// What a type exports: its own attachments, and those of the dimensions its
/// class swaps in (`BABY_DIMENSIONS`, `*_SITTING_DIMENSIONS`,
/// `DIGGING_DIMENSIONS`) and of each `Avatar.POSES` pose.
#[derive(Clone, Debug, Default)]
pub struct TypeAttachments {
    pub adult: Attachments,
    pub baby: Option<Attachments>,
    pub sitting: Option<Attachments>,
    pub baby_sitting: Option<Attachments>,
    pub digging: Option<Attachments>,
    /// `LivingEntity.SLEEPING_DIMENSIONS`, which every living type's class inherits.
    pub sleeping: Option<Attachments>,
    pub poses: Vec<(Pose, Attachments)>,
}

const POSES: [Pose; 6] = [
    Pose::Standing,
    Pose::Crouching,
    Pose::Swimming,
    Pose::FallFlying,
    Pose::SpinAttack,
    Pose::Sleeping,
];

fn point(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    Some([
        a.first()?.as_f64()?,
        a.get(1)?.as_f64()?,
        a.get(2)?.as_f64()?,
    ])
}
/// `Vec3.yRot(-yaw)` of `EntityAttachments.transformPoint`.
fn turned(p: [f64; 3], yaw: f32) -> [f64; 3] {
    let v = Vec3::new(p[0], p[1], p[2]).rotate_y(-yaw * (std::f64::consts::PI / 180.0) as f32);
    [v.x, v.y, v.z]
}

impl Attachments {
    /// A dimensions object (`width`, `height`, `fixed`, `attachments`) of `entity-types.json`.
    fn parse(v: &Value) -> Option<Self> {
        let a = v.get("attachments")?;
        let passenger: Vec<[f64; 3]> = a
            .get("passenger")?
            .as_array()?
            .iter()
            .map(point)
            .collect::<Option<_>>()?;
        let vehicle = point(a.get("vehicle")?.as_array()?.first()?)?;
        if passenger.is_empty() {
            return None;
        }
        Some(Self {
            passenger,
            vehicle,
            fixed: v.get("fixed").and_then(Value::as_bool).unwrap_or(false),
        })
    }
    /// `EntityAttachments.createDefault` of `EntityDimensions.fixed(width, height)`.
    pub fn fixed(height: f32) -> Self {
        Self {
            passenger: vec![[0.0, f64::from(height), 0.0]],
            vehicle: [0.0; 3],
            fixed: true,
        }
    }
    /// `EntityDimensions.scale(f)`: every point times `f` (floats widened).
    pub fn scaled(&self, f: f32) -> Self {
        if self.fixed || f == 1.0 {
            return self.clone();
        }
        let f = f64::from(f);
        let times = |p: [f64; 3]| [p[0] * f, p[1] * f, p[2] * f];
        Self {
            passenger: self.passenger.iter().map(|&p| times(p)).collect(),
            vehicle: times(self.vehicle),
            fixed: false,
        }
    }
    /// `EntityAttachments.getClamped(PASSENGER, index, yaw)`.
    pub fn passenger_point(&self, index: usize, yaw: f32) -> [f64; 3] {
        turned(self.passenger[index.min(self.passenger.len() - 1)], yaw)
    }
    /// `EntityAttachments.get(VEHICLE, 0, yaw)`.
    pub fn vehicle_point(&self, yaw: f32) -> [f64; 3] {
        turned(self.vehicle, yaw)
    }
}

impl TypeAttachments {
    /// The `attachments`, `dimensions` and `poses` of one type of `entity-types.json`.
    pub fn parse(id: &str, t: &Value, baby: Baby) -> crate::Result<Self> {
        let stale =
            || format!("{id}: no attachments in entity-types.json, re-prepare (OUTPUT_VERSION 4)");
        let adult = Attachments::parse(t).ok_or_else(stale)?;
        let dims = t.get("dimensions").ok_or_else(stale)?;
        let named = |k: &str| dims.get(k).and_then(Attachments::parse);
        let baby_set = named("BABY_DIMENSIONS").or_else(|| named("BABY_STANDING_DIMENSIONS"));
        if matches!(baby, Baby::Abs(..)) && baby_set.is_none() {
            return Err(format!("{id}: no BABY_DIMENSIONS in entity-types.json").into());
        }
        let mut poses = Vec::new();
        if let Some(Value::Object(map)) = t.get("poses") {
            for pose in POSES {
                if let Some(a) = map.get(pose.name()).and_then(Attachments::parse) {
                    poses.push((pose, a));
                }
            }
        }
        Ok(Self {
            adult,
            baby: baby_set,
            sitting: named("ADULT_SITTING_DIMENSIONS"),
            baby_sitting: named("BABY_SITTING_DIMENSIONS"),
            digging: named("DIGGING_DIMENSIONS"),
            sleeping: named("SLEEPING_DIMENSIONS"),
            poses,
        })
    }
}

/// `LivingEntity.getDimensions(pose).attachments()`: the set the mob's box
/// uses now, as `Mob::awake_dims_in` picks the box.
pub fn of(m: &Mob) -> Attachments {
    let t = &m.kind.attachments;
    let baby = super::species::def(m.kind.species).baby;
    let age_scale = match baby {
        Baby::Scale(s, _) => s as f32,
        _ => 0.5,
    };
    if let (true, Some(sleeping)) = (super::villager::sleeping(m), &t.sleeping) {
        return sleeping.clone();
    }
    let sniffer_digging = m.kind.species == Species::Sniffer && super::sniffer::digging(m);
    let mut a = if let (true, Some(d)) = (sniffer_digging, &t.digging) {
        d.scaled(age_scale)
    } else if camel::is_camel(m.kind.species) && camel::sitting(m) {
        let sitting = if m.is_baby() {
            &t.baby_sitting
        } else {
            &t.sitting
        };
        sitting.clone().unwrap_or_else(|| t.adult.clone())
    } else if m.is_baby() {
        match baby {
            Baby::None => t.adult.clone(),
            Baby::Abs(..) => t.baby.clone().unwrap_or_else(|| t.adult.clone()),
            Baby::Scale(s, _) => t.baby.clone().unwrap_or_else(|| t.adult.scaled(s as f32)),
        }
    } else {
        t.adult.clone()
    };
    if m.kind.species == Species::Warden && super::warden::digging_or_emerging(m) {
        a = Attachments::fixed(1.0);
    }
    if m.kind.species == Species::Pufferfish {
        a = a.scaled(super::aquatic::puff_scale(m.extra.size) as f32);
    } else if m.extra.size > 0 {
        a = a.scaled(m.extra.size as f32);
    }
    a
}
