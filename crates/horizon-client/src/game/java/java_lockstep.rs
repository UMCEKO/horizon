// game-module-cfg: feature java-mods
//! The lockstep client tick: the Java tick parks at `handleKeybinds` (K) and the local player's `aiStep` (M) while the window's tick runs there.

use super::*;
use horizon_java_host::wire::{In, Out};
use std::cell::RefCell;

/// Where the window's tick is with the Java client's parks.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(in crate::game) enum Stage {
    #[default]
    Idle,
    Sent,
    KeysHeld,
    KeysReleased,
    PlayerHeld,
    Done,
}

/// A park of the Java tick; the wire value is `Lockstep.Point`'s ordinal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Point {
    K = 1,
    M = 2,
}

#[derive(Default)]
struct State {
    /// This window tick runs in lockstep.
    lockstep: bool,
    /// The Java client's last status asked for lockstep on the next tick.
    next: bool,
    stage: Stage,
    /// This tick's `animateTick` picks, sent with the release at M.
    picks: Option<Vec<[i32; 3]>>,
}

thread_local! {
    static LOCKSTEP: RefCell<State> = RefCell::default();
}

/// Releases a pending park on every exit path of the window's tick (K, then M as "no move").
pub(in crate::game) struct LockstepTick;

impl Drop for LockstepTick {
    fn drop(&mut self) {
        release_pending();
    }
}

/// What the window's player step left, in the order `ClientBridge.fill` reads it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Step {
    position: [f64; 3],
    velocity: [f64; 3],
    look: [f32; 2],
    on_ground: bool,
    horizontal_collision: bool,
    vertical_collision: bool,
    fall_distance: f64,
    sprinting: bool,
    shift_key_down: bool,
    swimming: bool,
    fall_flying: bool,
    flying: bool,
    /// `Pose`'s id, which is its ordinal.
    pose: u8,
    xxa: f32,
    zza: f32,
    jumping: bool,
    /// `Input`'s bits: forward 1, backward 2, left 4, right 8, jump 16, shift 32, sprint 64.
    keys: u8,
    move_vector: [f32; 2],
}

/// The release payload at M: `bool moved`, the step when it moved, then `int n` and the picks.
fn fill_bytes(step: Option<&Step>, picks: &[[i32; 3]]) -> Vec<u8> {
    let mut out = Out::default();
    out.bool(step.is_some());
    if let Some(s) = step {
        s.position.iter().for_each(|v| out.double(*v));
        s.velocity.iter().for_each(|v| out.double(*v));
        out.float(s.look[0]);
        out.float(s.look[1]);
        out.bool(s.on_ground);
        out.bool(s.horizontal_collision);
        out.bool(s.vertical_collision);
        out.double(s.fall_distance);
        out.bool(s.sprinting);
        out.bool(s.shift_key_down);
        out.bool(s.swimming);
        out.bool(s.fall_flying);
        out.bool(s.flying);
        out.0.push(s.pose);
        out.float(s.xxa);
        out.float(s.zza);
        out.bool(s.jumping);
        out.0.push(s.keys);
        out.float(s.move_vector[0]);
        out.float(s.move_vector[1]);
    }
    out.int(picks.len() as i32);
    picks.iter().flatten().for_each(|v| out.int(*v));
    out.0
}

/// The releases a tick leaves owing at `stage`, in order; M's says the step didn't run.
fn pending_releases(stage: Stage) -> Vec<(Point, Vec<u8>)> {
    let no_move = || (Point::M, fill_bytes(None, &[]));
    match stage {
        Stage::Sent | Stage::KeysHeld => vec![(Point::K, Vec::new()), no_move()],
        Stage::KeysReleased | Stage::PlayerHeld => vec![no_move()],
        Stage::Idle | Stage::Done => Vec::new(),
    }
}

fn wait(point: Point) -> Option<(bool, Vec<u8>)> {
    let host = horizon_java_host::get()?;
    let mut out = Out::default();
    out.int(point as i32);
    let reply = host.request("client_lockstep_wait", &out.0)?;
    let mut reply = In(&reply);
    Some((reply.bool()?, reply.bytes()?.to_vec()))
}

fn release(point: Point, payload: &[u8]) {
    let Some(host) = horizon_java_host::get() else {
        return;
    };
    let mut out = Out::default();
    out.int(point as i32);
    out.bytes(payload);
    let _ = host.request("client_lockstep_release", &out.0);
}

fn stage() -> Stage {
    LOCKSTEP.with_borrow(|s| s.stage)
}

fn set_stage(stage: Stage) {
    LOCKSTEP.with_borrow_mut(|s| s.stage = stage);
}

