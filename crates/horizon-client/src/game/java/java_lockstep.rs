// game-module-cfg: feature java-mods
//! The lockstep client tick: the Java tick parks at `handleKeybinds` (K) and the local player's `aiStep` (M) while the window's tick runs there.

use super::*;
use super::java_listener::listens;
use horizon_client::sprint::Keys;
use horizon_core::geometry::Vec3;
use mod_api::HookId;
use horizon_java_host::wire::{In, Out};
use horizon_protocol::packets::play;
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
    /// The field-of-view modifier the Java camera tick's event left, until `tick_fov` takes it.
    fov_target: Option<f32>,
    /// Writes an event's listeners made where no `Game` is at hand (yaw, pitch, abilities), applied before the step's fill.
    pending: MotionWrites,
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
        s.fov_target = None;
        s.pending = MotionWrites::default();
    });
    LockstepTick
}

/// The park record's motion log (`int len`, bytes), then `bool present` and `float target`; nothing from an empty or short record.
fn fov_target_of(record: &[u8]) -> Option<f32> {
    let mut r = In(record);
    r.bytes()?;
    let present = r.bool()?;
    let target = r.float()?;
    present.then_some(target)
}

/// The Java camera tick's modifier for this window tick, once; None outside lockstep or when Java sent none.
pub(in crate::game) fn take_fov_target() -> Option<f32> {
    LOCKSTEP.with_borrow_mut(|s| s.fov_target.take())
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

/// What mods wrote to the local player since the last step, applied in order; `OldPos` marks where vanilla took `xo/yo/zo`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::game) struct MotionWrites {
    pub entries: Vec<MotionWrite>,
}

/// One setter a mod called on the local player, with the value the setter left (`MotionLog`'s kinds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::game) enum MotionWrite {
    Velocity([f64; 3]),
    Position([f64; 3]),
    Yaw(f32),
    Pitch(f32),
    Ground(bool),
    FallReset,
    Sprint(bool),
    Abilities {
        flying: bool,
        may_fly: bool,
        flying_speed: f32,
    },
    OldPos,
}

/// `MotionLog.take`'s bytes: `int n`, then each entry's kind and body; None for an unknown kind or a short read.
pub(in crate::game) fn read_motion_writes(bytes: &mut In) -> Option<MotionWrites> {
    let n = usize::try_from(bytes.int()?).ok()?;
    let mut entries = Vec::with_capacity(n.min(256));
    for _ in 0..n {
        let triple = |b: &mut In| Some([b.double()?, b.double()?, b.double()?]);
        entries.push(match bytes.byte()? {
            1 => MotionWrite::Velocity(triple(bytes)?),
            2 => MotionWrite::Position(triple(bytes)?),
            3 => MotionWrite::Yaw(bytes.float()?),
            4 => MotionWrite::Pitch(bytes.float()?),
            5 => MotionWrite::Ground(bytes.bool()?),
            6 => MotionWrite::FallReset,
            7 => MotionWrite::Sprint(bytes.bool()?),
            8 => MotionWrite::Abilities {
                flying: bytes.bool()?,
                may_fly: bytes.bool()?,
                flying_speed: bytes.float()?,
            },
            9 => MotionWrite::OldPos,
            _ => return None,
        });
    }
    Some(MotionWrites { entries })
}

/// The local player's state a write log reaches.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct LocalMotion {
    position: [f64; 3],
    velocity: [f64; 3],
    yaw: f32,
    pitch: f32,
    on_ground: bool,
    fall_distance: f64,
    sprinting: bool,
    flying: bool,
    may_fly: bool,
    flying_speed: f32,
}

/// What applying a log left besides the state: the marked old position, a pitch write, and the abilities packets' flags to send.
#[derive(Debug, Default, PartialEq)]
struct Applied {
    old: Option<[f64; 3]>,
    turned: bool,
    abilities: Vec<i8>,
}

