//! Native entities as live Java views: one per entity from its join to its leave, refreshed raw, written through by bridges.

use super::entity_life::{Kind, NewEntity, Origin, Removal};
use super::java_mods::{read_tag, write_tag};
use super::*;
use commands::Target;
use horizon_client::entity_nbt;
use horizon_client::projectiles::{Owner, ProjectileKind};
use horizon_client::riding::Ent;
use horizon_client::save::{float, number, random_uuid, text};
use horizon_java_host::wire::{In, Out};
use horizon_world::nbt::Tag;

pub(super) const POSITION: u32 = 2;
pub(super) const MOTION: u32 = 4;
pub(super) const HEALTH: u32 = 8;
const EFFECTS: u32 = 16;
pub(super) const EQUIPMENT: u32 = 32;
pub(super) const FIRE: u32 = 64;
pub(super) const AIR: u32 = 128;
pub(super) const FLAGS: u32 = 256;
pub(super) const NAME: u32 = 512;
pub(super) const ITEM: u32 = 1024;
pub(super) const RIDING: u32 = 2048;
pub(super) const REMOVAL: u32 = 4096;
pub(super) const SPECIES: u32 = 8192;
pub(super) const AI: u32 = 32768;

/// The keys of an entity's saved form that live in Java: attachments and persistent data.
const JAVA_KEYS: [&str; 2] = ["neoforge:attachments", "NeoForgeData"];
const SLOTS: [&str; 8] = [
    "mainhand", "offhand", "feet", "legs", "chest", "head", "body", "saddle",
];

const ALL_KINDS: [Kind; 15] = [
    Kind::Tnt,
    Kind::Bolt,
    Kind::Mob,
    Kind::Item,
    Kind::Orb,
    Kind::Projectile,
    Kind::Stand,
    Kind::Frame,
    Kind::Painting,
    Kind::Boat,
    Kind::Cart,
    Kind::Other,
    Kind::Crystal,
    Kind::Cushion,
    Kind::Dragon,
];

thread_local! {
    static VIEWED: std::cell::RefCell<std::collections::HashSet<(String, u32)>> = Default::default();
}

pub(super) fn reset_views() {
    VIEWED.with_borrow_mut(|v| v.clear());
}

/// The species group of a view whose Java class keeps state the saved form lacks.
#[derive(Default)]
enum SpeciesData {
    #[default]
    None,
    Tnt {
        fuse: i32,
        java_state: i32,
        power: f32,
        owner: Option<[i32; 4]>,
    },
    Bolt {
        damage: f32,
        visual_only: bool,
        life: i32,
        flashes: i32,
        seed: i64,
    },
    Projectile {
        /// `AbstractArrow.baseDamage`, for arrows and tridents only.
        base_damage: Option<f64>,
        owner: Option<[i32; 4]>,
    },
}

/// What the live structs know that the saved form does not.
#[derive(Default)]
struct Overlay {
    species: SpeciesData,
    head: Option<f32>,
    body: Option<f32>,
    hurt: Option<i32>,
    death: Option<i32>,
    crouching: bool,
    sprinting: bool,
    vehicle: u32,
    passengers: Vec<u32>,
    malus: Vec<(u8, f32)>,
}

/// Which groups of the type's views are Java's `LivingEntity` and `ItemEntity` ones.
#[derive(Clone, Copy, Default)]
struct Shape {
    living: bool,
    item: bool,
}

fn doubles<const N: usize>(tag: Option<&Tag>) -> [f64; N] {
    let mut out = [0.0; N];
    if let Some(Tag::List(list)) = tag {
        for (o, v) in out.iter_mut().zip(list) {
            *o = float(Some(v)).unwrap_or(0.0);
        }
    }
    out
}

fn flag(tag: &Tag, key: &str) -> bool {
    number(tag.get(key)).unwrap_or(0) != 0
}

fn component(out: &mut Out, tag: Option<&Tag>) {
    out.bool(tag.is_some());
    if let Some(tag) = tag {
        out.bytes(&write_tag(Tag::compound([("v", tag.clone())])).unwrap_or_default());
    }
}

fn stack(out: &mut Out, tag: Option<&Tag>) {
    out.bytes(&tag.and_then(|t| write_tag(t.clone())).unwrap_or_default());
}

fn uuid_longs(tag: Option<&Tag>) -> Option<(i64, i64)> {
    let Some(Tag::IntArray(u)) = tag else {
        return None;
    };
    let [a, b, c, d] = u[..] else { return None };
    let long = |hi: i32, lo: i32| (i64::from(hi) << 32) | i64::from(lo as u32);
    Some((long(a, b), long(c, d)))
}

fn uuid_tag(text: &str) -> Option<Tag> {
    let bits = u128::from_str_radix(&text.replace('-', ""), 16).ok()?;
    Some(Tag::IntArray(vec![
        (bits >> 96) as i32,
        (bits >> 64) as i32,
        (bits >> 32) as i32,
        bits as i32,
    ]))
}