fn release_pending() {
    let stage = LOCKSTEP.with_borrow_mut(|s| {
        let stage = s.stage;
        if s.lockstep {
            s.stage = Stage::Done;
        }
        stage
    });
    for (point, payload) in pending_releases(stage) {
        release(point, &payload);
    }
}

/// The Java client's last status asked for lockstep on the window's next tick.
pub(in crate::game) fn lockstep_next() -> bool {
    LOCKSTEP.with_borrow(|s| s.next)
}

pub(in crate::game) fn set_lockstep_next(next: bool) {
    LOCKSTEP.with_borrow_mut(|s| s.next = next);
}

/// This window tick's job was handed over in lockstep.
pub(in crate::game) fn start() -> LockstepTick {
    LOCKSTEP.with_borrow_mut(|s| {
        s.lockstep = true;
        s.stage = Stage::Sent;
        s.picks = None;
    });
    LockstepTick
}

pub(in crate::game) fn in_lockstep() -> bool {
    LOCKSTEP.with_borrow(|s| s.lockstep)
}

/// Keeps this tick's picks for the release at M; gives them back outside a lockstep tick.
pub(in crate::game) fn keep_picks(picked: Vec<[i32; 3]>) -> Option<Vec<[i32; 3]>> {
    LOCKSTEP.with_borrow_mut(|s| {
        if !s.lockstep {
            return Some(picked);
        }
        s.picks = Some(picked);
        None
    })
}

/// The tick's end: what it still owes is released, and the picks M never took are left for the next batch.
pub(in crate::game) fn finish() -> Vec<[i32; 3]> {
    release_pending();
    LOCKSTEP.with_borrow_mut(|s| {
        s.lockstep = false;
        s.stage = Stage::Idle;
        s.picks.take().unwrap_or_default()
    })
}

/// A lockstep inbox call from the window thread while Java is parked; None outside a lockstep tick.
#[cfg_attr(not(test), expect(dead_code, reason = "SPEC-3's requests use it"))]
pub(in crate::game) fn java_lockstep_call(kind: u8, request: &[u8]) -> Option<Vec<u8>> {
    if !in_lockstep() {
        return None;
    }
    let host = horizon_java_host::get()?;
    let mut out = Out::default();
    out.0.push(kind);
    out.bytes(request);
    let reply = host.request("client_lockstep_call", &out.0)?;
    let (served, rest) = reply.split_first()?;
    (*served == 1).then(|| rest.to_vec())
}