fn apply_motion_writes(m: &mut LocalMotion, writes: &MotionWrites) -> Applied {
    let mut applied = Applied::default();
    for write in &writes.entries {
        match *write {
            MotionWrite::Velocity(v) => m.velocity = v,
            MotionWrite::Position(p) => m.position = p,
            MotionWrite::Yaw(y) => m.yaw = y,
            MotionWrite::Pitch(x) => {
                m.pitch = x;
                applied.turned = true;
            }
            MotionWrite::Ground(g) => m.on_ground = g,
            MotionWrite::FallReset => m.fall_distance = 0.0,
            MotionWrite::Sprint(s) => m.sprinting = s,
            MotionWrite::Abilities {
                flying,
                may_fly,
                flying_speed,
            } => {
                // `ServerboundPlayerAbilitiesPacket` writes 2 for flying.
                if flying != m.flying {
                    applied.abilities.push(if flying { 2 } else { 0 });
                }
                m.flying = flying;
                m.may_fly = may_fly;
                m.flying_speed = flying_speed;
            }
            MotionWrite::OldPos => applied.old = Some(m.position),
        }
    }
    applied
}

/// A lockstep inbox call from the window thread while Java is parked; None outside a lockstep tick.
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

/// The window's player step is parked in Java, so its events can be posted.
fn player_held() -> bool {
    stage() == Stage::PlayerHeld
}

/// `Input`'s bits as the fill and the input packet order them: forward 1, backward 2, left 4, right 8, jump 16, shift 32, sprint 64.
fn keys_bits(keys: &Keys) -> u8 {
    u8::from(keys.forward)
        | u8::from(keys.backward) << 1
        | u8::from(keys.left) << 2
        | u8::from(keys.right) << 3
        | u8::from(keys.jump) << 4
        | u8::from(keys.shift) << 5
        | u8::from(keys.sprint) << 6
}

fn set_keys_bits(keys: &mut Keys, bits: u8) {
    keys.forward = bits & 1 != 0;
    keys.backward = bits & 2 != 0;
    keys.left = bits & 4 != 0;
    keys.right = bits & 8 != 0;
    keys.jump = bits & 16 != 0;
    keys.shift = bits & 32 != 0;
    keys.sprint = bits & 64 != 0;
}

/// Request kind 1: `byte keyPresses`, `float left`, `float forward`.
fn input_request(bits: u8, vector: [f32; 2]) -> Vec<u8> {
    let mut out = Out::default();
    out.0.push(bits);
    out.float(vector[0]);
    out.float(vector[1]);
    out.0
}

/// Reply to kind 1: the bits and move vector the listeners left, then the write log.
fn read_input_reply(reply: &[u8]) -> Option<(u8, [f32; 2], MotionWrites)> {
    let mut r = In(reply);
    let bits = r.byte()?;
    let vector = [r.float()?, r.float()?];
    Some((bits, vector, read_motion_writes(&mut r)?))
}

/// Request kind 2: the velocity vanilla's jump just set.
fn jump_request(velocity: [f64; 3]) -> Vec<u8> {
    let mut out = Out::default();
    velocity.iter().for_each(|v| out.double(*v));
    out.0
}

/// Request kind 3: `bool flyable`, `double distance`, `float multiplier`.
fn fall_request(flyable: bool, distance: f64, multiplier: f32) -> Vec<u8> {
    let mut out = Out::default();
    out.bool(flyable);
    out.double(distance);
    out.float(multiplier);
    out.0
}

/// Reply to kind 3: `bool canceled`, `double distance`, `float multiplier`, then the write log.
fn read_fall_reply(reply: &[u8]) -> Option<(bool, f64, f32, MotionWrites)> {
    let mut r = In(reply);
    let canceled = r.bool()?;
    let distance = r.double()?;
    let multiplier = r.float()?;
    Some((canceled, distance, multiplier, read_motion_writes(&mut r)?))
}

fn told_broken() {
    static TOLD: std::sync::Once = std::sync::Once::new();
    TOLD.call_once(|| eprintln!("Horizon: a lockstep event reply ended in pieces"));
}

/// Applies a log to the player where its step stands; yaw, pitch and abilities wait for the fill, where `Game` holds them.
pub(in crate::game) fn apply_motion_writes_to(
    player: &mut horizon_client::motion::PlayerMotion,
    writes: &MotionWrites,
    pending: &mut MotionWrites,
) {
    for write in &writes.entries {
        match *write {
            MotionWrite::Velocity([x, y, z]) => player.velocity = Vec3::new(x, y, z),
            MotionWrite::Position([x, y, z]) => player.position = Vec3::new(x, y, z),
            MotionWrite::Ground(g) => player.ground.on_ground = g,
            MotionWrite::FallReset => player.fall_distance = 0.0,
            MotionWrite::Sprint(s) => player.sprinting = s,
            other => pending.entries.push(other),
        }
    }
}