/// Java's `EntityViews.apply` reads these bytes: every group of `mask`, in bit order.
fn write_groups(out: &mut Out, tag: &Tag, over: &Overlay, shape: Shape, mask: u32) {
    let rotation = doubles::<2>(tag.get("Rotation"));
    let yaw = rotation[0] as f32;
    if mask & POSITION != 0 {
        for v in doubles::<3>(tag.get("Pos")) {
            out.double(v);
        }
        out.float(yaw);
        out.float(rotation[1] as f32);
        out.float(over.head.unwrap_or(yaw));
        out.float(over.body.unwrap_or(yaw));
    }
    if mask & MOTION != 0 {
        for v in doubles::<3>(tag.get("Motion")) {
            out.double(v);
        }
        out.bool(flag(tag, "OnGround"));
    }
    if mask & HEALTH != 0 {
        out.bool(shape.living);
        if shape.living {
            out.float(float(tag.get("Health")).unwrap_or(0.0) as f32);
            out.float(float(tag.get("AbsorptionAmount")).unwrap_or(0.0) as f32);
            let hurt = over
                .hurt
                .map_or(number(tag.get("HurtTime")).unwrap_or(0), i64::from);
            let death = over
                .death
                .map_or(number(tag.get("DeathTime")).unwrap_or(0), i64::from);
            out.0.extend((hurt as i16).to_be_bytes());
            out.0.extend((death as i16).to_be_bytes());
        }
    }
    if mask & EFFECTS != 0 {
        let effects = match tag.get("active_effects") {
            Some(Tag::List(list)) if shape.living => list.as_slice(),
            _ => &[],
        };
        out.int(effects.len() as i32);
        for e in effects {
            out.text(&text(e.get("id")).unwrap_or_default());
            out.int(number(e.get("amplifier")).unwrap_or(0) as i32);
            out.int(number(e.get("duration")).unwrap_or(0) as i32);
            out.bool(flag(e, "ambient"));
            out.bool(
                e.get("show_particles")
                    .is_none_or(|_| flag(e, "show_particles")),
            );
            out.bool(e.get("show_icon").is_none_or(|_| flag(e, "show_icon")));
        }
    }
    if mask & EQUIPMENT != 0 {
        let held: Vec<(u8, &Tag)> = match tag.get("equipment") {
            Some(eq) if shape.living => SLOTS
                .iter()
                .enumerate()
                .filter_map(|(i, name)| eq.get(name).map(|s| (i as u8, s)))
                .collect(),
            _ => Vec::new(),
        };
        out.int(held.len() as i32);
        for (slot, s) in held {
            out.0.push(slot);
            stack(out, Some(s));
        }
    }
    if mask & FIRE != 0 {
        out.int(number(tag.get("Fire")).unwrap_or(0) as i32);
    }
    if mask & AIR != 0 {
        out.int(number(tag.get("Air")).unwrap_or(300) as i32);
    }
    if mask & FLAGS != 0 {
        let mut bits = 0_i32;
        for (on, bit) in [
            (flag(tag, "OnGround"), 1),
            (over.crouching, 2),
            (over.sprinting, 4),
            (flag(tag, "Silent"), 16),
            (flag(tag, "Glowing"), 32),
            (flag(tag, "Invisible"), 64),
            (flag(tag, "NoGravity"), 128),
        ] {
            if on {
                bits |= bit;
            }
        }
        bits |= (number(tag.get("Color")).unwrap_or(0) as i32 & 15) << 16;
        out.int(bits);
    }
    if mask & NAME != 0 {
        component(out, tag.get("CustomName"));
        let tags: Vec<String> = match tag.get("Tags") {
            Some(Tag::List(list)) => list.iter().filter_map(|t| text(Some(t))).collect(),
            _ => Vec::new(),
        };
        out.int(tags.len() as i32);
        for t in tags {
            out.text(&t);
        }
    }
    if mask & ITEM != 0 {
        out.bool(shape.item);
        if shape.item {
            stack(out, tag.get("Item"));
            out.0
                .extend((number(tag.get("PickupDelay")).unwrap_or(0) as i16).to_be_bytes());
            out.int(number(tag.get("Age")).unwrap_or(0) as i32);
            for key in ["Thrower", "Owner"] {
                let uuid = uuid_longs(tag.get(key));
                out.bool(uuid.is_some());
                if let Some((hi, lo)) = uuid {
                    out.long(hi);
                    out.long(lo);
                }
            }
        }
    }
    if mask & RIDING != 0 {
        out.int(over.vehicle as i32);
        out.int(over.passengers.len() as i32);
        for p in &over.passengers {
            out.int(*p as i32);
        }
    }
    if mask & REMOVAL != 0 {
        out.0.push(0);
    }
    if mask & SPECIES != 0 {
        match &over.species {
            SpeciesData::None => {}
            SpeciesData::Tnt {
                fuse,
                java_state,
                power,
                owner,
            } => {
                out.int(*fuse);
                out.int(*java_state);
                out.float(*power);
                let uuid = owner.and_then(|o| uuid_longs(Some(&Tag::IntArray(o.to_vec()))));
                out.bool(uuid.is_some());
                if let Some((hi, lo)) = uuid {
                    out.long(hi);
                    out.long(lo);
                }
            }
            SpeciesData::Bolt {
                damage,
                visual_only,
                life,
                flashes,
                seed,
            } => {
                out.float(*damage);
                out.bool(*visual_only);
                out.int(*life);
                out.int(*flashes);
                out.long(*seed);
            }
            SpeciesData::Projectile { base_damage, owner } => {
                if let Some(damage) = base_damage {
                    out.double(*damage);
                }
                let uuid = owner.and_then(|o| uuid_longs(Some(&Tag::IntArray(o.to_vec()))));
                out.bool(uuid.is_some());
                if let Some((hi, lo)) = uuid {
                    out.long(hi);
                    out.long(lo);
                }
            }
        }
    }
    if mask & AI != 0 {
        out.int(over.malus.len() as i32);
        for (ordinal, cost) in &over.malus {
            out.0.push(*ordinal);
            out.float(*cost);
        }
    }
}

fn kind_of(t: Target) -> Option<(Kind, u32)> {
    Some(match t {
        Target::Mob(id) => (Kind::Mob, id),
        Target::Item(id) => (Kind::Item, id),
        Target::Orb(id) => (Kind::Orb, id),
        Target::Projectile(id) => (Kind::Projectile, id),
        Target::Stand(id) => (Kind::Stand, id),
        Target::Frame(id) => (Kind::Frame, id),
        Target::Painting(id) => (Kind::Painting, id),
        Target::Boat(id) => (Kind::Boat, id),
        Target::Cart(id) => (Kind::Cart, id),
        Target::Other(id) => (Kind::Other, id),
        Target::Crystal(id) => (Kind::Crystal, id),
        Target::Dragon(id) => (Kind::Dragon, id),
        Target::Tnt(id) => (Kind::Tnt, id),
        Target::Bolt(id) => (Kind::Bolt, id),
        Target::Player
        | Target::Guest(_)
        | Target::Fangs(_)
        | Target::Falling(_)
        | Target::Rocket(_)
        | Target::Trident(_)
        | Target::Spawner(_)
        | Target::Cloud(_) => return None,
    })
}

fn removal_byte(why: Removal) -> u8 {
    match why {
        Removal::Killed => 0,
        Removal::Discarded => 1,
        Removal::UnloadedToChunk => 2,
        Removal::UnloadedWithPlayer => 3,
        Removal::ChangedDimension => 4,
    }
}

