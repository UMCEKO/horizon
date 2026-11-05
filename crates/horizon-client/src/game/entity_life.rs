//! Where native entities enter and leave a level, as `PersistentEntitySectionManager.addEntity` and `Entity.setRemoved` do.
use super::*;
use horizon_client::riding::Ent;
use horizon_world::nbt::Tag;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum Kind {
    Mob,
    Item,
    Orb,
    Projectile,
    Stand,
    Frame,
    Painting,
    Boat,
    Cart,
    Other,
    Crystal,
    Cushion,
    Dragon,
    Tnt,
    Bolt,
}

/// `addEntity`'s `loaded`: read from a chunk's saved entities, or new to the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum Origin {
    Fresh,
    FromDisk,
}

/// `Entity.RemovalReason`, in its declaration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum Removal {
    Killed,
    Discarded,
    UnloadedToChunk,
    UnloadedWithPlayer,
    ChangedDimension,
}

impl Removal {
    /// `RemovalReason.shouldDestroy`.
    pub(in crate::game) fn destroys(self) -> bool {
        matches!(self, Self::Killed | Self::Discarded)
    }
}

pub(in crate::game) enum NewEntity {
    Mob(Mob),
    Item(ItemEntity),
    Orb(ExperienceOrb),
    Projectile(horizon_client::projectiles::Projectile),
    Stand(horizon_client::entities::ArmorStand),
    Frame(horizon_client::entities::ItemFrame),
    Painting(horizon_client::entities::Painting),
    Boat(boats::Boat),
    Cart(minecarts::Minecart),
    Other(commands::OtherEntity),
    Crystal(dragon::Crystal),
    Cushion(horizon_client::entities::Cushion),
    Dragon(Box<dragon::Dragon>),
    Tnt(tnt::PrimedTnt),
    Bolt(lightning::Bolt),
}

impl NewEntity {
    #[allow(dead_code)] // The java-mods join hook reads it; tests use it without the feature.
    pub(in crate::game) fn kind(&self) -> Kind {
        match self {
            Self::Mob(_) => Kind::Mob,
            Self::Item(_) => Kind::Item,
            Self::Orb(_) => Kind::Orb,
            Self::Projectile(_) => Kind::Projectile,
            Self::Stand(_) => Kind::Stand,
            Self::Frame(_) => Kind::Frame,
            Self::Painting(_) => Kind::Painting,
            Self::Boat(_) => Kind::Boat,
            Self::Cart(_) => Kind::Cart,
            Self::Other(_) => Kind::Other,
            Self::Crystal(_) => Kind::Crystal,
            Self::Cushion(_) => Kind::Cushion,
            Self::Dragon(_) => Kind::Dragon,
            Self::Tnt(_) => Kind::Tnt,
            Self::Bolt(_) => Kind::Bolt,
        }
    }
    pub(in crate::game) fn id(&self) -> u32 {
        match self {
            Self::Mob(e) => e.id,
            Self::Item(e) => e.id,
            Self::Orb(e) => e.id,
            Self::Projectile(e) => e.id,
            Self::Stand(e) => e.id,
            Self::Frame(e) => e.id,
            Self::Painting(e) => e.id,
            Self::Boat(e) => e.id,
            Self::Cart(e) => e.id,
            Self::Other(e) => e.id,
            Self::Crystal(e) => e.id,
            Self::Cushion(e) => e.id,
            Self::Dragon(e) => e.id,
            Self::Tnt(e) => e.id,
            Self::Bolt(e) => e.id,
        }
    }
}

/// What the test hook sees: a join it may refuse, or a leave.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum LifeEvent {
    Join(Kind, u32, Origin),
    Leave(Kind, u32, Removal),
}

