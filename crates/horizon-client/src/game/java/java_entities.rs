// game-module-cfg: feature java-mods
//! Native entities as live Java views: one per entity from its join to its leave, refreshed raw, written through by bridges.

use super::java_mods::{read_tag, write_tag};
use super::*;
use commands::Target;
use horizon_client::projectiles::ProjectileKind;
use horizon_client::save::{float, number, text};
use horizon_core::nbt::{Compound, Tag};
use horizon_java_host::bridge::Source;
use horizon_java_host::wire::{In, Out};

pub(in crate::game) const POSITION: u32 = 2;
pub(in crate::game) const MOTION: u32 = 4;
pub(in crate::game) const HEALTH: u32 = 8;
const EFFECTS: u32 = 16;
pub(in crate::game) const EQUIPMENT: u32 = 32;
pub(in crate::game) const FIRE: u32 = 64;
pub(in crate::game) const AIR: u32 = 128;
pub(in crate::game) const FLAGS: u32 = 256;
pub(in crate::game) const NAME: u32 = 512;
pub(in crate::game) const ITEM: u32 = 1024;
pub(in crate::game) const RIDING: u32 = 2048;
pub(in crate::game) const REMOVAL: u32 = 4096;
pub(in crate::game) const SPECIES: u32 = 8192;
pub(in crate::game) const AI: u32 = 32768;

/// The keys of an entity's saved form that live in Java: attachments and persistent data.
const JAVA_KEYS: [&str; 2] = ["neoforge:attachments", "NeoForgeData"];
const SLOTS: [&str; 8] = [
    "mainhand", "offhand", "feet", "legs", "chest", "head", "body", "saddle",
];

/// `Entity.RemovalReason`, in its declaration order: the byte Java's view is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(in crate::game) enum Removal {
    Killed,
    Discarded,
    UnloadedToChunk,
    UnloadedWithPlayer,
    ChangedDimension,
}

impl Removal {
    pub(in crate::game) fn of_ordinal(ordinal: i32) -> Self {
        match ordinal {
            0 => Self::Killed,
            1 => Self::Discarded,
            2 => Self::UnloadedToChunk,
            3 => Self::UnloadedWithPlayer,
            _ => Self::ChangedDimension,
        }
    }
}

/// A UUID as Horizon's four ints, as one number.
pub(in crate::game) fn uuid_bits(uuid: [i32; 4]) -> u128 {
    uuid.iter()
        .fold(0, |bits, part| (bits << 32) | u128::from(*part as u32))
}

thread_local! {
    static VIEWED: std::cell::RefCell<std::collections::HashSet<(String, u32)>> = Default::default();
}

