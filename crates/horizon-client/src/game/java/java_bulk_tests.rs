// game-module-cfg: feature java-mods
#![cfg(test)]
//! #262 differential fixtures: a ticker list ticked by a reference loop, the scalar path and real bulk-run Rust code against a model of Java's run loop gives one trace.

use super::java_be_runs::{Run, java_be_run_cap, java_be_run_len, java_be_run_processed};

/// One step of a ticker's tick.
#[derive(Clone, Copy, Debug)]
enum Op {
    /// A read callback of a cell (a block state, an inventory): it doesn't end a run.
    Read(usize),
    /// A write callback (`setBlock`, an inventory write) of a value into a cell.
    Write(usize, i64),
    /// A write that takes a ticker slot out of the list (its block broken, its chunk unloaded).
    Remove(usize),
    /// A write whose native handler asks Java again, which reads the cell back (a nested `setBlock` callback).
    Nested(usize, i64),
    /// A draw of the level's random.
    Rng,
    /// A sound, which Rust plays from the tick's reply.
    Sound(u8),
    /// The tick throws here.
    Throw,
}

#[derive(Clone, Copy)]
struct Ticker {
    java: bool,
    ops: &'static [Op],
}

const fn java(ops: &'static [Op]) -> Ticker {
    Ticker { java: true, ops }
}

const fn native(ops: &'static [Op]) -> Ticker {
    Ticker { java: false, ops }
}

/// What the world saw, in order; `Finish` is the native rest of a Java entry's tick.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Ev {
    Tick(usize),
    Read(usize, usize, i64),
    Write(usize, usize, i64),
    Removed(usize, usize),
    NestedRead(usize, usize, i64),
    Rng(usize, u64),
    Threw(usize),
    Sound(usize, u8),
    Finish(usize),
}

struct World {
    cells: [i64; 4],
    live: Vec<bool>,
    seed: u64,
    trace: Vec<Ev>,
    /// Rust-to-Java requests and Java-to-Rust callbacks.
    requests: usize,
    callbacks: usize,
}

impl World {
    fn new(n: usize) -> World {
        World {
            cells: [10, 20, 30, 40],
            live: vec![true; n],
            seed: 0x5eed,
            trace: Vec::new(),
            requests: 0,
            callbacks: 0,
        }
    }

    fn draw(&mut self) -> u64 {
        self.seed = self
            .seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.seed >> 33
    }
}

/// Slot `i`'s tick: `callback` hears each Java-to-Rust call first, told whether it ends a run; the sounds made, or None when it threw.
fn tick(
    world: &mut World,
    list: &[Ticker],
    i: usize,
    callback: &mut dyn FnMut(&mut World, bool),
) -> Option<Vec<u8>> {
    let ticker = list[i];
    let mut call = |world: &mut World, ends: bool| {
        if ticker.java {
            world.callbacks += 1;
            callback(world, ends);
        }
    };
    world.trace.push(Ev::Tick(i));
    let mut sounds = Vec::new();
    for &op in ticker.ops {
        match op {
            Op::Read(c) => {
                call(world, false);
                world.trace.push(Ev::Read(i, c, world.cells[c]));
            }
            Op::Write(c, v) => {
                call(world, true);
                world.cells[c] = v;
                world.trace.push(Ev::Write(i, c, v));
            }
            Op::Remove(j) => {
                call(world, true);
                world.live[j] = false;
                world.trace.push(Ev::Removed(i, j));
            }
            Op::Nested(c, v) => {
                call(world, true);
                world.cells[c] = v;
                world.trace.push(Ev::Write(i, c, v));
                // The handler's own request into Java: a nested call ticks scalar and reads what was just written.
                world.requests += 1;
                world.trace.push(Ev::NestedRead(i, c, world.cells[c]));
            }
            Op::Rng => {
                let drawn = world.draw();
                world.trace.push(Ev::Rng(i, drawn));
            }
            Op::Sound(s) => sounds.push(s),
            Op::Throw => {
                world.trace.push(Ev::Threw(i));
                return None;
            }
        }
    }
    Some(sounds)
}

/// The scalar path's whole tick of one slot: its tick, then its sounds, then a Java entry's native rest.
fn tick_alone(world: &mut World, list: &[Ticker], i: usize) {
    if list[i].java {
        world.requests += 1;
    }
    let sounds = tick(world, list, i, &mut |_, _| {}).unwrap_or_default();
    for s in sounds {
        world.trace.push(Ev::Sound(i, s));
    }
    if list[i].java {
        world.trace.push(Ev::Finish(i));
    }
}

/// Vanilla's loop over the ticker list, with no crossings to think of.
fn reference(list: &[Ticker]) -> World {
    let mut world = World::new(list.len());
    for i in 0..list.len() {
        if world.live[i] {
            tick_alone(&mut world, list, i);
        }
    }
    world
}

