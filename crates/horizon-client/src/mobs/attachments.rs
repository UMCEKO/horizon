//! `EntityAttachments`: the passenger and vehicle points each `EntityType`
//! declares (`passengerAttachments`, `vehicleAttachment`), exported with the
//! type's dimensions, and the set a mob's box uses now.
use super::{Mob, Species, species::Baby};
use serde_json::Value;

/// One `EntityAttachments`, with whether its dimensions are `fixed`
/// (`EntityDimensions.scale` leaves those alone).
#[derive(Clone, Debug, PartialEq)]
pub struct Attachments {
    pub passenger: Vec<[f64; 3]>,
    /// Index 0 only: `getVehicleAttachmentPoint` reads `get(VEHICLE, 0, yRot)`.
    pub vehicle: [f64; 3],
    pub fixed: bool,
}

/// `EntityAttachments.createDefault` of a zero-high box, for types built without the export.
impl Default for Attachments {
    fn default() -> Self {
        Self::fixed(0.0)
    }
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
    pub poses: Vec<(mod_api::Pose, Attachments)>,
}

const POSES: [(&str, mod_api::Pose); 7] = [
    ("STANDING", mod_api::Pose::Standing),
    ("CROUCHING", mod_api::Pose::Crouching),
    ("SWIMMING", mod_api::Pose::Swimming),
    ("FALL_FLYING", mod_api::Pose::FallFlying),
    ("SPIN_ATTACK", mod_api::Pose::SpinAttack),
    ("SLEEPING", mod_api::Pose::Sleeping),
    ("DYING", mod_api::Pose::Dying),
];

fn point(v: &Value) -> Option<[f64; 3]> {
    let a = v.as_array()?;
    Some([
        a.first()?.as_f64()?,
        a.get(1)?.as_f64()?,
        a.get(2)?.as_f64()?,
    ])
}

impl Attachments {
    /// A dimensions object (`width`, `height`, `fixed`, `attachments`) of `entity-types.json`.
    pub fn parse(v: &Value) -> Option<Self> {
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
    /// `EntityDimensions.scale(width, height)`: x and z by the width factor, y by the height's.
    pub fn scaled_by(&self, width: f32, height: f32) -> Self {
        if self.fixed || (width == 1.0 && height == 1.0) {
            return self.clone();
        }
        let (w, h) = (f64::from(width), f64::from(height));
        let times = |p: [f64; 3]| [p[0] * w, p[1] * h, p[2] * w];
        Self {
            passenger: self.passenger.iter().map(|&p| times(p)).collect(),
            vehicle: times(self.vehicle),
            fixed: false,
        }
    }
    /// `EntityDimensions.scale(f)`: every point times `f` (floats widened).
    pub fn scaled(&self, f: f32) -> Self {
        self.scaled_by(f, f)
    }
    /// `EntityAttachments.getClamped(PASSENGER, index, 0)`: the last point serves every later seat.
    pub fn passenger_point(&self, index: usize) -> [f64; 3] {
        self.passenger[index.min(self.passenger.len() - 1)]
    }
}

impl TypeAttachments {
    /// The `attachments`, `dimensions` and `poses` of one type of `entity-types.json`.
    pub fn parse(id: &str, t: &Value, baby: Baby) -> crate::Result<Self> {
        let stale =
            || format!("{id}: no attachments in entity-types.json, re-prepare (OUTPUT_VERSION 5)");
        let adult = Attachments::parse(t).ok_or_else(stale)?;
        let dims = t.get("dimensions").ok_or_else(stale)?;
        let named = |k: &str| dims.get(k).and_then(Attachments::parse);
        let baby_set = named("BABY_DIMENSIONS").or_else(|| named("BABY_STANDING_DIMENSIONS"));
        if matches!(baby, Baby::Abs(..)) && baby_set.is_none() {
            return Err(format!("{id}: no BABY_DIMENSIONS in entity-types.json").into());
        }
        let mut poses = Vec::new();
        if let Some(Value::Object(map)) = t.get("poses") {
            for (name, pose) in POSES {
                if let Some(a) = map.get(name).and_then(Attachments::parse) {
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
/// uses now, scaled as `Mob::dims_in_pose` scales the box.
pub fn of(m: &Mob) -> Attachments {
    let t = &m.kind.attachments;
    let pose = m.pose();
    if !t.poses.is_empty() {
        let set = t.poses.iter().find(|(p, _)| *p == pose).map(|(_, a)| a);
        return set.unwrap_or(&t.adult).scaled(m.model_scale());
    }
    if let (mod_api::Pose::Sleeping, Some(sleeping)) = (pose, &t.sleeping) {
        return sleeping.clone();
    }
    let species = m.kind.species;
    let baby = super::species::def(species).baby;
    let age_scale = if m.is_baby() { 0.5 } else { 1.0 };
    let mut a = if species == Species::Sniffer
        && super::sniffer::digging(m)
        && let Some(digging) = &t.digging
    {
        digging.scaled(age_scale)
    } else if super::camel::is_camel(species) && pose == mod_api::Pose::Sitting {
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
    if species == Species::Warden && super::warden::digging_or_emerging(m) {
        a = Attachments::fixed(1.0);
    }
    let (mut width, mut height) = (1.0_f32, 1.0_f32);
    if pose == mod_api::Pose::LongJumping && species == Species::Goat {
        (width, height) = (0.7, 0.7);
    }
    if species == Species::Pufferfish {
        width = super::aquatic::puff_scale(m.extra.size) as f32;
        height = width;
    } else if species == Species::Phantom {
        width = 1.0 + 0.15 * m.extra.size as f32;
        height = width;
    } else if m.extra.size > 0 {
        width = m.extra.size as f32;
        height = width;
    }
    if species == Species::Shulker
        && let Some(s) = super::shulker::peek_height_scale(m)
    {
        height *= s;
    }
    if species == Species::Salmon {
        let s = super::aquatic::salmon_scale(m.extra.variant);
        width *= s;
        height *= s;
    }
    if species == Species::PolarBear {
        height *= super::polar_bear::stand_box_scale(m);
    }
    let scale = m.model_scale();
    a.scaled_by(width * scale, height * scale)
}
