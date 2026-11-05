//! `Entity.vehicle` and `Entity.passengers` of one level: who rides whom,
//! for every kind of entity, players included (`Entity.java` 2441 to 2600).
use crate::fast_hash::{FxHashMap, FxHashSet};

/// `Entity.removePassenger`: how long a rider that got off cannot board.
pub const BOARDING_COOLDOWN: i32 = 60;

/// An entity of a level: any non-player by its id (ids are game-wide), or a player.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Ent {
    Id(u32),
    Host,
    Guest(u32),
}

impl Ent {
    pub fn is_player(self) -> bool {
        !matches!(self, Ent::Id(_))
    }
    pub fn id(self) -> Option<u32> {
        match self {
            Ent::Id(i) => Some(i),
            _ => None,
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct Riding {
    vehicle: FxHashMap<Ent, Ent>,
    passengers: FxHashMap<Ent, Vec<Ent>>,
    boarding_cooldown: FxHashMap<Ent, i32>,
    changed: FxHashSet<Ent>,
}

impl Riding {
    /// `Entity.getVehicle`.
    pub fn vehicle_of(&self, e: Ent) -> Option<Ent> {
        self.vehicle.get(&e).copied()
    }
    /// `Entity.getPassengers`, in vanilla's order.
    pub fn passengers_of(&self, e: Ent) -> &[Ent] {
        self.passengers.get(&e).map_or(&[], Vec::as_slice)
    }
    pub fn first_passenger(&self, e: Ent) -> Option<Ent> {
        self.passengers_of(e).first().copied()
    }
    pub fn is_passenger(&self, e: Ent) -> bool {
        self.vehicle.contains_key(&e)
    }
    pub fn is_vehicle(&self, e: Ent) -> bool {
        !self.passengers_of(e).is_empty()
    }
    pub fn has_passenger(&self, v: Ent, p: Ent) -> bool {
        self.passengers_of(v).contains(&p)
    }
    /// `Entity.getRootVehicle`.
    pub fn root_vehicle(&self, e: Ent) -> Ent {
        let mut at = e;
        while let Some(v) = self.vehicle_of(at) {
            at = v;
        }
        at
    }
    /// `Entity.getIndirectPassengers`: depth first.
    pub fn indirect_passengers(&self, e: Ent) -> Vec<Ent> {
        let mut out = Vec::new();
        self.collect_indirect(e, &mut out);
        out
    }
    fn collect_indirect(&self, e: Ent, out: &mut Vec<Ent>) {
        for &p in self.passengers_of(e) {
            out.push(p);
            self.collect_indirect(p, out);
        }
    }
    /// `Entity.isPassengerOfSameVehicle`.
    pub fn passenger_of_same_vehicle(&self, a: Ent, b: Ent) -> bool {
        self.root_vehicle(a) == self.root_vehicle(b) && self.is_passenger(a)
    }
    /// `Entity.hasExactlyOnePlayerPassenger`.
    pub fn exactly_one_player_passenger(&self, e: Ent) -> bool {
        let p = self.passengers_of(e);
        p.len() == 1 && p[0].is_player()
    }
    /// `Entity.boardingCooldown`.
    pub fn boarding_cooldown(&self, e: Ent) -> i32 {
        self.boarding_cooldown.get(&e).copied().unwrap_or(0)
    }

    /// Whether `v` or anything it rides is `r`: `startRiding`'s loop test.
    pub fn rides_through(&self, v: Ent, r: Ent) -> bool {
        let mut at = v;
        while let Some(up) = self.vehicle_of(at) {
            if up == r {
                return true;
            }
            at = up;
        }
        false
    }

    /// `startRiding`'s link: `vehicle = v` then `v.addPassenger(r)`, whose
    /// server rule seats a player first when the first passenger is none.
    pub fn link(&mut self, r: Ent, v: Ent) {
        self.vehicle.insert(r, v);
        let first_is_player = self.first_passenger(v).is_some_and(Ent::is_player);
        let list = self.passengers.entry(v).or_default();
        if r.is_player() && !first_is_player && !list.is_empty() {
            list.insert(0, r);
        } else {
            list.push(r);
        }
        self.changed.insert(v);
    }
    /// `removeVehicle`'s unlink: the old vehicle, with the boarding cooldown set.
    pub fn unlink(&mut self, r: Ent) -> Option<Ent> {
        let v = self.vehicle.remove(&r)?;
        if let Some(list) = self.passengers.get_mut(&v) {
            list.retain(|p| *p != r);
            if list.is_empty() {
                self.passengers.remove(&v);
            }
        }
        self.boarding_cooldown.insert(r, BOARDING_COOLDOWN);
        self.changed.insert(v);
        Some(v)
    }
    /// `Entity.baseTick`: every cooldown counts down once.
    pub fn tick_cooldowns(&mut self) {
        self.boarding_cooldown.retain(|_, c| {
            *c -= 1;
            *c > 0
        });
    }
    /// Forget `e` entirely, as the table of a level that no longer holds it.
    pub fn forget(&mut self, e: Ent) {
        if let Some(v) = self.vehicle.remove(&e) {
            if let Some(list) = self.passengers.get_mut(&v) {
                list.retain(|p| *p != e);
                if list.is_empty() {
                    self.passengers.remove(&v);
                }
            }
            self.changed.insert(v);
        }
        if let Some(list) = self.passengers.remove(&e) {
            for p in list {
                self.vehicle.remove(&p);
            }
            self.changed.insert(e);
        }
        self.boarding_cooldown.remove(&e);
    }
    /// Vehicles whose passenger list changed since the last call.
    pub fn take_changed(&mut self) -> Vec<Ent> {
        let mut v: Vec<Ent> = self.changed.drain().collect();
        v.sort_unstable();
        v
    }
    /// Every rider with its vehicle, roots first (`tickPassenger` order).
    pub fn riders_in_order(&self) -> Vec<(Ent, Ent)> {
        let mut roots: Vec<Ent> = self
            .passengers
            .keys()
            .copied()
            .filter(|v| !self.is_passenger(*v))
            .collect();
        roots.sort_unstable();
        let mut out = Vec::new();
        for root in roots {
            self.collect_pairs(root, &mut out);
        }
        out
    }
    fn collect_pairs(&self, v: Ent, out: &mut Vec<(Ent, Ent)>) {
        for &p in self.passengers_of(v) {
            out.push((p, v));
            self.collect_pairs(p, out);
        }
    }
    /// Every entity in the table, as rider or vehicle.
    pub fn members(&self) -> Vec<Ent> {
        let mut v: Vec<Ent> = self
            .vehicle
            .keys()
            .chain(self.passengers.keys())
            .copied()
            .collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_passenger_puts_a_player_first() {
        let mut t = Riding::default();
        let v = Ent::Id(1);
        t.link(Ent::Id(2), v);
        t.link(Ent::Host, v);
        assert_eq!(t.passengers_of(v), &[Ent::Host, Ent::Id(2)]);
        t.link(Ent::Guest(7), v);
        assert_eq!(t.passengers_of(v), &[Ent::Host, Ent::Id(2), Ent::Guest(7)]);
    }
    #[test]
    fn stop_riding_sets_the_boarding_cooldown_to_60() {
        let mut t = Riding::default();
        t.link(Ent::Id(2), Ent::Id(1));
        assert_eq!(t.unlink(Ent::Id(2)), Some(Ent::Id(1)));
        assert_eq!(t.boarding_cooldown(Ent::Id(2)), 60);
        assert!(!t.is_vehicle(Ent::Id(1)));
        for _ in 0..60 {
            t.tick_cooldowns();
        }
        assert_eq!(t.boarding_cooldown(Ent::Id(2)), 0);
    }
    #[test]
    fn trees_walk_depth_first_and_roots_first() {
        let mut t = Riding::default();
        t.link(Ent::Id(2), Ent::Id(1));
        t.link(Ent::Id(3), Ent::Id(2));
        t.link(Ent::Id(4), Ent::Id(1));
        assert_eq!(t.root_vehicle(Ent::Id(3)), Ent::Id(1));
        assert_eq!(
            t.indirect_passengers(Ent::Id(1)),
            [Ent::Id(2), Ent::Id(3), Ent::Id(4)]
        );
        assert_eq!(
            t.riders_in_order(),
            [
                (Ent::Id(2), Ent::Id(1)),
                (Ent::Id(3), Ent::Id(2)),
                (Ent::Id(4), Ent::Id(1))
            ]
        );
        assert!(t.rides_through(Ent::Id(3), Ent::Id(1)));
        assert!(!t.rides_through(Ent::Id(1), Ent::Id(3)));
        assert!(t.passenger_of_same_vehicle(Ent::Id(3), Ent::Id(4)));
    }
    #[test]
    fn forgetting_a_vehicle_frees_its_riders() {
        let mut t = Riding::default();
        t.link(Ent::Id(2), Ent::Id(1));
        t.forget(Ent::Id(1));
        assert!(!t.is_passenger(Ent::Id(2)));
        assert!(t.members().is_empty());
    }
}