impl Game<'_> {
    /// Waits for the Java tick to reach `handleKeybinds`, once per tick.
    pub(in crate::game) fn java_lockstep_keys_begin(&mut self) {
        if stage() != Stage::Sent {
            return;
        }
        // Not parked there means Java finished early: every later wait then returns at once.
        let _ = horizon_client::spans::time("lockstep_k", || wait(Point::K));
        set_stage(Stage::KeysHeld);
    }

    /// The window's clicks were served: the Java tick goes on from `handleKeybinds`.
    pub(in crate::game) fn java_lockstep_keys_end(&mut self) {
        if stage() != Stage::KeysHeld {
            return;
        }
        release(Point::K, &[]);
        set_stage(Stage::KeysReleased);
    }

    /// Waits for the Java tick to reach the local player's `aiStep`; its record when it parked there.
    pub(in crate::game) fn java_lockstep_player_begin(&mut self) -> Option<Vec<u8>> {
        self.java_lockstep_keys_begin();
        self.java_lockstep_keys_end();
        if stage() != Stage::KeysReleased {
            return None;
        }
        match horizon_client::spans::time("lockstep_m", || wait(Point::M)) {
            Some((true, record)) => {
                set_stage(Stage::PlayerHeld);
                Some(record)
            }
            _ => {
                set_stage(Stage::Done);
                None
            }
        }
    }

    /// The window's player step ran (`moved`) or not: the Java tick goes on from `aiStep` with its result.
    pub(in crate::game) fn java_lockstep_player_end(&mut self, moved: bool, keys: u8) {
        if stage() != Stage::PlayerHeld {
            return;
        }
        let fill = self.java_lockstep_fill(moved, keys);
        release(Point::M, &fill);
        set_stage(Stage::Done);
    }

    fn java_lockstep_fill(&self, moved: bool, keys: u8) -> Vec<u8> {
        let step = self.player.as_ref().filter(|_| moved).map(|p| Step {
            position: [p.position.x, p.position.y, p.position.z],
            velocity: [p.velocity.x, p.velocity.y, p.velocity.z],
            look: [self.yaw, self.pitch],
            on_ground: p.ground.on_ground,
            horizontal_collision: p.collision.horizontal,
            vertical_collision: p.collision.vertical,
            fall_distance: p.fall_distance,
            sprinting: p.sprinting,
            shift_key_down: p.shift_key_down,
            swimming: p.swimming,
            fall_flying: p.fall_flying,
            flying: p.flying,
            pose: p.pose.id() as u8,
            xxa: p.applied_input[0],
            zza: p.applied_input[1],
            jumping: p.jumping,
            keys,
            move_vector: p.move_vector,
        });
        let picks = LOCKSTEP.with_borrow_mut(|s| s.picks.take().unwrap_or_default());
        fill_bytes(step.as_ref(), &picks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fill_reads_back_in_the_documented_order() {
        let step = Step {
            position: [1.5, 64.0, -2.25],
            velocity: [0.1, -0.0784, 0.0],
            look: [90.0, -15.5],
            on_ground: true,
            horizontal_collision: false,
            vertical_collision: true,
            fall_distance: 0.75,
            sprinting: true,
            shift_key_down: false,
            swimming: false,
            fall_flying: true,
            flying: false,
            pose: 5,
            xxa: 0.98,
            zza: -0.5,
            jumping: true,
            keys: 1 | 16 | 64,
            move_vector: [0.0, 1.0],
        };
        let bytes = fill_bytes(Some(&step), &[[3, 70, -9]]);
        let mut r = In(&bytes);
        assert_eq!(r.bool(), Some(true));
        assert_eq!(
            [r.double(), r.double(), r.double()],
            [Some(1.5), Some(64.0), Some(-2.25)]
        );
        assert_eq!(
            [r.double(), r.double(), r.double()],
            [Some(0.1), Some(-0.0784), Some(0.0)]
        );
        assert_eq!([r.float(), r.float()], [Some(90.0), Some(-15.5)]);
        assert_eq!(
            [r.bool(), r.bool(), r.bool()],
            [Some(true), Some(false), Some(true)]
        );
        assert_eq!(r.double(), Some(0.75));
        assert_eq!(
            [r.bool(), r.bool(), r.bool(), r.bool(), r.bool()],
            [
                Some(true),
                Some(false),
                Some(false),
                Some(true),
                Some(false)
            ]
        );
        assert_eq!(r.byte(), Some(5));
        assert_eq!([r.float(), r.float()], [Some(0.98), Some(-0.5)]);
        assert_eq!(r.bool(), Some(true));
        assert_eq!(r.byte(), Some(81));
        assert_eq!([r.float(), r.float()], [Some(0.0), Some(1.0)]);
        assert_eq!(r.int(), Some(1));
        assert_eq!([r.int(), r.int(), r.int()], [Some(3), Some(70), Some(-9)]);
        assert_eq!(r.byte(), None);
    }

    #[test]
    fn no_move_is_a_false_and_no_picks() {
        assert_eq!(fill_bytes(None, &[]), vec![0, 0, 0, 0, 0]);
    }

    #[test]
    fn a_tick_owes_k_then_m_until_the_keys_are_released_and_nothing_once_done() {
        let no_move = vec![0, 0, 0, 0, 0];
        assert_eq!(
            pending_releases(Stage::KeysHeld),
            vec![(Point::K, Vec::new()), (Point::M, no_move.clone())]
        );
        assert_eq!(
            pending_releases(Stage::Sent),
            vec![(Point::K, Vec::new()), (Point::M, no_move.clone())]
        );
        assert_eq!(
            pending_releases(Stage::PlayerHeld),
            vec![(Point::M, no_move.clone())]
        );
        assert_eq!(
            pending_releases(Stage::KeysReleased),
            vec![(Point::M, no_move)]
        );
        assert!(pending_releases(Stage::Done).is_empty());
        assert!(pending_releases(Stage::Idle).is_empty());
    }

    #[test]
    fn a_dropped_guard_settles_the_tick_and_keeps_the_unsent_picks() {
        LOCKSTEP.with_borrow_mut(|s| *s = State::default());
        let guard = start();
        assert_eq!(keep_picks(vec![[1, 2, 3]]), None);
        drop(guard);
        assert_eq!(stage(), Stage::Done);
        assert_eq!(finish(), vec![[1, 2, 3]]);
        assert_eq!(stage(), Stage::Idle);
        assert_eq!(keep_picks(vec![[4, 5, 6]]), Some(vec![[4, 5, 6]]));
        assert_eq!(java_lockstep_call(1, &[]), None);
    }
}