/// `LivingJumpEvent` for the window's player, with the jump's velocity in place.
pub(in crate::game) fn living_jump(player: &mut horizon_client::motion::PlayerMotion) {
    if !player_held() || !listens(HookId::LivingJump) {
        return;
    }
    let v = player.velocity;
    let Some(reply) = java_lockstep_call(2, &jump_request([v.x, v.y, v.z])) else {
        return;
    };
    let Some(writes) = read_motion_writes(&mut In(&reply)) else {
        return told_broken();
    };
    LOCKSTEP.with_borrow_mut(|s| apply_motion_writes_to(player, &writes, &mut s.pending));
}

/// `LivingFallEvent` (or `PlayerFlyableFallEvent`) for the window's player: the listeners' cancel, distance and multiplier.
pub(in crate::game) fn living_fall(
    flyable: bool,
    distance: &mut f64,
    multiplier: &mut f32,
    cancel: &mut bool,
) {
    let hook = if flyable {
        HookId::PlayerFlyableFall
    } else {
        HookId::LivingFall
    };
    if !player_held() || !listens(hook) {
        return;
    }
    let Some(reply) = java_lockstep_call(3, &fall_request(flyable, *distance, *multiplier)) else {
        return;
    };
    let Some((canceled, new_distance, new_multiplier, writes)) = read_fall_reply(&reply) else {
        return told_broken();
    };
    *cancel |= canceled;
    *distance = new_distance;
    *multiplier = new_multiplier;
    LOCKSTEP.with_borrow_mut(|s| s.pending.entries.extend(writes.entries));
}

