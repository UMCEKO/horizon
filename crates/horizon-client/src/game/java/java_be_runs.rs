// game-module-cfg: feature java-mods
//! #268: a run of adjacent Java block entities in the ticker list ticks in one crossing, at the scalar path's sync points.

use super::*;
use horizon_client::block_entities::Pos;
use horizon_java_host::wire::{In, Out};
use native_hooks::TakenModBlockEntity;
use std::cell::{Cell, RefCell};

/// Most entries one run carries, so its hold of Java's server turn stays bounded.
const RUN_CAP: usize = 64;
/// Most request bytes one run carries.
const RUN_BYTES: usize = 4096;
/// Request bytes per entry: its packed position.
const ENTRY_BYTES: usize = 8;
/// An entry's status in the reply: 0 Java held none there, 1 held and ticked, 2 its tick threw.
const GONE: u8 = 0;

/// A run's entries while its crossing is out: which one Java ticks, and the first write back into Horizon.
struct Run {
    positions: Vec<Pos>,
    current: usize,
    /// Entries before this one are back in the world.
    done: usize,
    /// The entry that wrote, taken out of the world before its first write, as the scalar path has it for its whole tick.
    wrote: Option<(usize, Option<TakenModBlockEntity>)>,
}

thread_local! {
    static RUN: RefCell<Option<Run>> = const { RefCell::new(None) };
    static PRETICKED: Cell<Option<bool>> = const { Cell::new(None) };
}

/// The entry `java_block_entity_tick` is about to tick was ticked by its run: whether Java held it.
pub(super) fn take_preticked() -> Option<bool> {
    PRETICKED.take()
}

/// `BlockEntities.runEntry`: whether entry `index` may tick, which it may until an earlier one wrote into Horizon.
pub(super) fn run_entry(index: i32) -> bool {
    RUN.with_borrow_mut(|run| match run {
        Some(run) if run.wrote.is_none() => {
            run.current = usize::try_from(index).unwrap_or(0);
            true
        }
        _ => false,
    })
}

/// The cap of a run: `HORIZON_JAVA_BE_RUN_CAP` when set (1 is the scalar path), else `RUN_CAP`.
fn run_cap() -> usize {
    static CAP: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *CAP.get_or_init(|| {
        std::env::var("HORIZON_JAVA_BE_RUN_CAP")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .map_or(RUN_CAP, |cap| cap.max(1))
    })
}

/// How many entries a run whose fixed fields take `header` bytes may carry under `cap`.
pub(super) fn java_be_run_cap(cap: usize, header: usize) -> usize {
    cap.min(RUN_BYTES.saturating_sub(header) / ENTRY_BYTES)
}

/// How many slots from the start of `slots` one run takes: each that `joins` (told its index), up to `cap`.
pub(super) fn java_be_run_len<S: Copy>(
    slots: &[S],
    cap: usize,
    mut joins: impl FnMut(usize, S) -> bool,
) -> usize {
    slots
        .iter()
        .take(cap)
        .enumerate()
        .take_while(|(i, slot)| joins(*i, **slot))
        .count()
}

/// One entry of a run's reply: whether Java held it (a throw reads as held, as the scalar path's empty reply does).
pub(super) fn java_be_run_entry(reply: &mut In<'_>) -> Option<bool> {
    Some(reply.byte()? != GONE)
}

