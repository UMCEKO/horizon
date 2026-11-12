// game-module-cfg: feature java-mods
//! #268: a run of adjacent Java block entities in the ticker list ticks in one crossing, at the scalar path's sync points.

use super::*;
use horizon_client::block_entities::Pos;
use horizon_java_host::bulk;
use java_mods::JavaSound;
use native_hooks::TakenModBlockEntity;
use std::cell::{Cell, RefCell};

/// Most entries one run carries, so its hold of Java's server turn stays bounded.
const RUN_CAP: usize = 64;
/// Java's time for one run before it stops at the next entry: about 44 crusher ticks at POC-TICK's 6.84 µs each, under one `level_tick`'s lock wait.
const RUN_BUDGET_US: u32 = 300;

/// What Java did with a run: refused it whole, or ticked a prefix, each entry's (held, sounds) in order.
enum RunReply {
    Rejected,
    Ticked(Vec<(bool, Vec<JavaSound>)>),
}

/// Reads the reply to a run of `sent` entries, checked whole before any entry's effects are used.
fn java_be_run_reply(bytes: &[u8], sent: usize) -> std::result::Result<RunReply, bulk::Error> {
    let reply = bulk::read_reply(bytes, sent)?;
    if reply.rejected {
        return Ok(RunReply::Rejected);
    }
    Ok(RunReply::Ticked(
        reply
            .items()
            .map(|(status, mut effects)| {
                let sounds = match status {
                    bulk::Status::Threw => Vec::new(),
                    _ => java_mods::java_read_effects(&mut effects).unwrap_or_default(),
                };
                (java_be_run_entry(status), sounds)
            })
            .collect(),
    ))
}

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

/// The work budget of a run in µs: `HORIZON_JAVA_BE_RUN_BUDGET_US` when set, else `RUN_BUDGET_US`.
fn run_budget_us() -> u32 {
    static BUDGET: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    *BUDGET.get_or_init(|| {
        std::env::var("HORIZON_JAVA_BE_RUN_BUDGET_US")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .map_or(RUN_BUDGET_US, |us| us.clamp(1, i32::MAX as u32))
    })
}

