//! `EntityTickList` per level: the join-order walk over every kind, with ticking kept as section visibility keeps it.

use super::Game;
use horizon_client::fast_hash::FxHashMap;
use horizon_client::join_seq::{JoinSeq, Joined, residency::Kind};
use horizon_core::geometry::Vec3;

/// One entity of the level, by kind and join number (its stable key while it stays in the level).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::game) struct Ent {
    pub kind: Kind,
    pub join: JoinSeq,
    pub id: u32,
}

/// `SectionPos.asLong`: x in the top 22 bits, z in the next 22, y in the low 20.
pub(in crate::game) fn as_long(x: i32, y: i32, z: i32) -> i64 {
    ((i64::from(x) & 0x3F_FFFF) << 42)
        | (i64::from(y) & 0xF_FFFF)
        | ((i64::from(z) & 0x3F_FFFF) << 20)
}

/// `SectionPos.x(long)` and `SectionPos.z(long)`: the chunk of a section key.
fn chunk_of(section: i64) -> (i32, i32) {
    ((section >> 42) as i32, ((section << 22) >> 42) as i32)
}

fn section_of(block: [i32; 3]) -> i64 {
    as_long(block[0] >> 4, block[1] >> 4, block[2] >> 4)
}

/// What the walk saw of one entity: where it is, or `None` for a player (`isAlwaysTicking`).
#[derive(Clone, Copy, Debug)]
pub(in crate::game) struct Row {
    pub ent: Ent,
    pub block: Option<[i32; 3]>,
}

#[derive(Clone, Copy, Debug)]
struct Member {
    ent: Ent,
    /// Where it sits in the tick order: its join, or a later number when it started ticking again.
    stamp: JoinSeq,
    section: i64,
    /// When it entered its section, on the join counter: the section's insertion order.
    entered: JoinSeq,
    ticking: bool,
    always: bool,
    seen: u64,
}

/// The level's tick list: who is ticking and in which order.
#[derive(Default)]
pub(in crate::game) struct TickList {
    members: FxHashMap<JoinSeq, Member>,
    keeps: u64,
    rows: Vec<Row>,
}

impl TickList {
    /// `onTickingStart`: to the end of the order.
    pub fn start(&mut self, e: Ent) {
        let m = self.members.entry(e.join).or_insert(Member {
            ent: e,
            stamp: e.join,
            section: 0,
            entered: e.join,
            ticking: false,
            always: false,
            seen: self.keeps,
        });
        debug_assert!(!m.ticking, "{e:?} already ticking");
        m.ticking = true;
        m.stamp = JoinSeq::next();
    }

    /// `onTickingEnd`.
    pub fn stop(&mut self, e: Ent) {
        if let Some(m) = self.members.get_mut(&e.join) {
            m.ticking = false;
        }
    }

    /// `entityTickList.contains`, as `tickPassenger` asks it.
    #[allow(dead_code, reason = "the one ordered entity loop (SPEC-2-13) reads it")]
    pub fn contains(&self, e: Ent) -> bool {
        self.members.get(&e.join).is_some_and(|m| m.ticking)
    }

    /// `forEach`'s snapshot: what joins during the pass is not in it, what leaves stays in it.
    #[allow(dead_code, reason = "the one ordered entity loop (SPEC-2-13) reads it")]
    pub fn pass(&self) -> Vec<Ent> {
        let mut order: Vec<_> = self
            .members
            .values()
            .filter(|m| m.ticking)
            .map(|m| (m.stamp, m.ent))
            .collect();
        order.sort_unstable_by_key(|&(stamp, _)| stamp);
        order.into_iter().map(|(_, e)| e).collect()
    }

    /// `EntitySection.add`: the entity is the newest of `section`.
    pub fn enter_section(&mut self, e: Ent, section: i64) {
        if let Some(m) = self.members.get_mut(&e.join) {
            m.section = section;
            m.entered = JoinSeq::next();
        }
    }