impl Game<'_> {
    /// Adds `e` to the level unless the join is refused; its id, or None. `saved` is the compound a chunk load read it from.
    pub(in crate::game) fn level_add(
        &mut self,
        e: NewEntity,
        origin: Origin,
        saved: Option<&Tag>,
    ) -> Option<u32> {
        let id = e.id();
        if !self.java_hook_entity_joining(&e, origin, saved) {
            self.forget_entity(id);
            return None;
        }
        match e {
            NewEntity::Mob(e) => self.mobs.push(e),
            NewEntity::Item(e) => self.dropped.push(e),
            NewEntity::Orb(e) => self.orbs.push(e),
            NewEntity::Projectile(e) => self.creatures.projectiles.push(e),
            NewEntity::Stand(e) => self.armor_stands.push(e),
            NewEntity::Frame(e) => self.item_frames.push(e),
            NewEntity::Painting(e) => self.paintings.push(e),
            NewEntity::Boat(e) => self.boats.push(e),
            NewEntity::Cart(e) => self.carts.push(e),
            NewEntity::Other(e) => self.others.push(e),
            NewEntity::Crystal(e) => self.crystals.push(e),
            NewEntity::Cushion(e) => self.cushions.push(e),
            NewEntity::Dragon(e) => self.dragon = Some(*e),
            NewEntity::Tnt(e) => self.tnts.push(e),
            NewEntity::Bolt(e) => self.bolts.push(e),
        }
        self.note_touched(Some(id));
        Some(id)
    }

    /// Runs `f` with the mob at `index` out of the vector, so a world built inside `f` doesn't see it; it returns to its index and never leaves the level.
    pub(in crate::game) fn with_mob_outside<R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&mut Self, &mut Mob) -> R,
    ) -> R {
        let mut mob = self.mobs.remove(index);
        let result = f(self, &mut mob);
        self.mobs.insert(index, mob);
        result
    }

    /// Like `with_mob_outside`, for the cart at `index`.
    pub(in crate::game) fn with_cart_outside<R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&mut Self, &mut minecarts::Minecart) -> R,
    ) -> R {
        let mut cart = self.carts.remove(index);
        let result = f(self, &mut cart);
        self.carts.insert(index, cart);
        result
    }

    /// `Entity.remove(reason)` of the entity `id`; false when it isn't in the level.
    pub(in crate::game) fn level_remove(&mut self, kind: Kind, id: u32, why: Removal) -> bool {
        self.riding_removed(Ent::Id(id), why);
        let lent = self.lent();
        macro_rules! take {
            ($vec:expr, $lent:expr) => {{
                let Some(i) = $vec.iter().position(|e| e.id == id) else {
                    return false;
                };
                if lent.has($lent) {
                    if $vec[i].removed {
                        return false;
                    }
                    $vec[i].removed = true;
                } else {
                    $vec.remove(i);
                }
            }};
            ($vec:expr) => {{
                let Some(i) = $vec.iter().position(|e| e.id == id) else {
                    return false;
                };
                $vec.remove(i);
            }};
        }
        match kind {
            Kind::Mob => take!(self.mobs, crossing::LentKinds::MOBS),
            Kind::Item => take!(self.dropped, crossing::LentKinds::ITEMS),
            Kind::Orb => take!(self.orbs, crossing::LentKinds::ORBS),
            Kind::Projectile => take!(self.creatures.projectiles, crossing::LentKinds::PROJECTILES),
            Kind::Stand => take!(self.armor_stands, crossing::LentKinds::STANDS),
            Kind::Frame => take!(self.item_frames),
            Kind::Painting => take!(self.paintings),
            Kind::Boat => take!(self.boats),
            Kind::Cart => take!(self.carts),
            Kind::Other => take!(self.others),
            Kind::Crystal => take!(self.crystals),
            Kind::Cushion => take!(self.cushions),
            Kind::Tnt => take!(self.tnts),
            Kind::Bolt => take!(self.bolts, crossing::LentKinds::BOLTS),
            Kind::Dragon => {
                if self.dragon.as_ref().is_none_or(|d| d.id != id) {
                    return false;
                }
                self.dragon = None;
            }
        }
        self.note_touched(Some(id));
        self.java_hook_entity_left(kind, id, why);
        true
    }

    /// Drops what a crossing marked removed in lists that stayed in `Game` (mobs go with their tick).
    pub(in crate::game) fn drop_marked(&mut self) {
        self.dropped.retain(|e| !e.removed);
        self.orbs.retain(|e| !e.removed);
        self.creatures.projectiles.retain(|e| !e.removed);
        self.armor_stands.retain(|e| !e.removed);
    }
    /// `level_remove` for each of `ids`, in the order given.
    pub(in crate::game) fn level_remove_ids(&mut self, kind: Kind, ids: &[u32], why: Removal) {
        for &id in ids {
            self.level_remove(kind, id, why);
        }
    }

    /// Every entity of `kind` leaves in vector order: the vector is emptied in one pass, then each hook runs.
    pub(in crate::game) fn level_remove_all(&mut self, kind: Kind, why: Removal) {
        let ids = self.level_ids(kind);
        match kind {
            Kind::Mob => self.mobs.clear(),
            Kind::Item => self.dropped.clear(),
            Kind::Orb => self.orbs.clear(),
            Kind::Projectile => self.creatures.projectiles.clear(),
            Kind::Stand => self.armor_stands.clear(),
            Kind::Frame => self.item_frames.clear(),
            Kind::Painting => self.paintings.clear(),
            Kind::Boat => self.boats.clear(),
            Kind::Cart => self.carts.clear(),
            Kind::Other => self.others.clear(),
            Kind::Crystal => self.crystals.clear(),
            Kind::Cushion => self.cushions.clear(),
            Kind::Dragon => self.dragon = None,
            Kind::Tnt => self.tnts.clear(),
            Kind::Bolt => self.bolts.clear(),
        }
        for id in ids {
            self.riding_forget(Ent::Id(id));
            self.note_touched(Some(id));
            self.java_hook_entity_left(kind, id, why);
        }
    }

    /// The ids of every entity of `kind`, in vector order.
    pub(in crate::game) fn level_ids(&self, kind: Kind) -> Vec<u32> {
        match kind {
            Kind::Mob => self.mobs.iter().map(|e| e.id).collect(),
            Kind::Item => self.dropped.iter().map(|e| e.id).collect(),
            Kind::Orb => self.orbs.iter().map(|e| e.id).collect(),
            Kind::Projectile => self.creatures.projectiles.iter().map(|e| e.id).collect(),
            Kind::Stand => self.armor_stands.iter().map(|e| e.id).collect(),
            Kind::Frame => self.item_frames.iter().map(|e| e.id).collect(),
            Kind::Painting => self.paintings.iter().map(|e| e.id).collect(),
            Kind::Boat => self.boats.iter().map(|e| e.id).collect(),
            Kind::Cart => self.carts.iter().map(|e| e.id).collect(),
            Kind::Other => self.others.iter().map(|e| e.id).collect(),
            Kind::Crystal => self.crystals.iter().map(|e| e.id).collect(),
            Kind::Cushion => self.cushions.iter().map(|e| e.id).collect(),
            Kind::Dragon => self.dragon.iter().map(|e| e.id).collect(),
            Kind::Tnt => self.tnts.iter().map(|e| e.id).collect(),
            Kind::Bolt => self.bolts.iter().map(|e| e.id).collect(),
        }
    }

    /// A removal a loop decided on its taken vector (the entity is already marked `removed`): the leave runs with `lent` back in `Game`; only the mob loop needs the id as touched.
    pub(in crate::game) fn left_in_loop(
        &mut self,
        lent: &mut [crossing::Lent<'_>],
        kind: Kind,
        id: u32,
        why: Removal,
    ) {
        if kind == Kind::Mob {
            self.note_touched(Some(id));
        }
        self.lend(lent, |g| {
            g.riding_removed(Ent::Id(id), why);
            g.java_hook_entity_left(kind, id, why);
        });
    }
}