impl Game<'_> {
    fn java_facts() -> Option<&'static horizon_java_host::entity_facts::EntityFacts> {
        horizon_java_host::entity_facts::get()
    }

    /// The groups a type's views carry, and whether Java's class is living or an item entity: the kinds say it, the facts for the types Horizon only keeps as data.
    fn java_shape(type_id: &str, kind: Option<Kind>) -> (u32, Shape) {
        let known = Shape {
            living: matches!(kind, Some(Kind::Mob | Kind::Stand | Kind::Dragon)),
            item: kind == Some(Kind::Item),
        };
        let mut shape = match Self::java_facts() {
            Some(f) => (
                f.reads(type_id),
                Shape {
                    living: known.living || f.living(type_id),
                    item: known.item || f.item(type_id),
                },
            ),
            None => (1, known),
        };
        // The bridges of the species group refresh it, whatever the mods read.
        if kind == Some(Kind::Mob) {
            shape.0 |= AI;
        }
        if matches!(kind, Some(Kind::Tnt | Kind::Bolt))
            || matches!(
                type_id,
                "minecraft:arrow" | "minecraft:spectral_arrow" | "minecraft:trident"
            )
        {
            shape.0 |= SPECIES;
        }
        shape
    }

    /// `PrimedTnt.owner` as the UUID Java names it by: a mob's, or the player who lit it.
    fn tnt_owner_uuid(
        &self,
        owner: Option<horizon_client::mobs::Who>,
        guest: Option<u32>,
    ) -> Option<[i32; 4]> {
        match owner? {
            horizon_client::mobs::Who::Mob(id) => self.persist.uuids.get(&id).copied(),
            horizon_client::mobs::Who::Player => match guest {
                Some(conn) => self
                    .lan
                    .as_ref()
                    .and_then(|h| h.hub.guest(conn))
                    .map(|g| horizon_client::lan::playerdata::uuid_ints(g.uuid)),
                None => self.level.player_uuid,
            },
        }
    }
    fn java_overlay(&self, t: Target) -> Overlay {
        match t {
            Target::Tnt(id) => {
                let default_tnt = self.tnt_default_state();
                return Overlay {
                    species: self
                        .tnts
                        .iter()
                        .find(|t| t.id == id)
                        .map_or(SpeciesData::None, |t| SpeciesData::Tnt {
                            fuse: t.fuse,
                            java_state: horizon_java_host::bridge::STATES.get().map_or(
                                horizon_java_host::bridge::UNMAPPED,
                                |m| {
                                    m.to_java
                                        .get(t.block_state.unwrap_or(default_tnt).0 as usize)
                                        .copied()
                                        .unwrap_or(horizon_java_host::bridge::UNMAPPED)
                                },
                            ),
                            power: t.power,
                            owner: self.tnt_owner_uuid(t.owner, t.guest),
                        }),
                    ..Overlay::default()
                };
            }
            Target::Bolt(id) => {
                return Overlay {
                    species: self.bolts.iter().find(|b| b.id == id).map_or(
                        SpeciesData::None,
                        |b| SpeciesData::Bolt {
                            damage: b.damage,
                            visual_only: b.visual_only,
                            life: b.life,
                            flashes: b.flashes,
                            seed: b.seed,
                        },
                    ),
                    ..Overlay::default()
                };
            }
            Target::Projectile(id) => {
                return Overlay {
                    species: self
                        .creatures
                        .projectiles
                        .iter()
                        .find(|p| p.id == id)
                        .map_or(SpeciesData::None, |p| SpeciesData::Projectile {
                            base_damage: match p.kind {
                                ProjectileKind::Arrow { base_damage, .. } => Some(base_damage),
                                ProjectileKind::Trident => Some(2.0),
                                _ => None,
                            },
                            owner: p.owner.and(p.owner_uuid),
                        }),
                    ..Overlay::default()
                };
            }
            _ => {}
        }
        let Target::Mob(id) = t else {
            return Overlay::default();
        };
        let Some(m) = self.mobs.iter().find(|m| m.id == id) else {
            return Overlay::default();
        };
        Overlay {
            species: SpeciesData::None,
            head: Some(m.head_yaw),
            body: Some(m.body_yaw),
            hurt: Some(m.hurt_time),
            death: Some(m.death_time),
            crouching: m.extra.crouching,
            sprinting: m.extra.sprinting,
            vehicle: self.mob_vehicle(id).unwrap_or(0),
            passengers: self
                .passengers_of(Ent::Id(id))
                .into_iter()
                .filter_map(Ent::id)
                .collect(),
            malus: m
                .ai
                .malus
                .iter()
                .enumerate()
                .filter_map(|(i, cost)| Some((i as u8, (*cost)?)))
                .collect(),
        }
    }

    /// The group bytes of the entity `t` for `mask`; the saved form beside them when asked for.
    fn java_groups(&mut self, t: Target, mask: u32) -> Option<(Vec<u8>, Tag)> {
        let type_id = self.target_type(t);
        let tag = self.entity_nbt(t)?;
        let over = self.java_overlay(t);
        let (_, shape) = Self::java_shape(&type_id, kind_of(t).map(|k| k.0));
        let mut out = Out::default();
        write_groups(&mut out, &tag, &over, shape, mask);
        let mut tag = tag;
        if let Tag::Compound(map) = &mut tag {
            map.insert("id".into(), Tag::String(type_id.as_str().into()));
        }
        Some((out.0, tag))
    }

    fn java_view_reply(&mut self, t: Target, out: &mut Out) -> bool {
        let Some((_, id)) = kind_of(t) else {
            return false;
        };
        let type_id = self.target_type(t);
        let (mask, _) = Self::java_shape(&type_id, kind_of(t).map(|k| k.0));
        let Some((groups, tag)) = self.java_groups(t, mask) else {
            return false;
        };
        let known = VIEWED.with_borrow(|v| v.contains(&(self.dim.id().to_owned(), id)));
        let Some(saved) = (if known {
            Some(Vec::new())
        } else {
            write_tag(tag)
        }) else {
            return false;
        };
        out.int(id as i32);
        out.bool(known);
        if !known {
            out.bytes(&saved);
        }
        out.int(mask as i32);
        out.0.extend(groups);
        true
    }

    /// `EntityViews.query`: the entities with positions in the box, as ids and the groups their views carry.
    pub(in crate::game) fn java_entities_in(&mut self, low: [f64; 3], high: [f64; 3]) -> Vec<u8> {
        let mut out = Out::default();
        let mut count = 0;
        for target in self.level_candidates() {
            let inside = self
                .target_position(target)
                .is_some_and(|p| (0..3).all(|axis| p[axis] >= low[axis] && p[axis] <= high[axis]));
            if inside && self.java_view_reply(target, &mut out) {
                count += 1;
            }
        }
        let mut all = Out::default();
        all.int(count);
        all.0.extend(out.0);
        all.0
    }

    /// `EntityViews.queryId`: one entity the same way.
    pub(in crate::game) fn java_entity_by_id(&mut self, id: u32) -> Vec<u8> {
        let mut out = Out::default();
        let found = self
            .java_target(id)
            .is_some_and(|t| self.java_view_reply(t, &mut out));
        let mut all = Out::default();
        all.int(found as i32);
        all.0.extend(out.0);
        all.0
    }

    /// The saved form of an entity not yet in its vector, as `persist.rs` encodes it.
    fn java_new_entity_tag(&mut self, e: &NewEntity) -> Option<(Tag, String)> {
        let raw = |g: &Self, id: u32| g.persist.raw.get(&id).cloned();
        let (mut tag, type_id) = match e {
            NewEntity::Mob(m) => {
                let uuid = self.uuid_for(m.id);
                (
                    entity_nbt::mob_to_nbt(
                        m,
                        uuid,
                        raw(self, m.id).as_ref(),
                        Some(self.pack.assets.registry),
                    ),
                    horizon_client::command::args::normalize(&m.kind.id),
                )
            }
            NewEntity::Item(i) => {
                let uuid = self.uuid_for(i.id);
                (
                    entity_nbt::item_entity_to_nbt(i, uuid, raw(self, i.id).as_ref()),
                    "minecraft:item".into(),
                )
            }
            NewEntity::Orb(o) => (
                entity_nbt::orb_to_nbt(o, random_uuid()),
                "minecraft:experience_orb".into(),
            ),
            NewEntity::Projectile(p) => (
                entity_nbt::arrow_to_nbt(p, random_uuid(), None, None, None)
                    .or_else(|| entity_nbt::fireball_to_nbt(p, random_uuid(), None, None))?,
                p.kind.id().into(),
            ),
            NewEntity::Stand(a) => {
                let uuid = self.uuid_for(a.id);
                (
                    entity_nbt::armor_stand_to_nbt(a, uuid, raw(self, a.id).as_ref()),
                    "minecraft:armor_stand".into(),
                )
            }
            NewEntity::Frame(f) => {
                let uuid = self.uuid_for(f.id);
                (
                    entity_nbt::item_frame_to_nbt(f, uuid, raw(self, f.id).as_ref()),
                    if f.glow {
                        "minecraft:glow_item_frame"
                    } else {
                        "minecraft:item_frame"
                    }
                    .into(),
                )
            }
            NewEntity::Painting(p) => {
                let uuid = self.uuid_for(p.id);
                (
                    entity_nbt::painting_to_nbt(p, uuid, raw(self, p.id).as_ref()),
                    "minecraft:painting".into(),
                )
            }
            NewEntity::Boat(b) => {
                let uuid = self.uuid_for(b.id);
                (
                    entity_nbt::boat_to_nbt(
                        &b.entity_type(),
                        b.position,
                        b.velocity,
                        [b.yaw, b.pitch],
                        &b.moving,
                        uuid,
                        raw(self, b.id).as_ref(),
                        b.chest.then_some(&b.items[..]),
                    ),
                    b.entity_type(),
                )
            }
            NewEntity::Cart(c) => {
                let uuid = self.uuid_for(c.id);
                (c.to_nbt(uuid, raw(self, c.id).as_ref()), c.kind.id().into())
            }
            NewEntity::Other(o) => {
                let uuid = self.uuid_for(o.id);
                (self.other_to_nbt(o, uuid), o.kind.clone())
            }
            NewEntity::Crystal(c) => {
                let uuid = self.uuid_for(c.id);
                let raw = self.persist_anon("minecraft:end_crystal", c.position);
                (c.to_nbt(uuid, raw), "minecraft:end_crystal".into())
            }
            NewEntity::Cushion(c) => {
                let uuid = self.uuid_for(c.id);
                (
                    entity_nbt::cushion_to_nbt(c, uuid, raw(self, c.id).as_ref()),
                    String::new(),
                )
            }
            NewEntity::Tnt(t) => {
                let uuid = self.uuid_for(t.id);
                let state = self.state_to_nbt(t.block_state.unwrap_or(self.tnt_default_state()));
                let owner = self.tnt_owner_uuid(t.owner, t.guest);
                (
                    entity_nbt::primed_tnt_to_nbt(
                        &entity_nbt::TntNbt {
                            position: t.position,
                            velocity: t.velocity,
                            on_ground: t.on_ground,
                            fuse: t.fuse,
                            state: Some(state),
                            power: t.power,
                            owner,
                        },
                        uuid,
                        raw(self, t.id).as_ref(),
                    ),
                    "minecraft:tnt".into(),
                )
            }
            NewEntity::Bolt(b) => (
                Tag::compound([
                    ("id", Tag::String("minecraft:lightning_bolt".into())),
                    ("UUID", Tag::IntArray(b.uuid.to_vec())),
                    (
                        "Pos",
                        Tag::List(b.at.iter().map(|v| Tag::Double(v.to_bits())).collect()),
                    ),
                ]),
                "minecraft:lightning_bolt".into(),
            ),
            NewEntity::Dragon(d) => {
                let uuid = self.uuid_for(d.id);
                let raw = self.persist_anon("minecraft:ender_dragon", d.position);
                (d.to_nbt(uuid, raw), "minecraft:ender_dragon".into())
            }
        };
        let type_id = text(tag.get("id")).unwrap_or(type_id);
        if let Tag::Compound(map) = &mut tag {
            map.insert("id".into(), Tag::String(type_id.as_str().into()));
        }
        Some((tag, type_id))
    }

    fn java_entity_request(&mut self, op: &str, body: impl FnOnce(&mut Out)) -> Option<Vec<u8>> {
        self.java_bind_states();
        let mut out = Out::default();
        out.text(self.dim.id());
        out.long(self.world.game_time);
        body(&mut out);
        self.java_request(op, &out.0)
    }

    /// The java-mods arm of the join hook: Java makes the entity's view, and its verdict is the join's.
    pub(super) fn java_entity_joining(
        &mut self,
        e: &NewEntity,
        origin: Origin,
        saved: Option<&Tag>,
    ) -> bool {
        if !self.java_level_running() {
            return true;
        }
        let Some((tag, type_id)) = (match saved {
            Some(tag) => Some((tag.clone(), text(tag.get("id")).unwrap_or_default())),
            None => self.java_new_entity_tag(e),
        }) else {
            return true;
        };
        let Some(bytes) = write_tag(tag) else {
            return true;
        };
        let id = e.id();
        let (mask, _) = Self::java_shape(&type_id, Some(e.kind()));
        let reply = self.java_entity_request("entity_view", |out| {
            out.int(id as i32);
            out.bool(origin == Origin::FromDisk);
            out.bytes(&bytes);
            out.int(mask as i32);
        });
        let Some(reply) = reply else {
            return true;
        };
        let mut reply = In(&reply);
        let joined = reply.bool().unwrap_or(true);
        self.java_apply_effects(&mut reply);
        if joined {
            VIEWED.with_borrow_mut(|v| v.insert((self.dim.id().to_owned(), id)));
        }
        joined
    }

    /// The leave hook: Java drops the view and sets its removal reason.
    pub(super) fn java_entity_left(&mut self, _kind: Kind, id: u32, why: Removal) {
        let dim = self.dim.id().to_owned();
        if !VIEWED.with_borrow_mut(|v| v.remove(&(dim, id))) {
            return;
        }
        if let Some(reply) = self.java_entity_request("entity_left", |out| {
            out.int(id as i32);
            out.0.push(removal_byte(why));
        }) {
            self.java_apply_effects(&mut In(&reply));
        }
    }

    /// Before native saves the entities in `keys`: Java's live keys of each go into its remembered compound.
    pub(super) fn java_entities_save(&mut self, keys: &std::collections::HashSet<(i32, i32)>) {
        if !self.java_level_running() {
            return;
        }
        let ids: Vec<u32> = self
            .level_candidates()
            .into_iter()
            .filter(|t| {
                self.target_position(*t).is_some_and(|p| {
                    keys.contains(&((p[0].floor() as i32) >> 4, (p[2].floor() as i32) >> 4))
                })
            })
            .filter_map(|t| kind_of(t).map(|k| k.1))
            .filter(|id| VIEWED.with_borrow(|v| v.contains(&(self.dim.id().to_owned(), *id))))
            .collect();
        if ids.is_empty() {
            return;
        }
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        let mut out = Out::default();
        out.text(self.dim.id());
        out.int(ids.len() as i32);
        for id in &ids {
            out.int(*id as i32);
        }
        let Some(reply) =
            horizon_java_host::bridge::with_active(self, || host.request("entities_save", &out.0))
        else {
            return;
        };
        let mut reply = In(&reply);
        for _ in 0..reply.int().unwrap_or(0) {
            let (Some(id), Some(bytes)) = (reply.int(), reply.bytes()) else {
                return;
            };
            self.java_merge_owned(id as u32, read_tag(bytes));
        }
    }

    /// Java's answer for one entity replaces the Java-owned keys of what native remembers of it.
    pub(super) fn java_merge_owned(&mut self, id: u32, owned: Option<Tag>) {
        let remembered = self
            .persist
            .raw
            .entry(id)
            .or_insert_with(|| Tag::Compound(Default::default()));
        let Tag::Compound(map) = remembered else {
            return;
        };
        for key in JAVA_KEYS {
            map.remove(&key.into());
        }
        if let Some(Tag::Compound(owned)) = owned {
            for (k, v) in owned {
                map.insert(k, v);
            }
        }
    }

    /// Sends the groups of `mask` of the entity `id` to its view, after a bridge changed them.
    pub(super) fn java_refresh_entity(&mut self, id: u32, mask: u32) {
        let Some(t) = self.java_target(id) else {
            return;
        };
        let Some((groups, _)) = self.java_groups(t, mask) else {
            return;
        };
        if let Some(reply) = self.java_entity_request("entity_refresh", |out| {
            out.int(id as i32);
            out.int(mask as i32);
            out.0.extend(groups);
        }) {
            self.java_apply_effects(&mut In(&reply));
        }
    }

    /// `EntityViews.spawn`: a mod's new entity into the level, the way `/summon` adds one; its id, 0 when refused.
    pub(super) fn java_spawn_entity(&mut self, saved: &[u8], species: &[u8]) -> u32 {
        let Some(tag) = read_tag(saved) else { return 0 };
        if text(tag.get("id")).as_deref() == Some("minecraft:lightning_bolt") {
            return self.java_spawn_bolt(&tag, species);
        }
        let before = self.next_entity;
        let _ = self.spawn_entity_from_nbt(tag);
        // The root of a spawned stack takes the first id.
        let id = before + 1;
        let joined = self.next_entity != before
            && ALL_KINDS
                .iter()
                .any(|kind| self.level_ids(*kind).contains(&id));
        if joined { id } else { 0 }
    }
}