    /// `getExistingSectionsInChunk` in `asLong` order, each section's entities in entry order; players left out.
    #[allow(dead_code, reason = "the one ordered entity loop (SPEC-2-13) reads it")]
    pub fn in_chunk_ordered(&self, chunk: (i32, i32)) -> Vec<Ent> {
        let mut found: Vec<_> = self
            .members
            .values()
            .filter(|m| !m.always && chunk_of(m.section) == chunk)
            .map(|m| (m.section, m.entered, m.ent))
            .collect();
        found.sort_unstable_by_key(|&(section, entered, _)| (section, entered));
        found.into_iter().map(|(_, _, e)| e).collect()
    }

    /// Brings the list up to what the walk saw: adds, removes, section moves and chunk visibility changes.
    pub fn keep(&mut self, rows: &[Row], visible: impl Fn(i32, i32) -> bool) {
        self.keeps += 1;
        let keeps = self.keeps;
        let mut started = Vec::new();
        for row in rows {
            let section = row.block.map_or(0, section_of);
            let ticking = row.block.is_none_or(|b| visible(b[0], b[2]));
            match self.members.get_mut(&row.ent.join) {
                None => {
                    let stamp = if row.block.is_none() {
                        JoinSeq::next()
                    } else {
                        row.ent.join
                    };
                    self.members.insert(
                        row.ent.join,
                        Member {
                            ent: row.ent,
                            stamp,
                            section,
                            entered: row.ent.join,
                            ticking,
                            always: row.block.is_none(),
                            seen: keeps,
                        },
                    );
                }
                Some(m) => {
                    m.seen = keeps;
                    m.ent = row.ent;
                    if m.always {
                        continue;
                    }
                    let (moved, was_ticking) = (m.section != section, m.ticking);
                    if moved {
                        self.enter_section(row.ent, section);
                    }
                    if ticking && !was_ticking {
                        let entered = self.members[&row.ent.join].entered;
                        started.push((chunk_of(section), section, entered, row.ent));
                    } else if !ticking {
                        self.stop(row.ent);
                    }
                }
            }
        }
        self.members.retain(|_, m| m.seen == keeps);
        // Vanilla walks the changed chunks in identity-hash order; native goes by chunk coordinates.
        started.sort_unstable_by_key(|&(chunk, section, entered, _)| (chunk, section, entered));
        for (_, _, _, e) in started {
            self.start(e);
        }
    }
}

/// Where an entity of each kind is, for its section.
trait TickAt {
    fn tick_id(&self) -> u32;
    fn tick_block(&self) -> [i32; 3];
}

fn block_of(x: f64, y: f64, z: f64) -> [i32; 3] {
    [x.floor() as i32, y.floor() as i32, z.floor() as i32]
}

fn block_of_vec(v: Vec3) -> [i32; 3] {
    block_of(v.x, v.y, v.z)
}

macro_rules! tick_at_entity {
    ($($ty:ty),* $(,)?) => {$(
        impl TickAt for $ty {
            fn tick_id(&self) -> u32 {
                super::entity::Entity::id(self)
            }
            fn tick_block(&self) -> [i32; 3] {
                block_of_vec(super::entity::Entity::position(self))
            }
        }
    )*};
}

tick_at_entity!(
    horizon_client::mobs::Mob,
    horizon_client::entities::ItemEntity,
    horizon_client::entities::ExperienceOrb,
    horizon_client::entities::ArmorStand,
    super::boats::Boat,
    super::minecarts::Minecart,
    super::tnt::PrimedTnt,
    super::FallingBlock,
    super::fireworks::Rocket,
    super::trident::ThrownTrident,
    horizon_client::projectiles::Projectile,
);

macro_rules! tick_at {
    ($($ty:ty => |$e:ident| $block:expr),* $(,)?) => {$(
        impl TickAt for $ty {
            fn tick_id(&self) -> u32 {
                self.id
            }
            fn tick_block(&self) -> [i32; 3] {
                let $e = self;
                $block
            }
        }
    )*};
}

