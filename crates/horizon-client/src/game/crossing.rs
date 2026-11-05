//! Native entity loops lend their vectors back to `Game` so Java can run between two entities, in vanilla's order.
#![allow(dead_code)] // The event phases after SPEC-1a put work at the crossing points.
use super::*;
use horizon_client::join_seq::JoinList;

/// What a crossing changed through the native entity API, for the loop to refresh.
#[derive(Default)]
pub(in crate::game) struct Touched {
    pub ids: Vec<u32>,
    /// Players, teams, rules or blocks changed: the loop's shared state must be rebuilt.
    pub shared: bool,
}

impl Touched {
    pub(in crate::game) fn is_empty(&self) -> bool {
        self.ids.is_empty() && !self.shared
    }
    pub(in crate::game) fn has(&self, id: u32) -> bool {
        self.ids.contains(&id)
    }
}

/// The mobs a loop skips and drops: marked removed during a crossing, not removed before it.
pub(in crate::game) struct Marks {
    prior: Vec<u32>,
    marked: Vec<u32>,
}

impl Marks {
    /// The loop's start: the mobs already `removed` are not marks.
    pub(in crate::game) fn new(mobs: &[Mob]) -> Self {
        Self {
            prior: mobs.iter().filter(|m| m.removed).map(|m| m.id).collect(),
            marked: Vec::new(),
        }
    }
    /// A mob the loop itself left removed (and kept) is not a mark either.
    pub(in crate::game) fn prior(&mut self, id: u32) {
        self.prior.push(id);
    }
    pub(in crate::game) fn is_marked(&self, id: u32) -> bool {
        !self.marked.is_empty() && self.marked.contains(&id)
    }
    /// Marks the touched mobs that a crossing removed.
    pub(in crate::game) fn mark_touched(&mut self, t: &Touched, mobs: &[Mob]) {
        for &id in &t.ids {
            if !self.is_marked(id)
                && !self.prior.contains(&id)
                && mobs.iter().any(|m| m.id == id && m.removed)
            {
                self.marked.push(id);
            }
        }
    }
}

/// One vector a loop took out of `Game`.
pub(in crate::game) enum Lent<'a> {
    Mobs(&'a mut JoinList<Mob>),
    Items(&'a mut JoinList<ItemEntity>),
    Orbs(&'a mut JoinList<ExperienceOrb>),
    Projectiles(&'a mut JoinList<horizon_client::projectiles::Projectile>),
    Stands(&'a mut JoinList<horizon_client::entities::ArmorStand>),
    /// Every list the entity walk took (`take_entities`).
    All(&'a mut super::entity::TakenEntities),
}

/// The kinds of vector lent right now, one bit each.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(in crate::game) struct LentKinds(u8);

impl LentKinds {
    pub(in crate::game) const MOBS: Self = Self(1);
    pub(in crate::game) const ITEMS: Self = Self(2);
    pub(in crate::game) const ORBS: Self = Self(4);
    pub(in crate::game) const PROJECTILES: Self = Self(8);
    pub(in crate::game) const STANDS: Self = Self(16);
    /// The kinds whose removal a crossing can mark (they carry `removed`).
    pub(in crate::game) const MARKED: Self = Self(31);
    pub(in crate::game) fn has(self, kind: Self) -> bool {
        self.0 & kind.0 != 0
    }
}

impl Lent<'_> {
    fn kind(&self) -> LentKinds {
        match self {
            Self::Mobs(_) => LentKinds::MOBS,
            Self::Items(_) => LentKinds::ITEMS,
            Self::Orbs(_) => LentKinds::ORBS,
            Self::Projectiles(_) => LentKinds::PROJECTILES,
            Self::Stands(_) => LentKinds::STANDS,
            Self::All(_) => LentKinds::MARKED,
        }
    }
}

/// Where a loop crosses: the loop and the side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::game) enum CrossPoint {
    MobBefore(u32),
    MobAfter(u32),
    ItemAfter(u32),
    OrbAfter(u32),
    ProjectileAfter(u32),
    ExplosionMob(u32),
    ExplosionItem(u32),
    WindMob(u32),
    WindStand(u32),
    WindProjectile(u32),
    VillagerWake(u32),
    Creaking(u32),
    TouchArrow(u32),
    GuestArrow(u32),
    RemoteProjectile(u32),
}

impl<'a> Game<'a> {
    /// Runs `f` with `lent` back in place; what `f` appended stays at the end, what it removed is marked (SPEC-1b).
    pub(in crate::game) fn lend<R>(
        &mut self,
        lent: &mut [Lent<'_>],
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let mut kinds = LentKinds::default();
        for l in lent.iter_mut() {
            let kind = l.kind();
            debug_assert!(
                !self.lent.has(kind) && !kinds.has(kind),
                "nested lend of {kind:?}"
            );
            kinds.0 |= kind.0;
            self.swap_lent(l);
        }
        self.lent.0 |= kinds.0;
        let result = f(self);
        self.lent.0 &= !kinds.0;
        for l in lent.iter_mut() {
            self.swap_lent(l);
        }
        result
    }
    fn swap_lent(&mut self, lent: &mut Lent<'_>) {
        match lent {
            Lent::Mobs(v) => std::mem::swap(*v, &mut self.mobs),
            Lent::Items(v) => std::mem::swap(*v, &mut self.dropped),
            Lent::Orbs(v) => std::mem::swap(*v, &mut self.orbs),
            Lent::Projectiles(v) => std::mem::swap(*v, &mut self.creatures.projectiles),
            Lent::Stands(v) => std::mem::swap(*v, &mut self.armor_stands),
            Lent::All(taken) => self.swap_entities(taken),
        }
    }
    /// Records that the native entity API changed `id` (or shared state) during a crossing.
    pub(in crate::game) fn note_touched(&mut self, id: Option<u32>) {
        match id {
            Some(id) => self.touched.ids.push(id),
            None => self.touched.shared = true,
        }
    }
    pub(in crate::game) fn take_touched(&mut self) -> Touched {
        std::mem::take(&mut self.touched)
    }
    /// Which kinds are lent right now: `level_remove` (SPEC-1b) marks instead of removing them.
    pub(in crate::game) fn lent(&self) -> LentKinds {
        self.lent
    }
    /// `cross_point` for a walk over lists left in `Game`: removals of `kinds` are marked, not made.
    pub(in crate::game) fn cross_point_in_place(
        &mut self,
        point: CrossPoint,
        kinds: LentKinds,
    ) -> Touched {
        let before = self.lent;
        self.lent.0 |= kinds.0;
        let t = self.cross_point(point, &mut []);
        self.lent = before;
        t
    }
    /// Between two entities of a loop: where later phases cross into Java. Does nothing outside tests in this phase.
    pub(in crate::game) fn cross_point(
        &mut self,
        point: CrossPoint,
        lent: &mut [Lent<'_>],
    ) -> Touched {
        #[cfg(test)]
        if let Some(mut hook) = self.test_cross.take() {
            self.lend(lent, |g| hook(g, point));
            self.test_cross = Some(hook);
        }
        #[cfg(not(test))]
        let _ = (point, lent);
        self.take_touched()
    }
}
