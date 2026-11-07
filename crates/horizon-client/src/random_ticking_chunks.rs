//! The chunks `ServerChunkCache.tickChunks` random ticks, kept between ticks.
use crate::{ticket_levels::Levels, world::World};

type Key = (i32, i32);

/// A chunk whose ticking future can complete (`ChunkMap.prepareTickingChunk`):
/// lit, with its neighbours loaded, as `postprocess_chunks` waits for.
pub fn ticking_chunk(world: &World, x: i32, z: i32) -> bool {
    let Some(center) = world.chunks.get(&(x, z)) else {
        return false;
    };
    center.lit && world.chunks.surrounded((x, z))
}

/// What the list was built from: the resident keys, which of them are lit,
/// and the tickets.
type Stamp = (u64, u64, u64);

/// The random-ticking chunks in chunk position order, rebuilt only when one of
/// the three things they depend on changes.
#[derive(Default)]
pub struct RandomTickingChunks {
    stamp: Option<Stamp>,
    list: Vec<Key>,
}

impl RandomTickingChunks {
    /// The list for `world` under the tickets named by `tickets_revision`.
    pub fn sync(&mut self, world: &World, levels: &Levels, tickets_revision: u64) -> &[Key] {
        let stamp = (
            world.chunks.membership_revision(),
            world.chunks.lit_revision(),
            tickets_revision,
        );
        if self.stamp != Some(stamp) {
            self.list = walk(world, levels);
            self.stamp = Some(stamp);
        }
        debug_assert_eq!(self.list, walk(world, levels), "stale random-ticking list");
        &self.list
    }
}