impl Game<'_> {
    /// Whether the slot's block entity may join a run: a mod's whose block Java owns, and whose tick reaches the Java hook.
    fn java_runs_block_entity(&self, pos: Pos) -> bool {
        horizon_java_host::bridge::java_block(self.world.block(pos.0, pos.1, pos.2))
            && self.mod_block_entity_tick_reaches_hook(pos)
    }

    /// The ticks of the Java block entities from `slots[0]` on in one crossing: how many slots it ticked, 0 when `slots[0]` ticks alone.
    pub(super) fn java_block_entity_run(
        &mut self,
        slots: &[(Pos, u32)],
        ticking: &dyn Fn(i32, i32) -> bool,
    ) -> usize {
        let Some(host) = horizon_java_host::get() else {
            return 0;
        };
        // A listener on the hook, or events the scalar path drains after the first entry, keep each tick its own.
        if mod_api::__private::has_scoped_hook(mod_api::HookId::BlockEntityTick)
            || java_mods::block_events_pending()
        {
            return 0;
        }
        let dim = self.dim.id();
        let cap = java_be_run_cap(run_cap(), 4 + dim.len() + 8 + 4);
        let len = java_be_run_len(slots, cap, |i, slot| {
            (i == 0 || self.block_entity_slot_ticks(slot, ticking))
                && self.java_runs_block_entity(slot.0)
        });
        if len < 2 {
            return 0;
        }
        let positions: Vec<Pos> = slots[..len].iter().map(|s| s.0).collect();
        // As `java_request` does before each scalar tick.
        self.java_push_stale();
        let mut out = Out::default();
        out.text(self.dim.id());
        out.long(self.world.game_time);
        out.int(len as i32);
        for p in &positions {
            out.long(BlockPos::new(p.0, p.1, p.2).pack());
        }
        RUN.set(Some(Run {
            positions: positions.clone(),
            current: 0,
            done: 0,
            wrote: None,
        }));
        let reply = horizon_java_host::bridge::with_active(self, || {
            host.request("block_entity_tick_run", &out.0)
        });
        let run = RUN.take();
        let (done, wrote) = run.map_or((0, None), |r| (r.done, r.wrote));
        let mut reply = In(reply.as_deref().unwrap_or_default());
        // A failed crossing is the first entry's failed scalar call: held, no effects.
        let processed = reply.int().map_or(0, |n| n.clamp(0, len as i32) as usize);
        let (wrote_at, mut taken) = wrote.map_or((None, None), |(k, t)| (Some(k), t));
        // The entry that wrote was ticked, whatever the reply says.
        let processed = processed.max(1).max(wrote_at.map_or(0, |k| (k + 1).min(len)));
        for (j, &pos) in positions.iter().enumerate().take(processed) {
            let held = java_be_run_entry(&mut reply).unwrap_or(true);
            self.java_apply_effects(&mut reply);
            if j < done {
                if !held {
                    java_mods::java_warn_unheld(BlockPos::new(pos.0, pos.1, pos.2));
                }
                continue;
            }
            let taken = if Some(j) == wrote_at { taken.take() } else { None };
            self.java_finish_preticked(pos, held, taken);
        }
        if profile_on() {
            horizon_java_host::profile::record_run("block_entity_tick", processed);
        }
        processed
    }

    /// The rest of `tick_mod_block_entity` for an entry its run ticked in Java.
    fn java_finish_preticked(&mut self, pos: Pos, held: bool, taken: Option<TakenModBlockEntity>) {
        let taken = taken.or_else(|| self.take_ticking_mod_block_entity(pos));
        let Some(taken) = taken else {
            if !held {
                java_mods::java_warn_unheld(BlockPos::new(pos.0, pos.1, pos.2));
            }
            return;
        };
        PRETICKED.set(Some(held));
        self.finish_mod_block_entity_tick(pos, taken);
        PRETICKED.set(None);
    }

    /// Before each native Java calls: the first that `ends_run` during a run puts the entries before its own back and takes its own out, as the scalar path has them.
    pub(super) fn java_run_native(&mut self, native: &str) {
        let start = RUN.with_borrow_mut(|run| {
            let run = run.as_mut()?;
            if run.wrote.is_some() || !java_bridges::ends_run(native) {
                return None;
            }
            run.wrote = Some((run.current, None));
            Some((run.done, run.current, run.positions.clone()))
        });
        let Some((done, current, positions)) = start else {
            return;
        };
        // Held is the reply's to say; one not held warns when the reply comes.
        for &pos in positions.iter().take(current).skip(done) {
            self.java_finish_preticked(pos, true, None);
        }
        let taken = positions
            .get(current)
            .and_then(|&pos| self.take_ticking_mod_block_entity(pos));
        RUN.with_borrow_mut(|run| {
            if let Some(run) = run {
                run.done = current;
                run.wrote = Some((current, taken));
            }
        });
    }
}

fn profile_on() -> bool {
    horizon_java_host::profile::dir().is_some()
}

#[cfg(test)]
mod java_be_run_tests {
    use super::*;

    /// A ticker slot in a test list: whether it is a Java block entity, and whether the loop's checks pass.
    #[derive(Clone, Copy, Debug)]
    struct Slot {
        java: bool,
        live: bool,
    }

    const J: Slot = Slot {
        java: true,
        live: true,
    };
    const N: Slot = Slot {
        java: false,
        live: true,
    };

    fn run(slots: &[Slot], cap: usize) -> usize {
        java_be_run_len(slots, cap, |_, s| s.java && s.live)
    }

    #[test]
    fn java_be_run_stops_at_the_first_other_slot() {
        assert_eq!(run(&[J, J, N, J], 64), 2);
        assert_eq!(run(&[N, J, J], 64), 0);
        assert_eq!(run(&[J, N], 64), 1);
        assert_eq!(run(&[J, J, J], 64), 3);
    }

    #[test]
    fn java_be_run_of_nothing_is_empty() {
        assert_eq!(run(&[], 64), 0);
    }

    #[test]
    fn java_be_run_keeps_to_its_cap_and_a_cap_of_one_is_the_scalar_path() {
        let slots = [J; 10];
        assert_eq!(run(&slots, 4), 4);
        assert_eq!(run(&slots, 1), 1);
        assert_eq!(java_be_run_cap(64, 4096 - 3 * ENTRY_BYTES), 3);
        assert_eq!(java_be_run_cap(2, 0), 2);
    }

    #[test]
    fn java_be_run_resumes_past_what_an_entry_removed() {
        // Java ticked the first entry of [A, B, C, D], which wrote and removed B: the rest is checked again.
        let mut slots = [J, J, J, J];
        assert_eq!(run(&slots, 64), 4);
        let processed = 1;
        slots[1].live = false;
        let mut at = processed;
        let mut runs = Vec::new();
        while at < slots.len() {
            match run(&slots[at..], 64) {
                0 => at += 1,
                n => {
                    runs.push((at, n));
                    at += n;
                }
            }
        }
        // A run of C and D, which start at 2.
        assert_eq!(runs, [(2, 2)]);
    }

    #[test]
    fn java_be_run_reply_entries_read_held_gone_and_threw() {
        let bytes = [1u8, 0, 2];
        let mut reply = In(&bytes);
        assert_eq!(java_be_run_entry(&mut reply), Some(true));
        assert_eq!(java_be_run_entry(&mut reply), Some(false));
        assert_eq!(java_be_run_entry(&mut reply), Some(true));
        assert_eq!(java_be_run_entry(&mut reply), None);
    }
}