/// `tick_block_entities` with runs of at most `cap` entries, Java's work budget spent after `budget` entries.
fn ticked(list: &[Ticker], cap: usize, budget: usize) -> World {
    let mut world = World::new(list.len());
    let mut next = 0;
    while next < list.len() {
        let i = next;
        next += 1;
        if !world.live[i] {
            continue;
        }
        if !list[i].java {
            tick_alone(&mut world, list, i);
            continue;
        }
        let slots: Vec<usize> = (i..list.len()).collect();
        let len = java_be_run_len(&slots, java_be_run_cap(cap), |k, slot| {
            (k == 0 || world.live[slot]) && list[slot].java
        });
        if len < 2 {
            tick_alone(&mut world, list, i);
            continue;
        }
        next += run(&mut world, list, i, len, budget) - 1;
    }
    world
}

/// One run of `len` entries from `start`: Java's loop, then Rust's handling of the reply; how many entries it ticked.
fn run(world: &mut World, list: &[Ticker], start: usize, len: usize, budget: usize) -> usize {
    world.requests += 1;
    let mut state = Run::new(vec![(0, 0, 0); len]);
    let mut stopped = false;
    let mut replies: Vec<Option<Vec<u8>>> = Vec::new();
    for item in 0..len {
        if item > 0 && (item >= budget || stopped) {
            break;
        }
        let reply = tick(world, list, start + item, &mut |world, ends| {
            // `java_run_native`: the first write stops Java and puts the entries before it back.
            if state.stops_at(ends) {
                stopped = true;
                let (finish, _) = state.stop(item);
                world.trace.extend(finish.map(|k| Ev::Finish(start + k)));
            }
        });
        let last = reply.as_ref().is_none_or(|sounds| !sounds.is_empty());
        replies.push(reply);
        if last {
            break;
        }
    }
    let processed = java_be_run_processed(replies.len(), state.wrote_at(), len);
    for j in 0..processed {
        let sounds = replies.get(j).cloned().flatten().unwrap_or_default();
        world
            .trace
            .extend(sounds.into_iter().map(|s| Ev::Sound(start + j, s)));
        if j >= state.done() {
            world.trace.push(Ev::Finish(start + j));
        }
    }
    processed
}

/// What the world saw apart from when each entry's native rest ran.
fn seen(world: &World) -> Vec<Ev> {
    world
        .trace
        .iter()
        .filter(|e| !matches!(e, Ev::Finish(_)))
        .cloned()
        .collect()
}

fn finishes(world: &World) -> Vec<usize> {
    world
        .trace
        .iter()
        .filter_map(|e| match e {
            Ev::Finish(i) => Some(*i),
            _ => None,
        })
        .collect()
}

/// Every Java entry's native rest runs before the next native slot ticks, and before any sound or write of a later entry.
fn finished_in_time(world: &World, list: &[Ticker]) {
    for (at, ev) in world.trace.iter().enumerate() {
        let Ev::Finish(i) = *ev else { continue };
        let late = world.trace[..at].iter().find(|e| match **e {
            Ev::Tick(j) => j > i && !list[j].java,
            Ev::Write(j, ..) | Ev::Removed(j, _) | Ev::Sound(j, _) => j > i,
            _ => false,
        });
        assert!(late.is_none(), "entry {i} finished after {late:?}");
    }
}

/// The reference, the scalar path and bulk runs under several caps and budgets give the same trace and world.
fn same(list: &[Ticker]) -> World {
    let want = reference(list);
    for (cap, budget) in [
        (1, usize::MAX),
        (2, usize::MAX),
        (64, usize::MAX),
        (64, 1),
        (64, 2),
    ] {
        let got = ticked(list, cap, budget);
        let case = format!("cap {cap}, budget {budget}");
        assert_eq!(seen(&got), seen(&want), "{case}");
        assert_eq!(finishes(&got), finishes(&want), "{case}");
        finished_in_time(&got, list);
        assert_eq!(
            (got.cells, &got.live, got.seed),
            (want.cells, &want.live, want.seed),
            "{case}"
        );
    }
    ticked(list, 64, usize::MAX)
}

#[test]
fn java_bulk_a_write_is_read_by_the_next_entry() {
    same(&[
        java(&[Op::Read(0), Op::Write(0, 7)]),
        java(&[Op::Read(0), Op::Write(1, 8)]),
        java(&[Op::Read(1)]),
    ]);
}

#[test]
fn java_bulk_java_machines_and_native_hoppers_keep_their_turns() {
    let list = [
        java(&[Op::Read(0)]),
        java(&[Op::Read(1)]),
        native(&[Op::Read(0), Op::Write(0, 1)]),
        java(&[Op::Read(0)]),
        native(&[Op::Write(1, 2)]),
        java(&[Op::Read(1)]),
        java(&[Op::Read(1)]),
    ];
    let bulk = same(&list);
    // Runs of two around the hoppers: three requests for five Java ticks.
    assert_eq!(bulk.requests, 3);
}