impl Game<'_> {
    /// `MovementInputUpdateEvent` on the keys native just read; the listeners' keys and move vector replace native's only where they differ, so native's own direction stays exact.
    pub(in crate::game) fn java_lockstep_movement_input(
        &mut self,
        keys: &mut Keys,
        strafe: &mut f32,
        forward: &mut f32,
    ) {
        if !player_held() || !listens(HookId::MovementInput) {
            return;
        }
        let sent = [*strafe, *forward];
        let Some(reply) = java_lockstep_call(1, &input_request(keys_bits(keys), sent)) else {
            return;
        };
        let Some((bits, vector, writes)) = read_input_reply(&reply) else {
            return told_broken();
        };
        set_keys_bits(keys, bits);
        if vector != sent {
            [*strafe, *forward] = vector;
            keys.analog = Some(vector);
        }
        self.java_apply_motion_writes(&writes);
    }
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
        let waited = horizon_client::spans::time("lockstep_m", || wait(Point::M));
        if let Some((_, record)) = &waited {
            LOCKSTEP.with_borrow_mut(|s| s.fov_target = fov_target_of(record));
        }
        match waited {
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
        let pending = LOCKSTEP.with_borrow_mut(|s| std::mem::take(&mut s.pending));
        self.java_apply_motion_writes(&pending);
        let fill = self.java_lockstep_fill(moved, keys);
        release(Point::M, &fill);
        set_stage(Stage::Done);
    }

    /// The park record's motion log (`int len` and the log), applied to the window's player; where vanilla took `xo/yo/zo`, when it marked it.
    pub(in crate::game) fn java_lockstep_motion_writes(&mut self, record: &[u8]) -> Option<Vec3> {
        let writes = In(record)
            .bytes()
            .and_then(|log| read_motion_writes(&mut In(log)));
        let Some(writes) = writes else {
            static TOLD: std::sync::Once = std::sync::Once::new();
            TOLD.call_once(|| eprintln!("Horizon: a lockstep record ended in pieces"));
            return None;
        };
        self.java_apply_motion_writes(&writes)
    }

    /// Applies a write log to the window's player; returns the position vanilla took as `xo/yo/zo`, when the log marked it.
    pub(in crate::game) fn java_apply_motion_writes(&mut self, writes: &MotionWrites) -> Option<Vec3> {
        if writes.entries.is_empty() {
            return None;
        }
        let p = self.player.as_mut()?;
        let mut m = LocalMotion {
            position: [p.position.x, p.position.y, p.position.z],
            velocity: [p.velocity.x, p.velocity.y, p.velocity.z],
            yaw: self.yaw,
            pitch: self.pitch,
            on_ground: p.ground.on_ground,
            fall_distance: p.fall_distance,
            sprinting: p.sprinting,
            flying: p.flying,
            may_fly: p.may_fly,
            flying_speed: p.flying_speed,
        };
        let applied = apply_motion_writes(&mut m, writes);
        p.position = Vec3::new(m.position[0], m.position[1], m.position[2]);
        p.velocity = Vec3::new(m.velocity[0], m.velocity[1], m.velocity[2]);
        p.ground.on_ground = m.on_ground;
        p.fall_distance = m.fall_distance;
        p.sprinting = m.sprinting;
        p.flying = m.flying;
        p.may_fly = m.may_fly;
        p.flying_speed = m.flying_speed;
        self.yaw = m.yaw;
        self.pitch = m.pitch;
        if applied.turned
            && let Some(model) = self.player_model.as_mut()
        {
            model.head_pitch = m.pitch;
        }
        // Vanilla's `LocalPlayer.onUpdateAbilities` sends at once.
        for flags in applied.abilities {
            self.send(play::Serverbound::PlayerAbilities(
                play::PlayerAbilitiesRequest { flags },
            ));
        }
        applied.old.map(|[x, y, z]| Vec3::new(x, y, z))
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
    fn a_write_log_reads_back_as_typed_entries() {
        let mut out = Out::default();
        out.int(10);
        out.0.push(1);
        [0.0, 0.5, 0.0].iter().for_each(|v| out.double(*v));
        out.0.push(2);
        [1.5, 64.0, -2.25].iter().for_each(|v| out.double(*v));
        out.0.push(3);
        out.float(90.0);
        out.0.push(4);
        out.float(-15.5);
        out.0.push(5);
        out.bool(true);
        out.0.push(6);
        out.0.push(7);
        out.bool(true);
        out.0.push(8);
        out.bool(true);
        out.bool(true);
        out.float(0.05);
        out.0.push(9);
        out.0.push(1);
        [0.25, -1.0, 2.0].iter().for_each(|v| out.double(*v));
        let read = read_motion_writes(&mut In(&out.0)).unwrap();
        assert_eq!(
            read.entries,
            vec![
                MotionWrite::Velocity([0.0, 0.5, 0.0]),
                MotionWrite::Position([1.5, 64.0, -2.25]),
                MotionWrite::Yaw(90.0),
                MotionWrite::Pitch(-15.5),
                MotionWrite::Ground(true),
                MotionWrite::FallReset,
                MotionWrite::Sprint(true),
                MotionWrite::Abilities {
                    flying: true,
                    may_fly: true,
                    flying_speed: 0.05
                },
                MotionWrite::OldPos,
                MotionWrite::Velocity([0.25, -1.0, 2.0]),
            ]
        );
    }

    #[test]
    fn an_unknown_kind_or_a_short_log_reads_as_nothing() {
        let mut out = Out::default();
        out.int(1);
        out.0.push(42);
        assert_eq!(read_motion_writes(&mut In(&out.0)), None);
        let mut out = Out::default();
        out.int(2);
        out.0.push(6);
        assert_eq!(read_motion_writes(&mut In(&out.0)), None);
    }

    #[test]
    fn writes_apply_in_order_and_the_old_position_is_where_the_mark_fell() {
        let mut m = LocalMotion {
            position: [0.5, 64.0, 0.5],
            ..LocalMotion::default()
        };
        let writes = MotionWrites {
            entries: vec![
                MotionWrite::Velocity([0.0, 0.5, 0.0]),
                MotionWrite::Position([0.5, 70.0, 0.5]),
                MotionWrite::OldPos,
                MotionWrite::Velocity([0.1, 0.2, 0.3]),
            ],
        };
        let applied = apply_motion_writes(&mut m, &writes);
        assert_eq!(m.velocity, [0.1, 0.2, 0.3]);
        assert_eq!(m.position, [0.5, 70.0, 0.5]);
        assert_eq!(applied.old, Some([0.5, 70.0, 0.5]));
        assert!(applied.abilities.is_empty());
    }

    #[test]
    fn turning_flight_on_sends_one_abilities_packet_flying() {
        let mut m = LocalMotion::default();
        let on = MotionWrite::Abilities {
            flying: true,
            may_fly: true,
            flying_speed: 0.05,
        };
        let applied = apply_motion_writes(
            &mut m,
            &MotionWrites {
                entries: vec![on, on],
            },
        );
        assert_eq!(applied.abilities, vec![2]);
        assert!(m.flying && m.may_fly);
        assert_eq!(applied.old, None);
    }

    #[test]
    fn the_park_record_carries_the_fov_target_when_java_decided_one() {
        let mut out = Out::default();
        out.int(0);
        out.bool(true);
        out.float(0.5);
        assert_eq!(fov_target_of(&out.0), Some(0.5));
        let mut none = Out::default();
        none.int(0);
        none.bool(false);
        none.float(0.0);
        assert_eq!(fov_target_of(&none.0), None);
        assert_eq!(fov_target_of(&[]), None);
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

    #[test]
    fn input_bits_map_to_keys_in_vanillas_input_order() {
        let fields: [fn(&Keys) -> bool; 7] = [
            |k| k.forward,
            |k| k.backward,
            |k| k.left,
            |k| k.right,
            |k| k.jump,
            |k| k.shift,
            |k| k.sprint,
        ];
        for (i, field) in fields.iter().enumerate() {
            let mut keys = Keys::default();
            set_keys_bits(&mut keys, 1 << i);
            assert!(field(&keys));
            assert_eq!(fields.iter().filter(|f| f(&keys)).count(), 1);
            assert_eq!(keys_bits(&keys), 1 << i);
        }
    }

    #[test]
    fn an_input_reply_reads_bits_vector_and_log() {
        let mut out = Out::default();
        out.0.push(0b101_0001);
        out.float(0.0);
        out.float(1.0);
        out.int(1);
        out.0.push(6);
        let (bits, vector, writes) = read_input_reply(&out.0).unwrap();
        assert_eq!((bits, vector), (0b101_0001, [0.0, 1.0]));
        assert_eq!(writes.entries, vec![MotionWrite::FallReset]);
        assert!(read_input_reply(&out.0[..5]).is_none());
        let sent = input_request(0b10, [0.5, -1.0]);
        let mut r = In(&sent);
        assert_eq!((r.byte(), r.float(), r.float()), (Some(2), Some(0.5), Some(-1.0)));
    }

    #[test]
    fn a_fall_reply_reads_the_listeners_answer() {
        let mut out = Out::default();
        out.bool(true);
        out.double(5.5);
        out.float(0.5);
        out.int(0);
        assert_eq!(
            read_fall_reply(&out.0).map(|(c, d, m, w)| (c, d, m, w.entries.len())),
            Some((true, 5.5, 0.5, 0))
        );
    }

    #[test]
    fn jump_writes_reach_the_motion_and_the_rest_waits_for_the_fill() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/horizon-client")
            .join(horizon_core::VANILLA_VERSION);
        if !root.join("movement-properties.nbt").exists() {
            eprintln!("skipped: prepared resources unavailable");
            return;
        }
        let assets = horizon_client::assets::Assets::load(&root).unwrap();
        let resources = horizon_client::motion::MotionResources::load(&root, &assets).unwrap();
        let mut player = horizon_client::motion::PlayerMotion::new(&resources, Vec3::ZERO).unwrap();
        let mut pending = MotionWrites::default();
        let writes = MotionWrites {
            entries: vec![
                MotionWrite::Velocity([0.0, 0.92, 0.0]),
                MotionWrite::Yaw(90.0),
                MotionWrite::Sprint(true),
            ],
        };
        apply_motion_writes_to(&mut player, &writes, &mut pending);
        assert_eq!(player.velocity.y, 0.92);
        assert!(player.sprinting);
        assert_eq!(pending.entries, vec![MotionWrite::Yaw(90.0)]);
    }
}