tick_at!(
    horizon_client::entities::ItemFrame => |e| e.pos,
    horizon_client::entities::Painting => |e| e.pos,
    horizon_client::entities::Cushion => |e| block_of_vec(e.position),
    super::commands::OtherEntity => |e| block_of_vec(e.position),
    super::fangs::Fangs => |e| block_of(e.pos[0], e.pos[1], e.pos[2]),
    super::dragon::Crystal => |e| block_of_vec(e.position),
    super::lightning::Bolt => |e| block_of(e.at[0], e.at[1], e.at[2]),
    super::leads::Knot => |e| e.fence,
    super::ominous_item_spawner::OminousItemSpawner => |e| block_of(e.position[0], e.position[1], e.position[2]),
    super::dragon::Breath => |e| block_of(e.at[0], e.at[1], e.at[2]),
    super::potions::ThrownPotion => |e| block_of_vec(e.position),
    super::end::EyeOfEnder => |e| block_of(e.position.x, e.position.y, e.position.z),
    super::dragon::Dragon => |e| block_of_vec(e.position),
);

fn push_rows<T: Joined + TickAt>(out: &mut Vec<Row>, kind: Kind, list: &[T]) {
    for e in list {
        let join = e.join_seq();
        if join.is_joined() {
            out.push(Row {
                ent: Ent {
                    kind,
                    join,
                    id: e.tick_id(),
                },
                block: Some(e.tick_block()),
            });
        }
    }
}

macro_rules! tick_rows {
    (($this:ident, $($projectiles:ident).+) $($kind:ident => $target:ident: $($field:ident).+,)*) => {
        /// The join-order walk's rows, every kind of the level's resident owners and its players.
        fn tick_rows(&$this, out: &mut Vec<Row>) {
            $(push_rows(out, Kind::$kind, &$this$(.$field)+);)*
            push_rows(out, Kind::Projectile, &$this$(.$projectiles)+);
            push_rows(out, Kind::Dragon, $this.dragon.as_slice());
        }
    };
}