#[test]
fn java_bulk_an_entry_that_removes_the_next_ends_the_run_and_the_next_never_ticks() {
    let bulk = same(&[
        java(&[Op::Remove(1)]),
        java(&[Op::Write(0, 99)]),
        java(&[Op::Read(0)]),
    ]);
    assert!(!bulk.trace.contains(&Ev::Tick(1)));
}

#[test]
fn java_bulk_a_chunk_unload_takes_the_rest_of_the_run() {
    same(&[
        java(&[Op::Read(2)]),
        java(&[Op::Remove(2), Op::Remove(3), Op::Remove(4)]),
        java(&[Op::Read(2)]),
        java(&[Op::Read(2)]),
        java(&[Op::Read(2)]),
        java(&[Op::Read(3)]),
    ]);
}

#[test]
fn java_bulk_nested_set_block_callbacks_read_their_own_writes() {
    same(&[
        java(&[Op::Read(0)]),
        java(&[Op::Nested(0, 5), Op::Nested(1, 6), Op::Read(1)]),
        java(&[Op::Read(0), Op::Read(1)]),
    ]);
}

#[test]
fn java_bulk_random_draws_and_sounds_keep_their_order() {
    same(&[
        java(&[Op::Rng, Op::Sound(1)]),
        java(&[Op::Rng]),
        java(&[Op::Rng, Op::Write(0, 3), Op::Sound(2), Op::Rng]),
        native(&[Op::Sound(3), Op::Rng]),
        java(&[Op::Rng, Op::Sound(4), Op::Sound(5)]),
        java(&[Op::Rng]),
    ]);
}

#[test]
fn java_bulk_an_exception_after_a_processed_prefix_drops_only_its_own_effects() {
    let bulk = same(&[
        java(&[Op::Write(3, 1)]),
        java(&[Op::Read(3), Op::Rng]),
        java(&[Op::Rng, Op::Sound(9), Op::Throw]),
        java(&[Op::Read(3)]),
        java(&[Op::Write(3, 4), Op::Throw]),
        java(&[Op::Read(3)]),
    ]);
    assert!(!bulk.trace.contains(&Ev::Sound(2, 9)));
    assert_eq!(
        bulk.trace
            .iter()
            .filter(|e| matches!(e, Ev::Tick(_)))
            .count(),
        6
    );
}

#[test]
fn java_bulk_reads_only_cross_once_per_run_with_no_callback_per_entry() {
    const READS: Ticker = java(&[Op::Read(0)]);
    let list = [READS; 10];
    let scalar = ticked(&list, 1, usize::MAX);
    let whole = ticked(&list, 64, usize::MAX);
    let capped = ticked(&list, 4, usize::MAX);
    let budgeted = ticked(&list, 64, 3);
    assert_eq!(
        [
            scalar.requests,
            whole.requests,
            capped.requests,
            budgeted.requests
        ],
        [10, 1, 3, 4]
    );
    // Each read is its own callback; asking whether an entry may start costs none.
    assert_eq!(whole.callbacks, 10);
    assert_eq!(seen(&whole), seen(&scalar));
}

#[test]
fn java_bulk_a_lone_entry_and_an_empty_list_tick_as_the_scalar_path_does() {
    same(&[java(&[Op::Read(0), Op::Write(0, 1)])]);
    same(&[]);
}

#[test]
fn java_bulk_the_first_entry_writing_ends_the_run_after_it() {
    let bulk = same(&[
        java(&[Op::Write(0, 1), Op::Read(0), Op::Write(1, 2)]),
        java(&[Op::Read(1)]),
        java(&[Op::Read(0)]),
    ]);
    assert_eq!(bulk.requests, 2);
}

#[test]
fn java_bulk_a_stop_finishes_each_earlier_entry_once() {
    let mut state = Run::new(vec![(0, 0, 0); 5]);
    assert!(!state.stops_at(false));
    assert!(state.stops_at(true));
    assert_eq!(state.stop(3), (0..3, 3));
    assert_eq!((state.wrote_at(), state.done()), (Some(3), 3));
    assert!(!state.stops_at(true), "only the first write stops");
    // Java can't be past the run's end; a stop there is its last entry's.
    let mut state = Run::new(vec![(0, 0, 0); 2]);
    assert_eq!(state.stop(9), (0..1, 1));
    assert_eq!(java_be_run_processed(0, None, 4), 1);
    assert_eq!(java_be_run_processed(1, Some(2), 4), 3);
    assert_eq!(java_be_run_processed(9, None, 4), 4);
}