/// How many entries a run may carry under `cap` and the envelope's count and byte bounds.
pub(super) fn java_be_run_cap(cap: usize) -> usize {
    cap.min(bulk::Op::BlockEntityTick.max_items())
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

/// Whether Java held an entry (a throw reads as held, as the scalar path's empty reply does).
pub(super) fn java_be_run_entry(status: bulk::Status) -> bool {
    status != bulk::Status::Gone
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
            || java_mods::has_block_events()
        {
            return 0;
        }
        // A run's callbacks tick nothing else as a run: one run per thread, so `RUN` is never shared across depths.
        if RUN.with_borrow(Option::is_some) {
            return 0;
        }
        let len = java_be_run_len(slots, java_be_run_cap(run_cap()), |i, slot| {
            (i == 0 || self.block_entity_slot_ticks(slot, ticking))
                && self.java_runs_block_entity(slot.0)
        });
        if len < 2 {
            return 0;
        }
        let positions: Vec<Pos> = slots[..len].iter().map(|s| s.0).collect();
        let packed: Vec<i64> = positions
            .iter()
            .map(|p| BlockPos::new(p.0, p.1, p.2).pack())
            .collect();
        let (level, time) = (self.java_level_key(), self.world.game_time);
        // As `java_request` does before each scalar tick.
        self.java_push_stale();
        RUN.set(Some(Run {
            positions: positions.clone(),
            current: 0,
            done: 0,
            wrote: None,
        }));
        let reply = horizon_java_host::bridge::with_active(self, || {
            host.call_with(
                "block_entity_tick_run",
                |out| {
                    // Within the cap and budget bounds this can't fail; if it did, Java refuses the empty envelope.
                    let _ = bulk::block_entity_tick(out, run_budget_us(), level, time, &packed);
                },
                |reply| Some(java_be_run_reply(reply.0, len)),
            )
        });
        let run = RUN.take();
        let (done, wrote) = run.map_or((0, None), |r| (r.done, r.wrote));
        let (wrote_at, mut taken) = wrote.map_or((None, None), |(k, t)| (Some(k), t));
        let mut entries = match reply {
            // Nothing ran in Java and nothing called back, so the loop ticks the first entry on its own.
            Some(Ok(RunReply::Rejected)) if wrote_at.is_none() => return 0,
            Some(Ok(RunReply::Rejected)) => Vec::new(),
            Some(Ok(RunReply::Ticked(entries))) => entries,
            Some(Err(error)) => {
                log::error!("Horizon: Java's reply to a block entity run is malformed: {error}");
                Vec::new()
            }
            // A failed crossing is the first entry's failed scalar call: held, no effects.
            None => Vec::new(),
        }
        .into_iter();
        // The entry that wrote was ticked, whatever the reply says.
        let processed = entries
            .len()
            .max(1)
            .max(wrote_at.map_or(0, |k| k + 1))
            .min(len);
        for (j, &pos) in positions.iter().enumerate().take(processed) {
            let (held, sounds) = entries.next().unwrap_or((true, Vec::new()));
            self.java_play_effects(sounds);
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
        assert_eq!(java_be_run_cap(64), 64);
        assert_eq!(java_be_run_cap(2), 2);
        assert_eq!(java_be_run_cap(100_000), bulk::MAX_ITEMS);
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
        assert!(java_be_run_entry(bulk::Status::Done));
        assert!(!java_be_run_entry(bulk::Status::Gone));
        assert!(java_be_run_entry(bulk::Status::Threw));
    }

    /// `GameBridge.writeEffects` with `n` sounds.
    fn effects(n: usize) -> Vec<u8> {
        let mut out = horizon_java_host::wire::Out::default();
        out.int(n as i32);
        for i in 0..n {
            out.text(&format!("mod:sound_{i}"));
            out.double(1.0);
            out.double(2.0);
            out.double(3.0);
            out.float(1.0);
            out.float(0.5);
        }
        out.0
    }

    fn ticked(bytes: &[u8], sent: usize) -> Vec<(bool, usize)> {
        match java_be_run_reply(bytes, sent) {
            Ok(RunReply::Ticked(entries)) => entries.into_iter().map(|(h, s)| (h, s.len())).collect(),
            Ok(RunReply::Rejected) => panic!("rejected"),
            Err(e) => panic!("{e}"),
        }
    }

    #[test]
    fn java_be_run_reply_gives_each_entry_its_own_sounds_once() {
        let (none, one, two) = (effects(0), effects(1), effects(2));
        let mut out = horizon_java_host::wire::Out::default();
        bulk::write_reply(
            &mut out,
            &[
                (bulk::Status::Done, one.as_slice()),
                (bulk::Status::Gone, none.as_slice()),
                (bulk::Status::Done, two.as_slice()),
            ],
        );
        assert_eq!(ticked(&out.0, 3), [(true, 1), (false, 0), (true, 2)]);
    }

    #[test]
    fn java_be_run_reply_keeps_the_prefix_before_a_throw_and_nothing_after() {
        let one = effects(1);
        let mut out = horizon_java_host::wire::Out::default();
        bulk::write_reply(
            &mut out,
            &[(bulk::Status::Done, one.as_slice()), (bulk::Status::Threw, &[])],
        );
        assert_eq!(ticked(&out.0, 4), [(true, 1), (true, 0)]);
    }

    #[test]
    fn java_be_run_reply_refused_or_malformed_runs_nothing() {
        let mut out = horizon_java_host::wire::Out::default();
        bulk::write_rejected(&mut out);
        assert!(matches!(java_be_run_reply(&out.0, 3), Ok(RunReply::Rejected)));
        let mut out = horizon_java_host::wire::Out::default();
        let none = effects(0);
        bulk::write_reply(&mut out, &[(bulk::Status::Done, none.as_slice()); 3]);
        assert!(java_be_run_reply(&out.0, 2).is_err(), "more entries than were sent");
        assert!(java_be_run_reply(&out.0[..out.0.len() - 1], 3).is_err(), "a cut reply");
    }
}