impl Game<'_> {
    super::entity_residency::resident_owners!(
        tick_rows,
        self,
        creatures.projectiles,
        resident_entities.knots,
        potions.thrown,
        end.eyes
    );

    /// Keeps this level's tick list: called as chunk tickets change and as each entity pass begins.
    pub(super) fn keep_tick_list(&mut self) {
        if self.remote.is_some() {
            return;
        }
        let mut rows = std::mem::take(&mut self.tick_list.rows);
        rows.clear();
        self.tick_rows(&mut rows);
        // `ServerLevel.EntityCallbacks.onTickingStart`: a removed mob is no longer in the list.
        rows.retain(|row| {
            row.ent.kind != Kind::Mob
                || self
                    .mobs
                    .locate(row.ent.join)
                    .is_some_and(|i| !self.mobs[i].removed)
        });
        let player = |join: JoinSeq, id: u32| Row {
            ent: Ent {
                kind: Kind::Player,
                join,
                id,
            },
            block: None,
        };
        if let Some(p) = &self.player {
            let join = self.resident_entities.local_player.join();
            rows.push(player(join, p.entity_id as u32));
        }
        if let Some(h) = &self.lan {
            rows.extend(
                h.hub
                    .guests
                    .iter()
                    .filter(|g| h.hub.here(g))
                    .map(|g| player(g.residency.join(), g.entity_id as u32)),
            );
        }
        let visible = self
            .tickets
            .entity_visible_area(&self.world.chunks, self.remote.is_none());
        self.tick_list.keep(&rows, visible);
        self.tick_list.rows = rows;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(join: JoinSeq) -> Ent {
        Ent {
            kind: Kind::Mob,
            join,
            id: join.get() as u32,
        }
    }

    fn at(e: Ent, x: i32, y: i32, z: i32) -> Row {
        Row {
            ent: e,
            block: Some([x, y, z]),
        }
    }

    #[test]
    fn as_long_sorts_negative_sections_after_positive_ones() {
        let mut keys = [
            as_long(3, -1, -2),
            as_long(3, 5, -2),
            as_long(3, 0, -2),
            as_long(3, -4, -2),
        ];
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                as_long(3, 0, -2),
                as_long(3, 5, -2),
                as_long(3, -4, -2),
                as_long(3, -1, -2)
            ]
        );
        assert_eq!(chunk_of(as_long(-7, -3, 12)), (-7, 12));
        assert_eq!(chunk_of(as_long(5, 9, -30_000)), (5, -30_000));
    }

    #[test]
    fn the_pass_is_the_order_entities_started_ticking() {
        let (pig, item, stand) = (
            ent(JoinSeq::next()),
            ent(JoinSeq::next()),
            ent(JoinSeq::next()),
        );
        let mut list = TickList::default();
        list.keep(
            &[at(stand, 1, 64, 1), at(pig, 40, 64, 1), at(item, 2, 64, 2)],
            |_, _| true,
        );
        assert_eq!(list.pass(), [pig, item, stand]);
    }

    #[test]
    fn an_entity_added_during_a_pass_is_not_in_it() {
        let first = ent(JoinSeq::next());
        let mut list = TickList::default();
        list.keep(&[at(first, 0, 64, 0)], |_, _| true);
        let pass = list.pass();
        let later = ent(JoinSeq::next());
        list.keep(&[at(first, 0, 64, 0), at(later, 0, 64, 0)], |_, _| true);
        assert_eq!(pass, [first]);
        assert_eq!(list.pass(), [first, later]);
    }

    #[test]
    fn one_removed_during_a_pass_is_still_visited() {
        let (a, b) = (ent(JoinSeq::next()), ent(JoinSeq::next()));
        let mut list = TickList::default();
        list.keep(&[at(a, 0, 64, 0), at(b, 0, 64, 0)], |_, _| true);
        let pass = list.pass();
        list.keep(&[at(b, 0, 64, 0)], |_, _| true);
        assert_eq!(pass, [a, b]);
        assert!(!list.contains(a));
        assert_eq!(list.pass(), [b]);
    }

    #[test]
    fn a_chunk_becoming_ticking_appends_its_entities_by_section_then_entry() {
        let (low, high, under, outside) = (
            ent(JoinSeq::next()),
            ent(JoinSeq::next()),
            ent(JoinSeq::next()),
            ent(JoinSeq::next()),
        );
        let rows = [
            at(under, 3, -10, 3),
            at(high, 3, 90, 3),
            at(low, 3, 10, 3),
            at(outside, 40, 64, 40),
        ];
        let mut list = TickList::default();
        list.keep(&rows, |x, _| x >= 32);
        assert_eq!(list.pass(), [outside]);
        list.keep(&rows, |_, _| true);
        assert_eq!(list.pass(), [outside, low, high, under]);
        assert_eq!(list.in_chunk_ordered((0, 0)), [low, high, under]);
    }

    #[test]
    fn an_entity_walking_out_of_ticking_range_and_back_goes_to_the_end() {
        let (walker, other) = (ent(JoinSeq::next()), ent(JoinSeq::next()));
        let mut list = TickList::default();
        let inside = |x: i32, _: i32| x < 16;
        list.keep(&[at(walker, 0, 64, 0), at(other, 1, 64, 0)], inside);
        assert_eq!(list.pass(), [walker, other]);
        list.keep(&[at(walker, 20, 64, 0), at(other, 1, 64, 0)], inside);
        assert_eq!(list.pass(), [other]);
        list.keep(&[at(walker, 2, 64, 0), at(other, 1, 64, 0)], inside);
        assert_eq!(list.pass(), [other, walker]);
    }

    #[test]
    fn players_are_always_ticking() {
        let mob = ent(JoinSeq::next());
        let player = Ent {
            kind: Kind::Player,
            join: JoinSeq::next(),
            id: 1,
        };
        let rows = [
            at(mob, 0, 64, 0),
            Row {
                ent: player,
                block: None,
            },
        ];
        let mut list = TickList::default();
        list.keep(&rows, |_, _| false);
        assert_eq!(list.pass(), [player]);
        assert!(list.contains(player));
        assert_eq!(list.in_chunk_ordered((0, 0)), [mob]);
    }

    #[test]
    #[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
    fn summoned_kinds_tick_in_the_order_they_were_summoned() {
        crate::game::native_entity_refs::tests::with_game(|game| {
            game.build_registries();
            game.set_allow_commands(true);
            for command in [
                "summon pig 0 70 0",
                "summon item 1 70 0 {Item:{id:\"minecraft:stone\"}}",
                "summon armor_stand 2 70 0",
            ] {
                assert!(game.run_command(command));
            }
            game.keep_tick_list();
            let kinds: Vec<_> = game.tick_list.pass().into_iter().map(|e| e.kind).collect();
            assert_eq!(kinds, [Kind::Mob, Kind::Item, Kind::Stand]);
        });
    }
}