/// Every chunk of the map that random ticks, by position (empty chunks too:
/// `tick_chunk_random` still draws for precipitation in those).
fn walk(world: &World, levels: &Levels) -> Vec<Key> {
    let bounds = levels.random_tick_bounds();
    let mut list: Vec<Key> = (world.chunks.keys().copied())
        .filter(|&(x, z)| {
            if let Some((min_x, min_z, max_x, max_z)) = bounds
                && (x < min_x || x > max_x || z < min_z || z > max_z)
            {
                return false;
            }
            levels.random_ticking(x, z) && ticking_chunk(world, x, z)
        })
        .collect();
    list.sort_unstable();
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        chunk::Chunk,
        ticket_levels::{ChunkTickets, Sources},
    };
    use horizon_world::registry::BlockStateId;
    use std::sync::Arc;

    /// The walk `tick_blocks` made every tick before the list was kept, sorted by position.
    fn fresh_walk(world: &World, tickets: &ChunkTickets) -> Vec<Key> {
        let levels = tickets.levels();
        let bounds = levels.random_tick_bounds();
        let mut out = Vec::new();
        for &(x, z) in world.chunks.keys() {
            if let Some((min_x, min_z, max_x, max_z)) = bounds
                && (x < min_x || x > max_x || z < min_z || z > max_z)
            {
                continue;
            }
            let Some(center) = world.chunks.get(&(x, z)) else {
                continue;
            };
            let around = (-1..=1).all(|dz| {
                (-1..=1)
                    .all(|dx| (dx == 0 && dz == 0) || world.chunks.contains_key(&(x + dx, z + dz)))
            });
            if levels.random_ticking(x, z) && center.lit && around {
                out.push((x, z));
            }
        }
        out.sort_unstable();
        out
    }

    fn tickets(players: &[(i32, i32)]) -> ChunkTickets {
        let mut t = ChunkTickets::default();
        t.update(Some(Sources {
            players: players.to_vec(),
            view_distance: 6,
            simulation_distance: 4,
            ..Default::default()
        }));
        t
    }

    fn chunk(lit: bool) -> Arc<Chunk> {
        let mut c = Chunk::empty(0, 0);
        c.lit = lit;
        Arc::new(c)
    }

    fn check(cache: &mut RandomTickingChunks, world: &World, tickets: &ChunkTickets, step: &str) {
        let got = cache
            .sync(world, &tickets.levels(), tickets.revision())
            .to_vec();
        assert_eq!(got, fresh_walk(world, tickets), "{step}");
    }

    #[test]
    fn the_list_follows_membership_lit_flips_and_tickets_like_a_fresh_walk() {
        let mut world = World::new(BlockStateId(0), 0);
        let mut t = tickets(&[(0, 0)]);
        let mut cache = RandomTickingChunks::default();
        check(&mut cache, &world, &t, "empty world");
        // A 15x15 block of unlit chunks, then lit from the inside out.
        for x in -7..=7 {
            for z in -7..=7 {
                world.chunks.insert((x, z), chunk(false));
            }
        }
        check(&mut cache, &world, &t, "all unlit");
        assert!(cache.list.is_empty());
        let mut order: Vec<Key> = world.chunks.keys().copied().collect();
        order.sort_by_key(|&(x, z)| (x.abs().max(z.abs()), x, z));
        for (i, key) in order.iter().enumerate() {
            Arc::make_mut(world.chunks.get_mut(key).unwrap()).lit = true;
            world.chunks.note_lit_changed();
            if i % 17 == 0 {
                check(&mut cache, &world, &t, "lit one by one");
            }
        }
        check(&mut cache, &world, &t, "all lit");
        assert!(cache.list.len() > 20);
        // A neighbour leaving takes the ring around it out; coming back restores it.
        let gone = world.chunks.remove(&(2, 1)).unwrap();
        check(&mut cache, &world, &t, "a neighbour removed");
        world.chunks.insert((2, 1), gone);
        check(&mut cache, &world, &t, "the neighbour back");
        // A chunk replaced by an unlit one drops out without any key change.
        let before = world.chunks.membership_revision();
        world.chunks.insert((0, 1), chunk(false));
        assert_eq!(before, world.chunks.membership_revision());
        check(
            &mut cache,
            &world,
            &t,
            "a lit chunk replaced by an unlit one",
        );
        world.chunks.insert((0, 1), chunk(true));
        check(&mut cache, &world, &t, "replaced by a lit one again");
        // Churn that moves the map's layout around: the order stays by position.
        let mut seed = 12345u32;
        for round in 0..60 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let key = ((seed >> 8) as i32 % 21 - 10, (seed >> 16) as i32 % 21 - 10);
            if round % 3 == 0 {
                world.chunks.remove(&key);
            } else {
                world.chunks.insert(key, chunk(round % 5 != 0));
            }
            check(&mut cache, &world, &t, "churn");
        }
        // The tickets move, grow, vanish and come back.
        t.update(Some(Sources {
            players: vec![(3, -2), (-6, 5)],
            forced: vec![(9, 9)],
            view_distance: 6,
            simulation_distance: 4,
            ..Default::default()
        }));
        check(&mut cache, &world, &t, "players moved");
        t.update(None);
        check(&mut cache, &world, &t, "no tickets: everywhere");
        t.update(Some(Sources {
            view_distance: 6,
            simulation_distance: 4,
            ..Default::default()
        }));
        check(&mut cache, &world, &t, "tickets with no source");
        assert!(cache.list.is_empty());
        t.update(Some(Sources {
            players: vec![(0, 0)],
            view_distance: 6,
            simulation_distance: 2,
            ..Default::default()
        }));
        check(&mut cache, &world, &t, "a shorter simulation distance");
        // A whole map swapped in (another dimension) and cleared.
        let mut other = World::new(BlockStateId(0), 0);
        for x in -3..=3 {
            for z in -3..=3 {
                other.chunks.insert((x, z), chunk(true));
            }
        }
        check(&mut cache, &other, &t, "another world");
        check(&mut cache, &world, &t, "back to the first");
        world.chunks.clear();
        check(&mut cache, &world, &t, "cleared");
    }
}