pub(in crate::game) fn reset_views() {
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

/// The id a view is keyed by, for the kinds Java holds views of.
fn viewed_id(t: Target) -> Option<u32> {
    match t {
        Target::Mob(id)
        | Target::Item(id)
        | Target::Orb(id)
        | Target::Projectile(id)
        | Target::Stand(id)
        | Target::Frame(id)
        | Target::Painting(id)
        | Target::Boat(id)
        | Target::Cart(id)
        | Target::Other(id)
        | Target::Crystal(id)
        | Target::Cushion(id)
        | Target::Dragon(id)
        | Target::Tnt(id)
        | Target::Bolt(id) => Some(id),
        Target::Player
        | Target::Guest(_)
        | Target::Fangs(_)
        | Target::Falling(_)
        | Target::Rocket(_)
        | Target::Trident(_)
        | Target::Spawner(_)
        | Target::Cloud(_)
        | Target::DragonPart(_)
        | Target::Hook(_)
        | Target::Knot(_) => None,
    }
}

/// Java's own keys of an entity replace those in what native remembers of it.
fn merge_owned(remembered: &mut Tag, owned: Option<Tag>) {
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

impl Game<'_> {
    fn java_facts() -> Option<&'static horizon_java_host::entity_facts::EntityFacts> {
        horizon_java_host::entity_facts::get()
    }

    /// The groups a type's views carry, and whether Java's class is living or an item entity: the target says it, the facts for the types Horizon only keeps as data.
    fn java_shape(type_id: &str, t: Option<Target>) -> (u32, Shape) {
        let known = Shape {
            living: matches!(
                t,
                Some(Target::Mob(_) | Target::Stand(_) | Target::Dragon(_))
            ),
            item: matches!(t, Some(Target::Item(_))),
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
        if matches!(t, Some(Target::Mob(_))) {
            shape.0 |= AI;
        }
        if matches!(t, Some(Target::Tnt(_) | Target::Bolt(_)))
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
    fn java_tnt_owner_uuid(
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
                let default_tnt = self.pack.assets.registry.default_state("minecraft:tnt");
                return Overlay {
                    species: self
                        .tnts
                        .iter()
                        .find(|t| t.id == id)
                        .map_or(SpeciesData::None, |t| SpeciesData::Tnt {
                            fuse: t.fuse,
                            java_state: t
                                .block_state
                                .or(default_tnt)
                                .and_then(|state| {
                                    horizon_java_host::bridge::STATES
                                        .get()?
                                        .to_java
                                        .get(state.0 as usize)
                                        .copied()
                                })
                                .unwrap_or(horizon_java_host::bridge::UNMAPPED),
                            power: t.power,
                            owner: self.java_tnt_owner_uuid(t.owner, t.guest),
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
                            owner: p.owner_uuid,
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
        let handle = self.entity(id);
        Overlay {
            species: SpeciesData::None,
            head: Some(m.head_yaw),
            body: Some(m.body_yaw),
            hurt: Some(m.hurt_time),
            death: Some(m.death_time),
            crouching: m.extra.crouching,
            sprinting: m.extra.sprinting,
            vehicle: handle
                .and_then(|e| self.entity_vehicle(e))
                .map_or(0, |v| v.id()),
            passengers: handle
                .and_then(|e| self.entity_passengers(e))
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.id())
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

    /// The group bytes of the entity `t` for `mask`, and its saved form beside them.
    fn java_groups(&mut self, t: Target, mask: u32) -> Option<(Vec<u8>, Tag)> {
        let type_id = self.target_type(t);
        let tag = self.command_target_nbt(t)?;
        let over = self.java_overlay(t);
        let (_, shape) = Self::java_shape(&type_id, Some(t));
        let mut out = Out::default();
        write_groups(&mut out, &tag, &over, shape, mask);
        let mut tag = tag;
        if let Tag::Compound(map) = &mut tag {
            map.insert("id".into(), Tag::String(type_id.as_str().into()));
        }
        Some((out.0, tag))
    }

    fn java_view_reply(&mut self, t: Target, out: &mut Out) -> bool {
        let Some(id) = viewed_id(t) else {
            return false;
        };
        let type_id = self.target_type(t);
        let (mask, _) = Self::java_shape(&type_id, Some(t));
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

    fn java_entity_request(&mut self, op: &str, body: impl FnOnce(&mut Out)) -> Option<Vec<u8>> {
        self.java_bind_states();
        let mut out = Out::default();
        out.text(self.dim.id());
        out.long(self.world.game_time);
        body(&mut out);
        self.java_request(op, &out.0)
    }

    /// The java-mods arm of `native_entity_join_level`: Java makes the entity's view, and its verdict is the join's.
    pub(in crate::game) fn java_entity_joining(&mut self, id: u32, loaded_from_disk: bool) -> bool {
        if !self.java_level_running() {
            return true;
        }
        let Some(t) = self.java_target(id) else {
            return true;
        };
        let type_id = self.target_type(t);
        let Some(bytes) = self.java_groups(t, 0).and_then(|(_, tag)| write_tag(tag)) else {
            return true;
        };
        let (mask, _) = Self::java_shape(&type_id, Some(t));
        let reply = self.java_entity_request("entity_view", |out| {
            out.int(id as i32);
            out.bool(loaded_from_disk);
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
    pub(in crate::game) fn java_entity_left(&mut self, id: u32, why: Removal) {
        let dim = self.dim.id().to_owned();
        if !VIEWED.with_borrow_mut(|v| v.remove(&(dim, id))) {
            return;
        }
        if let Some(reply) = self.java_entity_request("entity_left", |out| {
            out.int(id as i32);
            out.0.push(why as u8);
        }) {
            self.java_apply_effects(&mut In(&reply));
        }
    }

    /// Before native saves the entities in `keys`: Java's live keys of each go into its remembered compound.
    pub(in crate::game) fn java_entities_save(
        &mut self,
        keys: &std::collections::HashSet<(i32, i32)>,
    ) {
        if !self.java_level_running() {
            return;
        }
        let dim = self.dim.id().to_owned();
        let ids: Vec<u32> = self
            .level_candidates()
            .into_iter()
            .filter(|t| {
                self.target_position(*t).is_some_and(|p| {
                    keys.contains(&((p[0].floor() as i32) >> 4, (p[2].floor() as i32) >> 4))
                })
            })
            .filter_map(viewed_id)
            .filter(|id| VIEWED.with_borrow(|v| v.contains(&(dim.clone(), *id))))
            .collect();
        if ids.is_empty() {
            return;
        }
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        let mut out = Out::default();
        out.text(&dim);
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
            let remembered = self
                .persist
                .raw
                .entry(id as u32)
                .or_insert_with(|| Tag::Compound(Default::default()));
            merge_owned(remembered, read_tag(bytes));
        }
    }

    /// Sends the groups of `mask` of the entity `id` to its view, after a bridge changed them.
    pub(in crate::game) fn java_refresh_entity(&mut self, id: u32, mask: u32) {
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

    /// `EntityViews.spawn`: a mod's new entity into the level the way `/summon` adds one; its id, 0 when refused.
    pub(in crate::game) fn java_spawn_entity(&mut self, saved: &[u8], species: &[u8]) -> u32 {
        let Some(Tag::Compound(mut tag)) = read_tag(saved) else {
            return 0;
        };
        let Some(kind) = text(tag.get(&"id".into())) else {
            return 0;
        };
        let pos = doubles::<3>(tag.get(&"Pos".into()));
        if kind == "minecraft:lightning_bolt" {
            return self.java_spawn_bolt(pos, species);
        }
        tag.remove(&"id".into());
        let dim = self.dim.id().to_owned();
        self.summon(&dim, &kind, pos, tag).unwrap_or(0)
    }
}

/// What a bridge changes on an entity's saved form, loaded back as `/data merge entity` does.
impl Game<'_> {
    pub(in crate::game) fn java_patch(
        &mut self,
        id: u32,
        patch: impl FnOnce(&mut Compound),
    ) -> bool {
        let Some(t) = self.java_target(id) else {
            return false;
        };
        let Some(Tag::Compound(mut map)) = self.command_target_nbt(t) else {
            return false;
        };
        patch(&mut map);
        self.apply_entity_nbt(t, Tag::Compound(map)).is_ok()
    }

    /// `Entity.setPos`, `setOldPos` and their kin: the position and the previous one, the rotation when given.
    pub(in crate::game) fn java_set_position(
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
                m.needs_sync = true;
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
                let Some(i) = self.creatures.projectiles.iter().position(|p| p.id == id) else {
                    return false;
                };
                self.creatures.projectiles.update(i, |p| {
                    p.position = at;
                    p.previous = at;
                    if let Some([yaw, pitch]) = facing {
                        p.yaw = yaw;
                        p.pitch = pitch;
                    }
                    p.needs_sync = true;
                });
            }
            Target::Boat(id) => {
                let Some(i) = self.boats.iter().position(|b| b.id == id) else {
                    return false;
                };
                self.boats
                    .update(i, |b| (b.position, b.previous) = (at, at));
            }
            Target::Cart(id) => {
                let Some(i) = self.carts.iter().position(|c| c.id == id) else {
                    return false;
                };
                self.carts
                    .update(i, |c| (c.position, c.previous) = (at, at));
            }
            _ => {
                let handle = self.entity(id);
                return handle
                    .is_some_and(|e| self.entity_teleport_to(e, [at.x, at.y, at.z], facing));
            }
        }
        true
    }

    /// `LivingEntity.releaseUsingItem` on a mob: its kinetic weapon has no release, so only `stopUsingItem`.
    pub(in crate::game) fn java_mob_stop_using(&mut self, id: u32) -> bool {
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
    fn java_damage(&self, source: &Source) -> horizon_client::mobs::Damage {
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
        let catalog = &self.motion.health.damage;
        horizon_client::mobs::Damage {
            kind: Self::damage_kind(catalog, &source.kind),
            attacker,
            from: source.at,
            ..horizon_client::mobs::Damage::of(Self::damage_kind(catalog, &source.kind))
        }
    }

    /// `LivingEntity.actuallyHurt` on a mob: true when it was a mob.
    pub(in crate::game) fn java_actually_hurt(
        &mut self,
        id: u32,
        source: &Source,
        amount: f32,
    ) -> bool {
        let Some(Target::Mob(mid)) = self.java_target(id) else {
            return false;
        };
        let damage = self.java_damage(source);
        let Some(m) = self.mobs.iter_mut().find(|m| m.id == mid) else {
            return false;
        };
        let mut container = horizon_client::health::DamageContainer::new(amount);
        let level = super::entity::damage_level_of!(self);
        if m.actually_hurt(level, &mut container, damage, amount) {
            self.mob_damage_event(mid);
        }
        true
    }

    /// `LivingEntity.knockback` on a mob.
    pub(in crate::game) fn java_knockback(&mut self, id: u32, power: f64, x: f64, z: f64) -> bool {
        let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) else {
            return false;
        };
        let source = horizon_client::mobs::Damage::of(horizon_client::mobs::DamageKind::Generic);
        m.knockback(super::entity::damage_level_of!(self), power, x, z, &source);
        true
    }

    /// `LivingEntity.setYHeadRot` and `setYBodyRot` on a mob.
    pub(in crate::game) fn java_set_head(
        &mut self,
        id: u32,
        head: Option<f32>,
        body: Option<f32>,
    ) -> bool {
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

    /// A mod's lightning bolt: placed where its saved form says, with the damage, visual flag and flashes it carried.
    fn java_spawn_bolt(&mut self, at: [f64; 3], species: &[u8]) -> u32 {
        let mut r = In(species);
        let (Some(damage), Some(visual_only), Some(life), Some(flashes), Some(seed)) =
            (r.float(), r.bool(), r.int(), r.int(), r.long())
        else {
            return 0;
        };
        let id = self.spawn_bolt(at, visual_only);
        if let Some(b) = self.bolts.iter_mut().find(|b| b.id == id) {
            (b.damage, b.life, b.flashes, b.seed) = (damage, life, flashes, seed);
        }
        id
    }

    /// The damage and position of the bolt a Java `thunderHit` carries.
    pub(in crate::game) fn java_bolt_hit(bytes: &[u8]) -> Option<(f32, [f64; 3])> {
        let mut r = In(bytes);
        r.bool()?;
        r.int()?;
        let damage = r.float()?;
        r.bool()?;
        let _uuid = (r.long()?, r.long()?);
        Some((damage, [r.double()?, r.double()?, r.double()?]))
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
        let handle = self.entity(id)?;
        let velocity = self.entity_velocity(handle)?;
        let on_ground = self.entity_on_ground(handle)?;
        Some((Vec3::new(velocity[0], velocity[1], velocity[2]), on_ground))
    }

    /// `Projectile.setOwner(owner)`: vanilla keeps only `EntityReference.of(owner)`, the owner's UUID (None for null).
    pub(in crate::game) fn java_set_owner(&mut self, id: u32, uuid: Option<[i32; 4]>) -> bool {
        let Some(projectile) = self.entity(id).and_then(|e| self.projectile(e)) else {
            return false;
        };
        let owner = uuid
            .map(uuid_bits)
            .and_then(|u| self.target_by_uuid(u))
            .and_then(|t| self.network_entity_id(t))
            .and_then(|e| self.entity(e as u32));
        if uuid.is_none() || owner.is_some() {
            return self.projectile_set_owner(projectile, owner);
        }
        // An owner not loaded is known by its UUID until it is, as `EntityReference` keeps it.
        let Some(i) = self.creatures.projectiles.iter().position(|p| p.id == id) else {
            return false;
        };
        self.creatures.projectiles.update(i, |p| {
            (p.owner, p.owner_uuid) = (None, uuid);
            p.needs_sync = true;
        });
        true
    }

    /// `Projectile.shootFromRotation(source, xRot, yRot, yOffset, pow, uncertainty)`.
    pub(in crate::game) fn java_shoot_from_rotation(
        &mut self,
        id: u32,
        source: u32,
        aim: [f32; 5],
    ) -> bool {
        let Some((movement, on_ground)) = self.java_known_movement(source) else {
            return false;
        };
        let [pitch, yaw, offset, speed, uncertainty] = aim;
        let Some(i) = self.creatures.projectiles.iter().position(|p| p.id == id) else {
            return false;
        };
        let random = &mut self.world.random;
        self.creatures.projectiles.update(i, |p| {
            p.shoot_from_rotation(
                random,
                pitch,
                yaw,
                offset,
                speed,
                uncertainty,
                movement,
                on_ground,
            );
            p.needs_sync = true;
        });
        true
    }

    /// `Entity.teleport` inside the level: the position, rotation and motion the transition says. Another level has no native travel to call.
    pub(in crate::game) fn java_teleport(&mut self, id: u32, transition: &[u8]) -> bool {
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
        let Some(handle) = self.entity(id) else {
            return false;
        };
        self.entity_teleport_to(handle, at, Some(facing)) && self.entity_set_velocity(handle, delta)
    }

    pub(in crate::game) fn java_uuid_tag(text: &str) -> Option<Tag> {
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
        let mut remembered = compound(vec![
            ("neoforge:attachments", compound(vec![])),
            ("Custom", Tag::Int(1)),
        ]);
        let owned = compound(vec![("NeoForgeData", compound(vec![("n", Tag::Int(7))]))]);
        merge_owned(&mut remembered, Some(owned));
        assert!(remembered.get("neoforge:attachments").is_none());
        assert!(remembered.get("NeoForgeData").is_some() && remembered.get("Custom").is_some());
    }

    #[test]
    fn a_uuid_reads_as_one_number_high_int_first() {
        assert_eq!(uuid_bits([0, 1, 0, 2]), (1u128 << 64) | 2);
        assert_eq!(uuid_bits([-1, 0, 0, 0]), 0xffff_ffffu128 << 96);
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
    fn uuid_text_and_longs_agree() {
        let tag = uuid_tag("00000000-0000-0001-0000-000000000002").unwrap();
        assert_eq!(uuid_longs(Some(&tag)), Some((1, 2)));
    }
}