/// The patch a bridge makes to an entity's saved form, then loaded back as `/data merge entity` does.
impl Game<'_> {
    pub(super) fn java_patch(
        &mut self,
        id: u32,
        patch: impl FnOnce(&mut horizon_world::nbt::Compound),
    ) -> bool {
        let Some(t) = self.java_target(id) else {
            return false;
        };
        let Some(Tag::Compound(mut map)) = self.entity_nbt(t) else {
            return false;
        };
        patch(&mut map);
        self.apply_entity_nbt(t, Tag::Compound(map)).is_ok()
    }

    pub(super) fn java_discard(&mut self, id: u32, why: Removal) {
        if let Some((kind, id)) = self.java_target(id).and_then(kind_of) {
            self.level_remove(kind, id, why);
        }
    }

    pub(super) fn java_kill(&mut self, id: u32) {
        if let Some(t) = self.java_target(id) {
            self.kill_target(t);
        }
    }

    /// `Entity.move` asked by Java for the entity `id`: false when it moved nothing.
    pub(super) fn java_move(&mut self, id: u32, mover: i32, delta: [f64; 3]) -> bool {
        let Some(target) = self.java_target(id) else {
            return false;
        };
        let mover = match mover {
            0 => horizon_world::entity_move::MoverType::SelfMovement,
            1 => horizon_world::entity_move::MoverType::Player,
            2 => horizon_world::entity_move::MoverType::Piston,
            3 => horizon_world::entity_move::MoverType::ShulkerBox,
            _ => horizon_world::entity_move::MoverType::Shulker,
        };
        let moved = self.entity_move(target, mover, Vec3::new(delta[0], delta[1], delta[2]));
        if !moved {
            log::warn!("Horizon: Entity.move of {target:?}: a player moves by its own packets");
        }
        moved
    }

    pub(super) fn java_set_motion(&mut self, id: u32, v: [f64; 3], add: bool) -> bool {
        let Some(t) = self.java_target(id) else {
            return false;
        };
        let vec = Vec3::new(v[0], v[1], v[2]);
        let set = |velocity: &mut Vec3| {
            *velocity = if add { velocity.add_vector(vec) } else { vec };
        };
        match t {
            Target::Mob(id) => {
                let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) else {
                    return false;
                };
                set(&mut m.velocity);
                m.needs_sync = true;
            }
            Target::Item(id) => {
                let Some(i) = self.dropped.iter_mut().find(|i| i.id == id) else {
                    return false;
                };
                set(&mut i.velocity);
            }
            Target::Orb(id) => {
                let Some(o) = self.orbs.iter_mut().find(|o| o.id == id) else {
                    return false;
                };
                set(&mut o.velocity);
            }
            Target::Projectile(id) => {
                let Some(p) = self.creatures.projectiles.iter_mut().find(|p| p.id == id) else {
                    return false;
                };
                set(&mut p.velocity);
                p.needs_sync = true;
            }
            Target::Boat(id) => {
                let Some(b) = self.boats.iter_mut().find(|b| b.id == id) else {
                    return false;
                };
                set(&mut b.velocity);
            }
            Target::Cart(id) => {
                let Some(c) = self.carts.iter_mut().find(|c| c.id == id) else {
                    return false;
                };
                set(&mut c.velocity);
            }
            Target::Stand(id) => {
                let Some(a) = self.armor_stands.iter_mut().find(|a| a.id == id) else {
                    return false;
                };
                set(&mut a.velocity);
            }
            _ => return false,
        }
        true
    }

    /// `Entity.setPos` and the teleports: the position, the previous one with it, and the rotation when given.
    pub(super) fn java_set_position(
        &mut self,
        id: u32,
        at: [f64; 3],
        facing: Option<[f32; 2]>,
    ) -> bool {
        let Some(t) = self.java_target(id) else {
            return false;
        };
        let at = Vec3::new(at[0], at[1], at[2]);
        match t {
            Target::Mob(id) => {
                let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) else {
                    return false;
                };
                m.position = at;
                m.previous = at;
                if let Some([yaw, pitch]) = facing {
                    m.yaw = yaw;
                    m.previous_yaw = yaw;
                    m.head_pitch = pitch;
                    m.previous_head_pitch = pitch;
                }
            }
            Target::Item(id) => {
                let Some(i) = self.dropped.iter_mut().find(|i| i.id == id) else {
                    return false;
                };
                i.position = at;
                i.previous = at;
            }
            Target::Orb(id) => {
                let Some(o) = self.orbs.iter_mut().find(|o| o.id == id) else {
                    return false;
                };
                o.position = at;
                o.previous = at;
            }
            Target::Projectile(id) => {
                let Some(p) = self.creatures.projectiles.iter_mut().find(|p| p.id == id) else {
                    return false;
                };
                p.position = at;
                p.previous = at;
                if let Some([yaw, pitch]) = facing {
                    p.yaw = yaw;
                    p.pitch = pitch;
                }
            }
            Target::Boat(id) => {
                let Some(b) = self.boats.iter_mut().find(|b| b.id == id) else {
                    return false;
                };
                b.position = at;
                b.previous = at;
            }
            Target::Cart(id) => {
                let Some(c) = self.carts.iter_mut().find(|c| c.id == id) else {
                    return false;
                };
                c.position = at;
                c.previous = at;
            }
            Target::Stand(id) => {
                let Some(a) = self.armor_stands.iter_mut().find(|a| a.id == id) else {
                    return false;
                };
                a.position = at;
            }
            _ => return false,
        }
        true
    }

    /// The damage `/damage` does to one entity: a mob's `damage`, a boat's or cart's `hurt`, an item's health.
    /// `LivingEntity.releaseUsingItem` on a mob: its kinetic weapon has no release, so only `stopUsingItem`.
    pub(super) fn java_mob_stop_using(&mut self, id: u32) -> bool {
        use horizon_client::mobs::spear_use;
        let Some(Target::Mob(mid)) = self.java_target(id) else {
            return false;
        };
        let Some(m) = self.mobs.iter_mut().find(|m| m.id == mid) else {
            return false;
        };
        let was_using = spear_use::is_using_item(m);
        spear_use::stop_using_item(m);
        if was_using {
            self.post_entity_event(
                "minecraft:item_interact_finish",
                horizon_client::mobs::Who::Mob(mid),
            );
        }
        true
    }

    /// A flattened `DamageSource` as the mob code takes it.
    fn java_damage(
        &mut self,
        source: &horizon_java_host::bridge::Source,
    ) -> horizon_client::mobs::Damage {
        let causing = (source.causing > 0).then_some(source.causing as u32);
        let attacker = causing.and_then(|c| {
            if self.mobs.iter().any(|m| m.id == c) {
                Some(horizon_client::mobs::Who::Mob(c))
            } else if self.network_entity_id(Target::Player) == Some(c as i32) {
                Some(horizon_client::mobs::Who::Player)
            } else {
                None
            }
        });
        horizon_client::mobs::Damage {
            kind: Self::damage_kind(&source.kind),
            attacker,
            attacker_species: None,
            from: source.at,
        }
    }

    /// `LivingEntity.actuallyHurt` on a mob: the health it took.
    pub(super) fn java_actually_hurt(
        &mut self,
        id: u32,
        source: &horizon_java_host::bridge::Source,
        amount: f32,
    ) -> bool {
        let Some(Target::Mob(mid)) = self.java_target(id) else {
            return false;
        };
        let damage = self.java_damage(source);
        let Some(m) = self.mobs.iter_mut().find(|m| m.id == mid) else {
            return false;
        };
        if m.actually_hurt(damage, amount, None) != 0.0 {
            self.mob_damage_event(mid);
        }
        true
    }

    pub(super) fn java_hurt_target(
        &mut self,
        id: u32,
        source: &horizon_java_host::bridge::Source,
        amount: f32,
    ) -> bool {
        let causing = (source.causing > 0).then_some(source.causing as u32);
        let Some(t) = self.java_target(id) else {
            return false;
        };
        match t {
            Target::Mob(mid) => {
                let damage = self.java_damage(source);
                let hurt = self.mobs.iter_mut().find(|m| m.id == mid).is_some_and(|m| {
                    m.damage(
                        amount,
                        damage,
                        horizon_client::mobs::anim::Ride::of(&self.riding_table, m.id),
                    )
                });
                if hurt {
                    self.resolve_player_responsible(mid, damage);
                }
                hurt
            }
            Target::Boat(bid) => self
                .boats
                .iter()
                .position(|b| b.id == bid)
                .is_some_and(|i| self.hurt_boat(i, amount, false)),
            Target::Cart(cid) => self
                .carts
                .iter()
                .position(|c| c.id == cid)
                .is_some_and(|i| {
                    self.hurt_cart(i, amount, false, source.kind.ends_with("on_fire"));
                    true
                }),
            Target::Item(iid) => {
                let source = super::explosion::HurtSource {
                    kind: source.kind.clone(),
                    causing_mob: causing.filter(|c| self.mobs.iter().any(|m| m.id == *c)),
                    creative: source.creative,
                };
                self.hurt_item_entity(iid, &source, amount)
            }
            _ => false,
        }
    }

    pub(super) fn java_knockback(&mut self, id: u32, power: f64, x: f64, z: f64) -> bool {
        match self.mobs.iter_mut().find(|m| m.id == id) {
            Some(m) => {
                let source =
                    horizon_client::mobs::Damage::of(horizon_client::mobs::DamageKind::Generic);
                m.knockback(power, x, z, &source);
                true
            }
            None => false,
        }
    }

    pub(super) fn java_set_health(&mut self, id: u32, health: f32) -> bool {
        let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) else {
            return false;
        };
        m.health = health.clamp(0.0, m.attrs.max_health);
        true
    }

    pub(super) fn java_ride(
        &mut self,
        rider: u32,
        vehicle: u32,
        force: bool,
        events: bool,
    ) -> bool {
        if !self.mobs.iter().any(|m| m.id == rider) {
            return false;
        }
        self.start_riding(Ent::Id(rider), Ent::Id(vehicle), force, events)
    }

    pub(super) fn java_dismount(&mut self, rider: u32) -> bool {
        if !self.mobs.iter().any(|m| m.id == rider) {
            return false;
        }
        self.stop_riding(Ent::Id(rider));
        true
    }

    /// A mod's lightning bolt: placed where its saved form says, with the damage, visual flag and flashes it carried.
    fn java_spawn_bolt(&mut self, tag: &Tag, species: &[u8]) -> u32 {
        let [x, y, z] = doubles::<3>(tag.get("Pos"));
        let mut r = In(species);
        let (Some(damage), Some(visual_only), Some(life), Some(flashes), Some(seed)) =
            (r.float(), r.bool(), r.int(), r.int(), r.long())
        else {
            return 0;
        };
        self.next_entity += 1;
        let id = self.next_entity;
        let uuid = self.uuid_for(id);
        let mut bolt = super::lightning::Bolt::new(id, [x, y, z], flashes, seed, visual_only, uuid);
        bolt.damage = damage;
        bolt.life = life;
        self.lightning_seen[0] += 1;
        self.level_add(NewEntity::Bolt(bolt), Origin::Fresh, None)
            .unwrap_or(0)
    }

    /// The bolt a Java `thunderHit` carries.
    pub(super) fn java_bolt_hit(bytes: &[u8]) -> Option<super::lightning::BoltHit> {
        let mut r = In(bytes);
        r.bool()?;
        r.int()?;
        let damage = r.float()?;
        r.bool()?;
        let (hi, lo) = (r.long()?, r.long()?);
        let at = [r.double()?, r.double()?, r.double()?];
        Some(super::lightning::BoltHit {
            damage,
            uuid: [(hi >> 32) as i32, hi as i32, (lo >> 32) as i32, lo as i32],
            at,
        })
    }

    pub(super) fn java_set_head(&mut self, id: u32, head: Option<f32>, body: Option<f32>) -> bool {
        let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) else {
            return false;
        };
        if let Some(head) = head {
            m.head_yaw = head;
            m.previous_head_yaw = head;
        }
        if let Some(body) = body {
            m.body_yaw = body;
            m.previous_body_yaw = body;
        }
        true
    }

    pub(super) fn java_set_old_position(&mut self, id: u32) {
        if let Some(at) = self.java_target(id).and_then(|t| self.target_position(t)) {
            self.java_set_position(id, at, None);
        }
    }

    pub(super) fn java_vehicle_of(&self, id: u32) -> Option<u32> {
        self.mob_vehicle(id)
    }

    /// `Entity.getKnownMovement` and `onGround` of a projectile's source.
    fn java_known_movement(&mut self, id: u32) -> Option<(Vec3, bool)> {
        if self.network_entity_id(Target::Player) == Some(id as i32) {
            let on_ground = self.player.as_ref()?.ground.on_ground;
            return Some((self.known_player_movement(), on_ground));
        }
        if let Some(g) = self
            .lan
            .as_ref()
            .and_then(|h| h.hub.guests.iter().find(|g| g.entity_id == id as i32))
        {
            let k = g.known_movement;
            return Some((Vec3::new(k[0], k[1], k[2]), g.on_ground));
        }
        match self.java_target(id)? {
            Target::Mob(id) => self
                .mobs
                .iter()
                .find(|m| m.id == id)
                .map(|m| (m.velocity, m.on_ground)),
            Target::Item(id) => self
                .dropped
                .iter()
                .find(|i| i.id == id)
                .map(|i| (i.velocity, i.on_ground)),
            Target::Orb(id) => self
                .orbs
                .iter()
                .find(|o| o.id == id)
                .map(|o| (o.velocity, o.moving.ground.on_ground)),
            Target::Boat(id) => self
                .boats
                .iter()
                .find(|b| b.id == id)
                .map(|b| (b.velocity, b.moving.ground.on_ground)),
            Target::Tnt(id) => self
                .tnts
                .iter()
                .find(|t| t.id == id)
                .map(|t| (t.velocity, t.on_ground)),
            _ => None,
        }
    }

    /// `Projectile.setOwner(owner)`: vanilla keeps only `EntityReference.of(owner)`, the owner's UUID (None for null).
    pub(super) fn java_set_owner(&mut self, id: u32, uuid: Option<[i32; 4]>) -> bool {
        let owner = uuid.map(|u| self.resolve_owner(Owner::Unloaded(u)));
        let Some(p) = self.creatures.projectiles.iter_mut().find(|p| p.id == id) else {
            return false;
        };
        (p.owner, p.owner_uuid) = (owner, uuid);
        if let Some(a) = p.arrow.as_mut() {
            a.set_owner(owner.and_then(Owner::target));
        }
        p.needs_sync = true;
        true
    }

    /// `Projectile.shootFromRotation(source, xRot, yRot, yOffset, pow, uncertainty)`.
    pub(super) fn java_shoot_from_rotation(&mut self, id: u32, source: u32, aim: [f32; 5]) -> bool {
        let Some((movement, on_ground)) = self.java_known_movement(source) else {
            return false;
        };
        let [pitch, yaw, offset, speed, uncertainty] = aim;
        let Some(p) = self.creatures.projectiles.iter_mut().find(|p| p.id == id) else {
            return false;
        };
        p.shoot_from_rotation(
            &mut self.world.random,
            pitch,
            yaw,
            offset,
            speed,
            uncertainty,
            movement,
            on_ground,
        );
        p.needs_sync = true;
        true
    }

    /// `Entity.teleport` inside the level: the position, rotation and motion the transition says. Another level has no native travel to call.
    pub(super) fn java_teleport(&mut self, id: u32, transition: &[u8]) -> bool {
        let mut r = In(transition);
        let (Some(at), Some(facing), Some(delta), Some(dim)) = (
            (|| Some([r.double()?, r.double()?, r.double()?]))(),
            (|| Some([r.float()?, r.float()?]))(),
            (|| Some([r.double()?, r.double()?, r.double()?]))(),
            r.text(),
        ) else {
            return false;
        };
        if dim != self.dim.id() {
            log::warn!(
                "Horizon: a Java mod teleported an entity to {dim}, which has no native entity travel"
            );
            return false;
        }
        self.java_set_position(id, at, Some(facing)) && self.java_set_motion(id, delta, false)
    }

    pub(super) fn java_uuid_tag(text: &str) -> Option<Tag> {
        uuid_tag(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compound(entries: Vec<(&str, Tag)>) -> Tag {
        Tag::compound(entries)
    }

    fn doubles_tag(values: &[f64]) -> Tag {
        Tag::List(values.iter().map(|v| Tag::Double(v.to_bits())).collect())
    }

    /// The reader of Java's `EntityViews.apply`, in Rust: every group in bit order.
    fn read_back(bytes: &[u8], mask: u32, shape: Shape) -> Vec<String> {
        let mut r = In(bytes);
        let mut seen = Vec::new();
        if mask & POSITION != 0 {
            seen.push(format!(
                "pos {} {} {} {} {} {} {}",
                r.double().unwrap(),
                r.double().unwrap(),
                r.double().unwrap(),
                r.float().unwrap(),
                r.float().unwrap(),
                r.float().unwrap(),
                r.float().unwrap()
            ));
        }
        if mask & MOTION != 0 {
            seen.push(format!(
                "motion {} {} {} {}",
                r.double().unwrap(),
                r.double().unwrap(),
                r.double().unwrap(),
                r.bool().unwrap()
            ));
        }
        if mask & HEALTH != 0 && r.bool().unwrap() {
            assert!(shape.living);
            let health = (r.float().unwrap(), r.float().unwrap());
            let times = (r.take_i16(), r.take_i16());
            seen.push(format!("health {health:?} {times:?}"));
        }
        if mask & EFFECTS != 0 {
            for _ in 0..r.int().unwrap() {
                seen.push(format!(
                    "effect {} {} {} {} {} {}",
                    r.text().unwrap(),
                    r.int().unwrap(),
                    r.int().unwrap(),
                    r.bool().unwrap(),
                    r.bool().unwrap(),
                    r.bool().unwrap()
                ));
            }
        }
        if mask & EQUIPMENT != 0 {
            for _ in 0..r.int().unwrap() {
                let slot = r.byte().unwrap();
                seen.push(format!("slot {slot} {}", !r.bytes().unwrap().is_empty()));
            }
        }
        if mask & FIRE != 0 {
            seen.push(format!("fire {}", r.int().unwrap()));
        }
        if mask & AIR != 0 {
            seen.push(format!("air {}", r.int().unwrap()));
        }
        if mask & FLAGS != 0 {
            seen.push(format!("flags {}", r.int().unwrap()));
        }
        if mask & NAME != 0 {
            if r.bool().unwrap() {
                seen.push(format!("name {}", !r.bytes().unwrap().is_empty()));
            }
            for _ in 0..r.int().unwrap() {
                seen.push(format!("tag {}", r.text().unwrap()));
            }
        }
        if mask & ITEM != 0 {
            assert_eq!(r.bool(), Some(shape.item));
        }
        if mask & RIDING != 0 {
            seen.push(format!("vehicle {}", r.int().unwrap()));
            for _ in 0..r.int().unwrap() {
                seen.push(format!("passenger {}", r.int().unwrap()));
            }
        }
        if mask & REMOVAL != 0 {
            seen.push(format!("removal {}", r.byte().unwrap()));
        }
        assert!(r.0.is_empty(), "{} bytes left", r.0.len());
        seen
    }

    trait TakeI16 {
        fn take_i16(&mut self) -> i16;
    }
    impl TakeI16 for In<'_> {
        fn take_i16(&mut self) -> i16 {
            let (head, rest) = self.0.split_at(2);
            self.0 = rest;
            i16::from_be_bytes([head[0], head[1]])
        }
    }

    #[test]
    fn every_group_of_a_mob_reads_back_in_bit_order() {
        let tag = compound(vec![
            ("Pos", doubles_tag(&[1.5, 64.0, -2.5])),
            (
                "Rotation",
                Tag::List(vec![
                    Tag::Float(90.0f32.to_bits()),
                    Tag::Float(10.0f32.to_bits()),
                ]),
            ),
            ("Motion", doubles_tag(&[0.0, 0.5, 0.0])),
            ("OnGround", Tag::Byte(1)),
            ("Health", Tag::Float(1.5f32.to_bits())),
            ("Fire", Tag::Short(20)),
            ("Air", Tag::Short(250)),
            ("Silent", Tag::Byte(1)),
            ("CustomName", Tag::String("Bob".into())),
            ("Tags", Tag::List(vec![Tag::String("a".into())])),
            (
                "active_effects",
                Tag::List(vec![compound(vec![
                    ("id", Tag::String("minecraft:speed".into())),
                    ("amplifier", Tag::Byte(1)),
                    ("duration", Tag::Int(100)),
                ])]),
            ),
            (
                "equipment",
                compound(vec![(
                    "head",
                    compound(vec![
                        ("id", Tag::String("minecraft:stone".into())),
                        ("count", Tag::Int(1)),
                    ]),
                )]),
            ),
        ]);
        let over = Overlay {
            head: Some(80.0),
            body: Some(70.0),
            hurt: Some(3),
            death: Some(0),
            vehicle: 9,
            passengers: vec![4, 5],
            ..Overlay::default()
        };
        let mut out = Out::default();
        let shape = Shape {
            living: true,
            item: false,
        };
        write_groups(&mut out, &tag, &over, shape, 0x1ffe);
        let seen = read_back(&out.0, 0x1ffe, shape);
        assert_eq!(seen[0], "pos 1.5 64 -2.5 90 10 80 70");
        assert_eq!(seen[1], "motion 0 0.5 0 true");
        assert_eq!(seen[2], "health (1.5, 0.0) (3, 0)");
        assert_eq!(seen[3], "effect minecraft:speed 1 100 false true true");
        assert_eq!(seen[4], "slot 5 true");
        assert!(seen.contains(&"fire 20".to_string()) && seen.contains(&"air 250".to_string()));
        assert!(seen.contains(&"flags 17".to_string()));
        assert!(seen.contains(&"name true".to_string()) && seen.contains(&"tag a".to_string()));
        assert_eq!(
            &seen[seen.len() - 4..],
            ["vehicle 9", "passenger 4", "passenger 5", "removal 0"]
        );
    }

    #[test]
    fn the_ai_and_arrow_groups_read_back() {
        let mut out = Out::default();
        let over = Overlay {
            malus: vec![(9, 0.0), (16, 8.0)],
            species: SpeciesData::Projectile {
                base_damage: Some(7.0),
                owner: Some([0, 3, 0, 4]),
            },
            ..Overlay::default()
        };
        write_groups(
            &mut out,
            &Tag::compound(Vec::<(&str, Tag)>::new()),
            &over,
            Shape::default(),
            SPECIES | AI,
        );
        let mut r = In(&out.0);
        assert_eq!(r.double(), Some(7.0));
        assert_eq!(
            (r.bool(), r.long(), r.long()),
            (Some(true), Some(3), Some(4))
        );
        assert_eq!(r.int(), Some(2));
        assert_eq!((r.byte(), r.float()), (Some(9), Some(0.0)));
        assert_eq!((r.byte(), r.float()), (Some(16), Some(8.0)));
    }

    #[test]
    fn a_thing_that_is_not_living_writes_no_health() {
        let tag = compound(vec![("Health", Tag::Float(1.0f32.to_bits()))]);
        let mut out = Out::default();
        write_groups(
            &mut out,
            &tag,
            &Overlay::default(),
            Shape::default(),
            HEALTH | FIRE,
        );
        assert_eq!(out.0, [0, 0, 0, 0, 0]);
    }

    #[test]
    fn an_item_entity_writes_its_stack_delay_age_and_both_uuids() {
        let tag = compound(vec![
            (
                "Item",
                compound(vec![("id", Tag::String("minecraft:stone".into()))]),
            ),
            ("PickupDelay", Tag::Short(10)),
            ("Age", Tag::Short(5)),
            ("Owner", Tag::IntArray(vec![0, 1, 0, 2])),
        ]);
        let mut out = Out::default();
        write_groups(
            &mut out,
            &tag,
            &Overlay::default(),
            Shape {
                living: false,
                item: true,
            },
            ITEM,
        );
        let mut r = In(&out.0);
        assert_eq!(r.bool(), Some(true));
        assert!(!r.bytes().unwrap().is_empty());
        assert_eq!(r.take_i16(), 10);
        assert_eq!(r.int(), Some(5));
        assert_eq!(r.bool(), Some(false));
        assert_eq!(r.bool(), Some(true));
        assert_eq!((r.long(), r.long()), (Some(1), Some(2)));
        assert!(r.0.is_empty());
    }

    #[test]
    fn a_save_replaces_only_the_java_owned_keys_of_what_native_remembers() {
        let owned = compound(vec![("NeoForgeData", compound(vec![("n", Tag::Int(7))]))]);
        let mut raw = std::collections::HashMap::new();
        raw.insert(
            3_u32,
            compound(vec![
                ("neoforge:attachments", compound(vec![])),
                ("Custom", Tag::Int(1)),
            ]),
        );
        let mut remembered = raw.remove(&3).unwrap();
        let Tag::Compound(map) = &mut remembered else {
            unreachable!()
        };
        for key in JAVA_KEYS {
            map.remove(&key.into());
        }
        let Tag::Compound(owned) = owned else {
            unreachable!()
        };
        for (k, v) in owned {
            map.insert(k, v);
        }
        assert!(remembered.get("neoforge:attachments").is_none());
        assert!(remembered.get("NeoForgeData").is_some() && remembered.get("Custom").is_some());
    }

    #[test]
    fn the_species_group_of_a_tnt_and_a_bolt_reads_back() {
        let tag = compound(vec![]);
        let tnt = Overlay {
            species: SpeciesData::Tnt {
                fuse: 150,
                java_state: 9,
                power: 4.0,
                owner: Some([0, 1, 0, 2]),
            },
            ..Overlay::default()
        };
        let mut out = Out::default();
        write_groups(&mut out, &tag, &tnt, Shape::default(), SPECIES);
        let mut r = In(&out.0);
        assert_eq!(
            (r.int(), r.int(), r.float()),
            (Some(150), Some(9), Some(4.0))
        );
        assert_eq!(
            (r.bool(), r.long(), r.long()),
            (Some(true), Some(1), Some(2))
        );
        assert!(r.0.is_empty());
        let bolt = Overlay {
            species: SpeciesData::Bolt {
                damage: 2.0,
                visual_only: true,
                life: 1,
                flashes: 3,
                seed: -7,
            },
            ..Overlay::default()
        };
        let mut out = Out::default();
        write_groups(&mut out, &tag, &bolt, Shape::default(), SPECIES);
        let mut r = In(&out.0);
        assert_eq!((r.float(), r.bool()), (Some(2.0), Some(true)));
        assert_eq!((r.int(), r.int(), r.long()), (Some(1), Some(3), Some(-7)));
        assert!(r.0.is_empty());
    }

    #[test]
    fn a_tnt_saves_and_loads_what_vanilla_does() {
        let t = entity_nbt::TntNbt {
            position: horizon_core::geometry::Vec3::new(1.0, 2.0, 3.0),
            velocity: horizon_core::geometry::Vec3::ZERO,
            on_ground: true,
            fuse: 33,
            state: Some(compound(vec![(
                "Name",
                Tag::String("minecraft:obsidian".into()),
            )])),
            power: 6.5,
            owner: Some([1, 2, 3, 4]),
        };
        let tag = entity_nbt::primed_tnt_to_nbt(&t, [9, 9, 9, 9], None);
        assert_eq!(number(tag.get("fuse")), Some(33));
        let back = entity_nbt::tnt_from_nbt(&tag).unwrap();
        assert_eq!(
            (back.fuse, back.power, back.owner),
            (33, 6.5, Some([1, 2, 3, 4]))
        );
        assert!(back.state.is_some() && back.on_ground);
        let plain = entity_nbt::TntNbt {
            power: 4.0,
            owner: None,
            ..t
        };
        let tag = entity_nbt::primed_tnt_to_nbt(&plain, [9, 9, 9, 9], None);
        assert!(tag.get("explosion_power").is_none() && tag.get("owner").is_none());
        let clamped = compound(vec![
            ("Pos", doubles_tag(&[0.0, 0.0, 0.0])),
            ("explosion_power", Tag::Float(500.0f32.to_bits())),
        ]);
        let loaded = entity_nbt::tnt_from_nbt(&clamped).unwrap();
        assert_eq!((loaded.fuse, loaded.power), (80, 128.0));
    }

    #[test]
    fn uuid_text_and_longs_agree() {
        let tag = uuid_tag("00000000-0000-0001-0000-000000000002").unwrap();
        assert_eq!(uuid_longs(Some(&tag)), Some((1, 2)));
    }
}
