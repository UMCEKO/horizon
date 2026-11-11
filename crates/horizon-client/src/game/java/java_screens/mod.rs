// game-module-cfg: feature java-mods
//! Java mods' screens in Horizon's window: the Java client in the same JVM opens them from what the
//! player's server view sent it, draws them into a list Rust paints, and takes the window's mouse
//! and keys while one is open.

use super::java_hooks_client::{HudLayer, JavaClick, JavaKeyAction, Leaf};
use super::*;

mod draw;
mod hud;
mod input;
mod shell;

use self::draw::*;
use self::hud::*;

/// The Java client's liveness, where `super::java_screens::live::` paths expect it.
pub(in crate::game) mod live {
    pub(in crate::game) use super::{java_client_running, java_shape_generation};
}

/// One segment before the first layer, then two per layer.
const HUD_SEGMENTS: usize = 49;

/// Two recorded segments per `Leaf`.
const SEGMENTS: usize = 28;

use horizon_client::hud::{HudAtlas, HudVertex};
use horizon_client::text::{Style, Text};
use horizon_java_host::wire::{In, Out};
use std::cell::RefCell;
use std::sync::mpsc::TryRecvError;

#[derive(Default)]
pub(in crate::game) struct Client {
    joined: bool,
    /// What the Java client's `useItemOn` answered for the last click: 0 none, 1 passed, 2 success, 3 consumed.
    block_use: u8,
    /// Columns holding block entity models wait for the Java client's answer.
    armed: bool,
    /// Columns the Java client has taken in since its answers were last applied.
    told: Vec<(i32, i32)>,
    /// The server is in another process: the Java client's packets go over the window's connection.
    remote: bool,
    font: bool,
    owed: i32,
    open: bool,
    shapes: i32,
    /// The mods' HUD layers as the Java client last recorded them, drawn every frame until it records another.
    hud: HudRecording,
    /// The Java client says the HUD is changing: each frame is recorded with its own timing until two alike.
    hud_animating: bool,
    /// A tick's status was applied this frame: its recording is this frame's, so no frame recording follows.
    recorded: bool,
    hud_frames: (u32, u128),
    /// The selected hotbar slot the HUD recording was asked for, while Java draws the hotbar layer.
    hud_selected: usize,
    /// Bit per `HudLayer` whose after-segment this frame painted.
    hud_painted: u32,
    states: bool,
    /// Input the world took since the last tick, in order, as `events` reads it.
    queue: Vec<u8>,
    queued: i32,
    focused: Option<bool>,
    /// The Java client's playing sounds by its handle.
    sounds: std::collections::HashMap<i32, horizon_client::sound_engine::InstanceId>,
    sent: Sent,
    /// One group of picked `animateTick` positions per window tick not yet sent, oldest first.
    animate: std::collections::VecDeque<Vec<[i32; 3]>>,
    /// This tick's picks, which the tick's end turns into a group.
    picked: Vec<[i32; 3]>,
    /// The open screen's frame blurs what is behind it (`blurBeforeThisStratum`).
    blurred: bool,
    shell: Shell,
}

/// A vanilla screen of the window's that the Java client holds as a shell, so mods' screen events fire on it.
#[derive(Default)]
struct Shell {
    open: bool,
    /// Two per `Leaf`: the gap after it (what mods drew), then its own content when Java drew it.
    segments: [Vec<u8>; SEGMENTS],
    /// Bit per `Leaf`: Java drew that step, native skips its own.
    routed: u32,
    animating: bool,
    /// The cursor of the last recording, in GUI pixels.
    drawn_at: Option<(f32, f32)>,
    /// The tick that opened it has not answered yet: the next frame waits for it.
    opening: bool,
    /// A recording was applied since the last frame asked for one.
    fresh: bool,
}

/// Who took an input over a shell, as the Java client answers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ShellOwner {
    Event,
    Widget,
    Native,
    None,
}

/// The window's input as the callbacks of its library would hand it to a real client.
enum Input<'a> {
    /// `KeyboardHandler.keyPress`: the key's name, the action (release 0, press 1, repeat 2), the modifiers, SDL's keycode.
    Key(&'a str, i32, i32, i32),
    /// A key the window's own screen took: only the keyboard's state hears of it.
    KeyState(&'a str, bool),
    Char(u32),
    /// `MouseHandler.onButton`: SDL's button number, the action, the modifiers.
    Button(i32, i32, i32),
    /// `MouseHandler.onMove`, in window pixels.
    Move(f64, f64),
    Scroll(f64, f64),
    /// One of the window's own screens opened over the world (and whether it pauses), or closed.
    Screen(bool, bool, u8),
    Focus(bool),
    /// The window camera's position, yaw, pitch, field of view and far plane, told when its block or its view changes.
    Camera([f64; 3], [f32; 4], u8),
    /// `handleKeybinds`' drop, with the whole stack or not.
    Drop(bool),
    SwapHands,
    Hotbar(i32),
    /// A column of the window's arrived (true) or left.
    Chunk(i32, i32, bool),
    /// A frame of the server's Java side, for the Java client's connection.
    Wire(&'a [u8]),
    /// The positions native picked this tick that Java runs `animateTick` at.
    Animate(&'a [[i32; 3]]),
    /// The window's Particles option.
    Particles(i32),
    /// `leftPos`, `topPos`, `imageWidth`, `imageHeight` and the hovered menu slot (-1 none) of the screen native shows.
    ShellLayout([i32; 5]),
}

impl Input<'_> {
    fn write(&self, out: &mut Out) {
        let kind = match self {
            Input::Key(..) => 0,
            Input::KeyState(..) => 1,
            Input::Char(_) => 2,
            Input::Button(..) => 3,
            Input::Move(..) => 4,
            Input::Scroll(..) => 5,
            Input::Screen(..) => 6,
            Input::Focus(_) => 7,
            Input::Drop(_) => 8,
            Input::SwapHands => 9,
            Input::Hotbar(_) => 10,
            Input::Chunk(..) => 11,
            Input::Wire(_) => 12,
            Input::Camera(..) => 13,
            Input::Animate(_) => 14,
            Input::Particles(_) => 15,
            Input::ShellLayout(_) => 16,
        };
        out.0.push(kind);
        match *self {
            Input::Key(name, action, modifiers, keycode) => {
                out.text(name);
                out.int(action);
                out.int(modifiers);
                out.int(keycode);
            }
            Input::KeyState(name, down) => {
                out.text(name);
                out.bool(down);
            }
            Input::Char(c) => out.int(c as i32),
            Input::Button(button, action, modifiers) => {
                out.int(button);
                out.int(action);
                out.int(modifiers);
            }
            Input::Move(x, y) | Input::Scroll(x, y) => {
                out.double(x);
                out.double(y);
            }
            Input::Screen(open, pauses, shell) => {
                out.bool(open);
                out.bool(pauses);
                out.0.push(shell);
            }
            Input::Focus(focused) | Input::Drop(focused) => out.bool(focused),
            Input::SwapHands => {}
            Input::Hotbar(slot) => out.int(slot),
            Input::Chunk(x, z, arrived) => {
                out.int(x);
                out.int(z);
                out.bool(arrived);
            }
            Input::Wire(frame) => out.bytes(frame),
            Input::Camera(at, view, camera_type) => {
                at.iter().for_each(|v| out.double(*v));
                view.iter().for_each(|v| out.float(*v));
                out.0.push(camera_type);
            }
            Input::Animate(group) => {
                out.int(group.len() as i32);
                group.iter().flatten().for_each(|v| out.int(*v));
            }
            Input::Particles(level) => out.int(level),
            Input::ShellLayout(layout) => layout.iter().for_each(|v| out.int(*v)),
        }
    }
}

const SOUND_PLAY: u8 = 0;
const SOUND_UPDATE: u8 = 1;
const SOUND_STOP: u8 = 2;

/// SDL's keycode of a key by its vanilla name, with the layout's character when winit gave one.
fn sdl_keycode_of(name: &str, layout: Option<char>) -> i32 {
    let scancode = horizon_client::key_names::scancode_of(name).unwrap_or(0);
    horizon_client::key_names::sdl_keycode(scancode, layout)
}

/// SDL's numbering, left 1, middle 2, right 3.
fn button_number(button: MouseButton) -> i32 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Middle => 2,
        MouseButton::Right => 3,
        MouseButton::Back => 4,
        MouseButton::Forward => 5,
        MouseButton::Other(n) => i32::from(n) + 1,
    }
}

/// What the Java client last heard of the window's own state, so each is sent only when it changes.
#[derive(Default)]
struct Sent {
    size: Option<[i32; 3]>,
    native_screen: Option<(bool, bool, u8)>,
    layout: Option<[i32; 5]>,
    focused: Option<bool>,
    camera: Option<([i32; 3], [f32; 4], u8)>,
    cursor: Option<(f64, f64)>,
    motion: Option<Motion>,
    particles: Option<i32>,
}

#[derive(Clone, Copy, PartialEq)]
struct Motion {
    position: [f64; 3],
    look: [f32; 2],
    velocity: [f64; 3],
    on_ground: bool,
}

/// Writes whether `now` differs from what was last sent, and remembers it.
fn changed<T: PartialEq + Copy>(out: &mut Out, last: &mut Option<T>, now: T) -> bool {
    let changed = *last != Some(now);
    *last = Some(now);
    out.bool(changed);
    changed
}

static RUNNING: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

thread_local! {
    pub(in crate::game) static CLIENT: RefCell<Client> = RefCell::default();
    static WORKER: RefCell<Option<Worker>> = const { RefCell::new(None) };
}

/// One client tick's calls: the tick and the geometry its status says changed.
struct Job {
    remote: bool,
    tick: Vec<u8>,
    items: Option<(Vec<super::java_models::ItemKey>, Vec<u8>)>,
    decorations: Option<(Vec<Vec<u8>>, Vec<u8>)>,
    item_text: Option<(Vec<super::java_item_text::TextKey>, Vec<u8>)>,
}

#[derive(Default)]
struct Done {
    wire: Option<Vec<u8>>,
    tick: Option<Vec<u8>>,
    models: Option<Vec<u8>>,
    renders: Option<Vec<u8>>,
    items: Option<(Vec<super::java_models::ItemKey>, Vec<u8>)>,
    decorations: Option<(Vec<Vec<u8>>, Vec<u8>)>,
    item_text: Option<(Vec<super::java_item_text::TextKey>, Vec<u8>)>,
}

impl Job {
    fn run(self, host: &horizon_java_host::Host) -> Done {
        let Some(tick) = host.request("client_tick", &self.tick) else {
            return Done::default();
        };
        let items = self.items.and_then(|(keys, request)| {
            Some((keys, host.request("client_items_take", &request)?))
        });
        let decorations = self.decorations.and_then(|(keys, request)| {
            Some((keys, host.request("client_items_decorations", &request)?))
        });
        let item_text = self.item_text.and_then(|(keys, request)| {
            Some((keys, host.request("client_item_text", &request)?))
        });
        let wire = self
            .remote
            .then(|| host.request("client_wire_take", &[]))
            .flatten();
        let (models, renders) = status_pending(&tick).unwrap_or((0, 0));
        let take = |op, pending: i32| (pending > 0).then(|| host.request(op, &[])).flatten();
        Done {
            wire,
            models: take("client_models_take", models),
            renders: take("client_renders_take", renders),
            tick: Some(tick),
            items,
            decorations,
            item_text,
        }
    }
}

type JavaParticle = (
    [f64; 3],
    [f64; 3],
    Option<horizon_protocol::entity_data::Particle>,
);

/// A status's particle entries: where, how fast and the particle type's codec bytes, which `None` when they don't read.
fn read_particles(reply: &mut In) -> Option<Vec<JavaParticle>> {
    let mut found = Vec::new();
    for _ in 0..reply.int()? {
        let at = [reply.double()?, reply.double()?, reply.double()?];
        let velocity = [reply.double()?, reply.double()?, reply.double()?];
        let particle = horizon_protocol::entity_data::Particle::read(
            &mut horizon_protocol::buf::Reader::new(reply.bytes()?),
        )
        .ok();
        found.push((at, velocity, particle));
    }
    Some(found)
}

/// The HUD as the Java client recorded it: a segment before layer 0, then per layer its own content and the gap after it, the layers a mod cancelled and the ones Java drew.
struct HudRecording {
    segments: [Vec<u8>; HUD_SEGMENTS],
    cancelled: u32,
    drawn: u32,
}

impl Default for HudRecording {
    fn default() -> Self {
        Self {
            segments: std::array::from_fn(|_| Vec::new()),
            cancelled: 0,
            drawn: 0,
        }
    }
}

/// A HUD recording as the Java client writes it: `int n` segments, then `int cancelled`, `int drawn`.
fn read_hud(bytes: &[u8]) -> Option<HudRecording> {
    let mut reply = In(bytes);
    if reply.int()? != HUD_SEGMENTS as i32 {
        return None;
    }
    let mut hud = HudRecording::default();
    for segment in &mut hud.segments {
        *segment = reply.bytes()?.to_vec();
    }
    hud.cancelled = reply.int()? as u32;
    hud.drawn = reply.int()? as u32;
    Some(hud)
}

/// Whether native draws a vanilla layer and whether Java's own content for it is painted: a cancelled layer is neither, a layer Java drew is Java's.
fn hud_layer_draws(cancelled: u32, drawn: u32, layer: HudLayer) -> (bool, bool) {
    let bit = 1u32 << layer as u32;
    (cancelled & bit == 0 && drawn & bit == 0, drawn & bit != 0)
}

/// A shell's segments and the steps Java drew.
type Recording = ([Vec<u8>; SEGMENTS], u32);

/// A shell's segments as a status (`counted`) or a `ShellReply` writes them.
fn read_segments(reply: &mut In, counted: bool) -> Option<[Vec<u8>; SEGMENTS]> {
    if counted && reply.int()? != SEGMENTS as i32 {
        return None;
    }
    let mut segments: [Vec<u8>; SEGMENTS] = Default::default();
    for segment in &mut segments {
        *segment = reply.bytes()?.to_vec();
    }
    Some(segments)
}

/// A status's end: whether the creative tabs wait, the open shell kind, its fresh recording if any, the steps Java drew, whether it animates.
fn read_shell_tail(reply: &mut In) -> Option<(bool, i32, Option<Recording>, bool)> {
    let tabs_ready = reply.bool()?;
    let shell = reply.int()?;
    let segments = if reply.bool()? {
        Some(read_segments(reply, true)?)
    } else {
        None
    };
    let routed = reply.int()? as u32;
    Some((
        tabs_ready,
        shell,
        segments.map(|segments| (segments, routed)),
        reply.bool()?,
    ))
}

/// A `ShellReply`: who took the input, the fresh recording if any, whether it animates.
fn read_shell_reply(reply: &mut In) -> Option<(ShellOwner, Option<Recording>, bool)> {
    let owner = match reply.byte()? {
        0 => ShellOwner::Event,
        1 => ShellOwner::Widget,
        2 => ShellOwner::Native,
        _ => ShellOwner::None,
    };
    let segments = if reply.bool()? {
        Some(read_segments(reply, false)?)
    } else {
        None
    };
    let routed = reply.int()? as u32;
    let animating = reply.bool()?;
    Some((owner, segments.map(|s| (s, routed)), animating))
}

/// Whether native draws a step and whether Java's own content for it is painted, as the steps Java drew say.
fn leaf_draws(routed: u32, leaf: Leaf) -> (bool, bool) {
    let java = routed & (1 << leaf as u32) != 0;
    match leaf {
        Leaf::Widgets => (true, true),
        _ => (!java, java),
    }
}

/// True once per run, and only with `HORIZON_JAVA_DEBUG`.
fn debug_once() -> bool {
    static SEEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    std::env::var_os("HORIZON_JAVA_DEBUG").is_some()
        && !SEEN.swap(true, std::sync::atomic::Ordering::Relaxed)
}

/// The pending geometry and renderer counts, which lead a status reply's variable parts.
fn status_pending(status: &[u8]) -> Option<(i32, i32)> {
    let mut status = In(status);
    status.bool()?;
    status.int()?;
    status.bool()?;
    Some((status.int()?, status.int()?))
}

/// The per-frame recordings one frame asks for, in one request to the worker.
struct FrameJob {
    hud: Option<Vec<u8>>,
    world: Option<Vec<u8>>,
}

/// A frame request's replies, each with what the request took.
#[derive(Default)]
struct FrameDone {
    hud: Option<(Vec<u8>, u128)>,
    world: Option<(Vec<u8>, u128)>,
}

impl FrameJob {
    fn run(self, host: &horizon_java_host::Host) -> FrameDone {
        let record = |op, request: Option<Vec<u8>>| {
            let request = request?;
            let started = std::time::Instant::now();
            let reply = host.request(op, &request)?;
            Some((reply, started.elapsed().as_micros()))
        };
        FrameDone {
            hud: record("client_hud_frame", self.hud),
            world: record("client_world_frame", self.world),
        }
    }
}

enum Work {
    Tick(Job),
    Frame(FrameJob),
}

enum Finished {
    Tick(Done),
    Frame(FrameDone),
}

/// Orders the worker's jobs: one frame request in flight, none while a tick runs, and a recording older than an applied tick is dropped.
#[derive(Default)]
struct Sequence {
    next: u64,
    tick: Option<u64>,
    applied: u64,
    frame: Option<u64>,
}

impl Sequence {
    fn start_tick(&mut self) -> u64 {
        self.next += 1;
        self.tick = Some(self.next);
        self.next
    }

    /// The sequence number of a frame request, when none is in flight and no tick runs.
    fn start_frame(&mut self) -> Option<u64> {
        if self.tick.is_some() || self.frame.is_some() {
            return None;
        }
        self.next += 1;
        self.frame = Some(self.next);
        self.frame
    }

    fn finish_tick(&mut self, seq: u64) {
        self.tick = None;
        self.applied = self.applied.max(seq);
    }

    fn finish_frame(&mut self) {
        self.frame = None;
    }

    /// Whether a frame request's recording is newer than every applied tick's.
    fn fresh(&self, seq: u64) -> bool {
        seq > self.applied
    }

    fn in_flight(&self) -> bool {
        self.tick.is_some() || self.frame.is_some()
    }
}

/// What `Worker::collect` found finished: the tick, and the newest frame recording if no applied tick is newer.
#[derive(Default)]
struct Collected {
    tick: Option<Done>,
    frame: Option<FrameDone>,
}

/// Runs the Java client's ticks and frame recordings off the window's thread while no Java screen is open, as chunk meshing runs off it.
struct Worker {
    jobs: std::sync::mpsc::Sender<(u64, Work)>,
    done: std::sync::mpsc::Receiver<(u64, Finished)>,
    sequence: Sequence,
}

impl Worker {
    fn start() -> Self {
        let (jobs, queue) = std::sync::mpsc::channel::<(u64, Work)>();
        let (finished, done) = std::sync::mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("Java client".into())
            .spawn(move || {
                let Some(host) = horizon_java_host::get() else {
                    return;
                };
                for (seq, work) in queue {
                    let done = match work {
                        Work::Tick(job) => Finished::Tick(job.run(host)),
                        Work::Frame(job) => Finished::Frame(job.run(host)),
                    };
                    RUNNING.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
                    if finished.send((seq, done)).is_err() {
                        return;
                    }
                }
            });
        Self {
            jobs,
            done,
            sequence: Sequence::default(),
        }
    }

    fn submit(&self, seq: u64, work: Work) -> bool {
        RUNNING.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        let sent = self.jobs.send((seq, work)).is_ok();
        if !sent {
            RUNNING.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
        }
        sent
    }

    fn send(&mut self, job: Job) {
        let seq = self.sequence.start_tick();
        if !self.submit(seq, Work::Tick(job)) {
            self.sequence.tick = None;
        }
    }

    /// Sends one frame request unless one is in flight or a tick runs.
    fn send_frame(&mut self, job: FrameJob) {
        if let Some(seq) = self.sequence.start_frame()
            && !self.submit(seq, Work::Frame(job))
        {
            self.sequence.finish_frame();
        }
    }

    /// What finished, without waiting; `wait` blocks until nothing is in flight.
    fn collect(&mut self, wait: bool) -> Collected {
        let mut found = Collected::default();
        let mut frame = None;
        while self.sequence.in_flight() {
            let message = if wait {
                self.done.recv().map_err(|_| TryRecvError::Disconnected)
            } else {
                self.done.try_recv()
            };
            match message {
                Ok((seq, Finished::Tick(done))) => {
                    self.sequence.finish_tick(seq);
                    found.tick = Some(done);
                }
                Ok((seq, Finished::Frame(done))) => {
                    self.sequence.finish_frame();
                    frame = Some((seq, done));
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.sequence.tick = None;
                    self.sequence.frame = None;
                }
            }
        }
        found.frame = frame
            .filter(|(seq, _)| self.sequence.fresh(*seq))
            .map(|(_, done)| done);
        found
    }
}

/// Whether a background client tick is running, during which the window doesn't wait on Java.
pub(in crate::game) fn java_client_running() -> bool {
    RUNNING.load(std::sync::atomic::Ordering::Acquire) > 0
}

/// A background tick was sent and its results not yet collected.
pub(in crate::game) fn java_client_busy() -> bool {
    WORKER.with_borrow(|w| w.as_ref().is_some_and(|w| w.sequence.tick.is_some()))
}

/// Changes whenever a block entity's client copy was made, dropped or updated, so cached shapes may be stale.
pub(in crate::game) fn java_shape_generation() -> i32 {
    CLIENT.with_borrow(|c| c.shapes)
}

impl Game<'_> {
    pub(in crate::game) fn java_client_window(&self) -> [i32; 3] {
        let size = self
            .renderer
            .as_ref()
            .map_or([854, 480], |r| [r.config.width, r.config.height]);
        let batch = HudBatch::new(size, self.settings.gui_scale_inputs());
        [size[0] as i32, size[1] as i32, batch.scale as i32]
    }

    fn java_client_size(&self, out: &mut Out) {
        for value in self.java_client_window() {
            out.int(value);
        }
    }

    fn java_client_status(&mut self, reply: &mut In) -> Option<()> {
        let open = reply.bool()?;
        let _opened = reply.int()?;
        let _pause = reply.bool()?;
        let _pending = (reply.int()?, reply.int()?);
        for _ in 0..reply.int()? {
            let column = (reply.int()?, reply.int()?);
            CLIENT.with_borrow_mut(|c| c.told.push(column));
        }
        self.java_client_sounds(reply)?;
        self.java_client_particles(reply)?;
        let shapes = reply.int()?;
        let hud = reply
            .bool()?
            .then(|| reply.bytes())
            .flatten()
            .map(<[u8]>::to_vec);
        let animating = reply.bool()?;
        let world = reply
            .bool()?
            .then(|| reply.bytes())
            .flatten()
            .map(<[u8]>::to_vec);
        let world_animating = reply.bool()?;
        let (tabs_ready, shell, segments, shell_animating) = read_shell_tail(reply)?;
        super::java_world_renders::java_world_status(world.as_deref(), world_animating);
        let refused = CLIENT.with_borrow_mut(|c| {
            c.hud_animating = animating;
            c.recorded = true;
            c.shapes = shapes;
            c.open = open;
            if let Some(hud) = hud {
                c.hud = read_hud(&hud).unwrap_or_default();
            }
            if (shell == 1) != c.shell.open && std::env::var_os("HORIZON_JAVA_DEBUG").is_some()
            {
                let sizes = segments.as_ref().map(|s| s.0.each_ref().map(Vec::len));
                log::info!(
                    "Horizon: Java shell 1 {} {sizes:?}",
                    if shell == 1 { "opened" } else { "closed" }
                );
            }
            c.shell.open = shell == 1;
            c.shell.opening = false;
            c.shell.animating = shell_animating;
            if let Some((segments, routed)) = segments {
                c.shell.segments = segments;
                c.shell.routed = routed;
                c.shell.fresh = true;
            }
            shell == 0 && matches!(c.sent.native_screen, Some((true, _, 1)))
        });
        if tabs_ready {
            self.java_creative_tabs_built();
        }
        if open && self.screen != Screen::Java {
            self.screen = Screen::Java;
            self.capture(false);
        } else if !open && self.screen == Screen::Java {
            // A mod's screen over a shell closed back to it: native's screen is the shell's again.
            if shell == 1 {
                self.screen = Screen::Inventory;
                self.capture(false);
            } else {
                self.screen = Screen::Playing;
                self.capture(true);
            }
        } else if refused && self.screen == Screen::Inventory && debug_once() {
            log::info!(
                "Horizon: no shell opened for the inventory; native's screen stays alone"
            );
        }
        Some(())
    }

    /// What the Java client's sound engine did since the last call: Horizon's engine plays, changes and stops those sounds.
    fn java_client_sounds(&mut self, reply: &mut In) -> Option<()> {
        for _ in 0..reply.int()? {
            let kind = reply.byte()?;
            let handle = reply.int()?;
            match kind {
                SOUND_PLAY => {
                    let sound = horizon_client::audio::ExternalSound {
                        event: reply.text()?.to_owned(),
                        source: usize::try_from(reply.int()?).ok()?,
                        volume: reply.float()?,
                        pitch: reply.float()?,
                        position: [reply.double()?, reply.double()?, reply.double()?],
                        looping: reply.bool()?,
                        delay: reply.int()?,
                        linear: reply.bool()?,
                        relative: reply.bool()?,
                        can_start_silent: reply.bool()?,
                        tickable: reply.bool()?,
                    };
                    let id = self.audio.play_external(&sound);
                    CLIENT.with_borrow_mut(|c| c.sounds.insert(handle, id));
                }
                SOUND_UPDATE => {
                    let (volume, pitch) = (reply.float()?, reply.float()?);
                    let at = [reply.double()?, reply.double()?, reply.double()?];
                    if let Some(id) = CLIENT.with_borrow(|c| c.sounds.get(&handle).copied()) {
                        self.audio.update_external(id, volume, pitch, at);
                    }
                }
                SOUND_STOP => {
                    if let Some(id) = CLIENT.with_borrow_mut(|c| c.sounds.remove(&handle)) {
                        self.audio.stop_and_release(id);
                    }
                }
                _ => return None,
            }
        }
        Some(())
    }

    /// The particles the Java client's `animateTick` made, which Horizon's engine draws: the limiter already ran in Java.
    fn java_client_particles(&mut self, reply: &mut In) -> Option<()> {
        for (at, velocity, particle) in read_particles(reply)? {
            let Some(options) = particle
                .as_ref()
                .and_then(|particle| self.received_particle(particle, at))
            else {
                let id = particle.map_or(-1, |particle| particle.id);
                thread_local! { static UNKNOWN: RefCell<std::collections::HashSet<i32>> = RefCell::default(); }
                if UNKNOWN.with_borrow_mut(|seen| seen.insert(id)) {
                    let name = super::java_particles::java_particle_name(id)
                        .unwrap_or("?");
                    log::info!(
                        "Horizon: Java particle {name} ({id}) reached native, which can't draw it"
                    );
                }
                continue;
            };
            self.particles.add_with(&options, at, velocity, true, true);
        }
        Some(())
    }

    /// The Java client's sounds Horizon's engine finished, as its channels report them stopped.
    fn java_client_finished_sounds(&mut self, out: &mut Out) {
        let finished: Vec<(i32, horizon_client::sound_engine::InstanceId)> = CLIENT
            .with_borrow(|c| {
                c.sounds
                    .iter()
                    .filter(|(_, id)| !self.audio.external_playing(**id))
                    .map(|(handle, id)| (*handle, *id))
                    .collect()
            });
        out.int(finished.len() as i32);
        for (handle, id) in finished {
            out.int(handle);
            self.audio.stop_and_release(id);
            CLIENT.with_borrow_mut(|c| c.sounds.remove(&handle));
        }
    }

    /// The window joined a server in another process whose Java side the Java client connected to in configuration: its play packets go over the window's connection.
    pub(in crate::game) fn java_remote_joined(&mut self, java: bool) {
        if !java {
            return;
        }
        CLIENT.with_borrow_mut(|c| c.remote = true);
        self.java_bind_states();
    }

    /// A frame of the server's Java side, which the Java client reads at its next tick.
    pub(in crate::game) fn java_wire_in(&mut self, frame: &[u8]) {
        if CLIENT.with_borrow(|c| c.remote) {
            self.java_queue(Input::Wire(frame));
        }
    }

    /// Horizon's font's advances, which the Java client measures text with: all of them, ready for its start.
    pub(in crate::game) fn java_client_font(&self) {
        if std::env::var_os("HORIZON_JAVA_RUN").is_none()
            || CLIENT.with_borrow_mut(|c| std::mem::replace(&mut c.font, true))
        {
            return;
        }
        let advances = trimmed_advances(self.gui.atlas.font.advances());
        let mut out = Out::default();
        out.text(&self.settings.language);
        out.int(advances.len() as i32);
        for advance in advances {
            out.float(advance);
        }
        super::java_components::prepare_font(out.0);
    }

    /// `Minecraft.disconnect`, once the window has left its server: the Java client ends with no level and no screen, ready for the next join.
    pub(in crate::game) fn java_client_left(&mut self) {
        if !CLIENT.with_borrow(|c| c.joined || c.remote) {
            return;
        }
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        self.java_client_collect(true);
        host.request("client_leave", &[]);
        self.java_creative_tabs_left();
        horizon_client::dynamic_models::clear();
        horizon_client::dynamic_renders::clear();
        super::java_world_renders::java_world_left();
        CLIENT.with_borrow_mut(|c| {
            c.joined = false;
            c.remote = false;
            c.open = false;
            c.states = false;
            c.armed = false;
            c.told.clear();
            c.hud = HudRecording::default();
            c.hud_animating = false;
            c.shell = Shell::default();
            c.owed = 0;
            c.animate.clear();
            c.picked.clear();
            c.sent = Sent::default();
            c.queue.clear();
            c.queued = 0;
        });
        for (_, id) in CLIENT.with_borrow_mut(|c| std::mem::take(&mut c.sounds)) {
            self.audio.stop_and_release(id);
        }
        super::java_bridges::client_world::java_forget_client_world();
    }

    /// The start of the window's tick: the Java client's last tick results are applied, before the window's tick reads what they changed.
    pub(in crate::game) fn java_client_tick_begin(&mut self) {
        if self.remote_profile().is_none() || horizon_java_host::get().is_none() {
            return;
        }
        let open = CLIENT.with_borrow(|c| c.open);
        // A screen ticks in step with the window, so its tick waits for any background one first.
        self.java_client_collect(open);
    }

    /// The picks of this tick's `animateTick`, which the tick's end sends.
    pub(in crate::game) fn java_set_animate_picked(&mut self, picked: Vec<[i32; 3]>) {
        thread_local! { static SEEN: RefCell<std::collections::HashSet<String>> = RefCell::default(); }
        for at in &picked {
            let state = self.world.block(at[0], at[1], at[2]);
            if let Some(state) = self.pack.assets.registry.state(state)
                && !SEEN.with_borrow(|seen| seen.contains(&*state.name))
            {
                SEEN.with_borrow_mut(|seen| seen.insert(state.name.to_string()));
                log::info!("Horizon: animateTick of {} runs in Java", state.name);
            }
        }
        CLIENT.with_borrow_mut(|c| c.picked = picked);
    }

    /// `Minecraft.runTick` for the Java client, once the window's tick has run: what the server sent it and the tick's picks, then the real `Minecraft.tick`.
    pub(in crate::game) fn java_client_tick_send(&mut self) {
        if self.remote_profile().is_none() {
            return;
        }
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        // `Timer.advanceTime`: a tick the last one's length delayed still runs, up to its limit of 10.
        CLIENT.with_borrow_mut(|c| {
            c.owed = (c.owed + 1).min(10);
            let group = std::mem::take(&mut c.picked);
            c.animate.push_back(group);
            while c.animate.len() > 10 {
                c.animate.pop_front();
            }
        });
        if java_client_busy() {
            return;
        }
        self.java_client_collisions();
        let mut out = Out::default();
        out.int(CLIENT.with_borrow_mut(|c| std::mem::take(&mut c.owed)));
        out.float(self.clock.fraction() as f32);
        let window = self.java_client_window();
        if CLIENT.with_borrow_mut(|c| changed(&mut out, &mut c.sent.size, window)) {
            for value in window {
                out.int(value);
            }
        }
        let open = !self.java_world_has_input() && self.screen != Screen::Java;
        let kind = if open { self.java_shell_kind() } else { 0 };
        let native_screen = (open, open && self.pause, kind);
        let layout = (kind != 0).then(|| self.java_shell_layout());
        let camera = self.java_world_camera();
        let camera_type = self.camera_type as u8;
        self.java_publish_client_world();
        let (left, arrived) = self.java_client_chunk_changes();
        CLIENT.with_borrow_mut(|c| {
            let mut events = Out(std::mem::take(&mut c.queue));
            let mut count = std::mem::take(&mut c.queued);
            for (x, z) in &left {
                Input::Chunk(*x, *z, false).write(&mut events);
                count += 1;
            }
            for (x, z) in &arrived {
                Input::Chunk(*x, *z, true).write(&mut events);
                count += 1;
            }
            if let Some(layout) = layout
                && c.sent.layout != Some(layout)
            {
                c.sent.layout = Some(layout);
                Input::ShellLayout(layout).write(&mut events);
                count += 1;
            }
            if c.sent.native_screen != Some(native_screen) {
                if kind != 0 && c.sent.native_screen.is_none_or(|sent| sent.2 != kind) {
                    c.shell.opening = true;
                }
                c.sent.native_screen = Some(native_screen);
                Input::Screen(native_screen.0, native_screen.1, native_screen.2)
                    .write(&mut events);
                count += 1;
            }
            // A window starts focused; an offscreen one never hears otherwise.
            let focused = c.focused.unwrap_or(true);
            if c.sent.focused != Some(focused) {
                c.sent.focused = Some(focused);
                Input::Focus(focused).write(&mut events);
                count += 1;
            }
            let particles = i32::from(self.settings.particles);
            if c.sent.particles != Some(particles) {
                c.sent.particles = Some(particles);
                Input::Particles(particles).write(&mut events);
                count += 1;
            }
            let block = camera.map(|(p, _)| p.map(|v| v.floor() as i32));
            if let (Some((at, view)), Some(block)) = (camera, block)
                && c.sent.camera != Some((block, view, camera_type))
            {
                c.sent.camera = Some((block, view, camera_type));
                Input::Camera(at, view, camera_type).write(&mut events);
                count += 1;
            }
            for group in c.animate.drain(..) {
                Input::Animate(&group).write(&mut events);
                count += 1;
            }
            out.int(count);
            out.0.extend_from_slice(&events.0);
        });
        // The window's movement code is this player's motion authority, as `LocalPlayer.aiStep` is a real client's.
        let motion = self.player.as_ref().map(|p| Motion {
            position: [p.position.x, p.position.y, p.position.z],
            look: [self.yaw, self.pitch],
            velocity: [p.velocity.x, p.velocity.y, p.velocity.z],
            on_ground: p.ground.on_ground,
        });
        let moved = motion.filter(|m| CLIENT.with_borrow_mut(|c| c.sent.motion != Some(*m)));
        out.bool(moved.is_some());
        if let Some(m) = moved {
            CLIENT.with_borrow_mut(|c| c.sent.motion = Some(m));
            for value in m.position {
                out.double(value);
            }
            out.float(m.look[0]);
            out.float(m.look[1]);
            for value in m.velocity {
                out.double(value);
            }
            out.bool(m.on_ground);
        }
        self.java_client_finished_sounds(&mut out);
        let job = Job {
            items: super::java_models::java_items_request(),
            decorations: super::java_models::java_decorations_request(),
            item_text: super::java_item_text::request(),
            remote: CLIENT.with_borrow(|c| c.remote),
            tick: out.0,
        };
        if open {
            self.java_client_collect(true);
            let done = job.run(host);
            self.java_client_apply(done);
        } else {
            WORKER.with_borrow_mut(|w| w.get_or_insert_with(Worker::start).send(job));
        }
    }

    /// The background tick's results, once it finished; `wait` blocks until it has.
    fn java_client_collect(&mut self, wait: bool) {
        let found = WORKER.with_borrow_mut(|w| w.as_mut().map(|w| w.collect(wait)));
        let Some(found) = found else {
            return;
        };
        if let Some(done) = found.tick {
            self.java_client_apply(done);
        }
        if let Some(frame) = found.frame {
            if let Some((reply, spent)) = frame.hud {
                java_hud_frame_apply(&reply, spent);
            }
            if let Some((reply, spent)) = frame.world {
                super::java_world_renders::java_world_frame_apply(&reply, spent);
            }
        }
    }

    fn java_client_apply(&mut self, done: Done) {
        if let Some(wire) = &done.wire
            && let Some(r) = &self.remote
        {
            let mut frames = In(wire);
            for _ in 0..frames.int().unwrap_or(0) {
                let Some(frame) = frames.bytes() else {
                    break;
                };
                r.session.send_java(frame.to_vec());
            }
        }
        let Some(tick) = done.tick else {
            // The Java client failed: no answer is coming for the columns waiting on it.
            horizon_client::dynamic_models::set_answering(false);
            super::java_item_text::forget_pending();
            CLIENT.with_borrow_mut(|c| c.armed = false);
            return;
        };
        if !CLIENT.with_borrow_mut(|c| std::mem::replace(&mut c.joined, true)) {
            self.java_apply_mod_keys();
        }
        // A status answers once the Java client's login is done: until then nothing is held for it.
        if !tick.is_empty() && !CLIENT.with_borrow(|c| c.armed) {
            CLIENT.with_borrow_mut(|c| c.armed = true);
            self.java_client_arm();
        }
        self.java_client_status(&mut In(&tick));
        if !CLIENT.with_borrow(|c| c.states) {
            self.java_client_resources();
            CLIENT.with_borrow_mut(|c| c.states = true);
        }
        if let Some((keys, reply)) = done.items {
            super::java_models::java_items_apply(keys, &reply, self.world.game_time);
        }
        if let Some((keys, reply)) = done.decorations {
            super::java_models::java_decorations_apply(keys, &reply);
        }
        if let Some((keys, reply)) = done.item_text {
            super::java_item_text::apply(keys, &reply);
        }
        if let Some(models) = done.models {
            self.java_models_apply(&models);
        }
        if let Some(renders) = done.renders {
            self.java_renders_apply(&renders);
        }
        let told = CLIENT.with_borrow_mut(|c| std::mem::take(&mut c.told));
        horizon_client::dynamic_models::answered(told);
    }

}

/// The advance Java's font answers for a code point past the table's end.
const JAVA_MISSING_ADVANCE: f32 = 6.0;

/// The table up to the last code point whose advance Java wouldn't answer by itself.
fn trimmed_advances(mut advances: Vec<f32>) -> Vec<f32> {
    let keep = advances
        .iter()
        .rposition(|a| *a != JAVA_MISSING_ADVANCE)
        .map_or(0, |last| last + 1);
    advances.truncate(keep);
    advances
}

/// `Direction.get3DDataValue` of a face's normal.
fn face_index(face: [i32; 3]) -> i32 {
    match face {
        [0, -1, 0] => 0,
        [0, 1, 0] => 1,
        [0, 0, -1] => 2,
        [0, 0, 1] => 3,
        [-1, 0, 0] => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_font_table_ends_at_the_last_code_point_java_cannot_guess() {
        let mut table = vec![6.0; 0x11_0000];
        table[0x41] = 5.0;
        table[0xFFFD] = 7.0;
        let trimmed = super::trimmed_advances(table);
        assert_eq!(trimmed.len(), 0xFFFE);
        assert_eq!(trimmed[0xFFFD], 7.0);
        assert!(super::trimmed_advances(vec![6.0; 8]).is_empty());
    }

    #[test]
    fn a_key_input_lays_out_name_action_modifiers_keycode() {
        let mut out = Out::default();
        Input::Key(
            "key.keyboard.e",
            1,
            64,
            sdl_keycode_of("key.keyboard.e", None),
        )
        .write(&mut out);
        let mut read = In(&out.0[1..]);
        assert_eq!(out.0[0], 0);
        assert_eq!(read.text(), Some("key.keyboard.e"));
        assert_eq!(
            (read.int(), read.int(), read.int()),
            (Some(1), Some(64), Some(101))
        );
    }

    #[test]
    fn a_camera_input_carries_the_perspective_after_the_seven_numbers() {
        let mut out = Out::default();
        Input::Camera([1.0, 2.0, 3.0], [4.0, 5.0, 6.0, 7.0], 2).write(&mut out);
        assert_eq!(out.0[0], 13);
        assert_eq!(out.0.len(), 1 + 3 * 8 + 4 * 4 + 1);
        assert_eq!(out.0[41], 2);
    }

    fn segments_of(sizes: &[(usize, u8)]) -> Out {
        let mut out = Out::default();
        for i in 0..SEGMENTS {
            match sizes.iter().find(|(at, _)| *at == i) {
                Some((_, n)) => out.bytes(&vec![0u8; usize::from(*n)]),
                None => out.bytes(&[]),
            }
        }
        out
    }

    #[test]
    fn a_status_with_a_shell_reads_its_kind_segments_and_routed_back() {
        let mut out = Out::default();
        out.bool(true);
        out.int(1);
        out.bool(true);
        out.int(SEGMENTS as i32);
        out.0.extend(segments_of(&[(1, 2), (7, 1)]).0);
        out.int(1 << 10);
        out.bool(true);
        let (tabs, shell, recording, animating) = read_shell_tail(&mut In(&out.0)).unwrap();
        assert!(tabs && animating);
        assert_eq!(shell, 1);
        let (segments, routed) = recording.unwrap();
        assert_eq!(routed, 1 << Leaf::Tooltip as u32);
        let sizes = segments.each_ref().map(Vec::len);
        assert_eq!((sizes[1], sizes[7], sizes[0]), (2, 1, 0));
        let mut none = Out::default();
        none.bool(false);
        none.int(0);
        none.bool(false);
        none.int(0);
        none.bool(false);
        let (_, shell, recording, animating) = read_shell_tail(&mut In(&none.0)).unwrap();
        assert!(shell == 0 && recording.is_none() && !animating);
    }

    #[test]
    fn a_status_with_the_old_segment_count_is_refused() {
        let mut out = Out::default();
        out.bool(false);
        out.int(1);
        out.bool(true);
        out.int(5);
        assert!(read_shell_tail(&mut In(&out.0)).is_none());
    }

    #[test]
    fn a_shell_reply_has_no_segment_count_and_may_carry_no_recording() {
        let mut out = Out::default();
        out.0.push(2);
        out.bool(true);
        out.0.extend(segments_of(&[(0, 1), (27, 2)]).0);
        out.int(0);
        out.bool(false);
        let (owner, recording, animating) = read_shell_reply(&mut In(&out.0)).unwrap();
        assert_eq!((owner, animating), (ShellOwner::Native, false));
        let sizes = recording.unwrap().0.each_ref().map(Vec::len);
        assert_eq!((sizes[0], sizes[27]), (1, 2));
        let mut quiet = Out::default();
        quiet.0.push(3);
        quiet.bool(false);
        quiet.int(0);
        quiet.bool(true);
        let (owner, recording, animating) = read_shell_reply(&mut In(&quiet.0)).unwrap();
        assert!(owner == ShellOwner::None && recording.is_none() && animating);
    }

    fn hud_blob(filled: &[(usize, u8)], cancelled: i32, drawn: i32) -> Out {
        let mut out = Out::default();
        out.int(HUD_SEGMENTS as i32);
        out.0.extend(
            {
                let mut all = Out::default();
                for i in 0..HUD_SEGMENTS {
                    match filled.iter().find(|(at, _)| *at == i) {
                        Some((_, n)) => all.bytes(&vec![0u8; usize::from(*n)]),
                        None => all.bytes(&[]),
                    }
                }
                all
            }
            .0,
        );
        out.int(cancelled);
        out.int(drawn);
        out
    }

    #[test]
    fn the_49_segment_hud_blob_and_its_two_masks_read_back() {
        let blob = hud_blob(&[(4, 3), (9, 5)], 1 << 5, 1 << 4);
        let hud = read_hud(&blob.0).unwrap();
        let sizes = hud.segments.each_ref().map(Vec::len);
        assert_eq!((sizes[4], sizes[9], sizes[0]), (3, 5, 0));
        assert_eq!(hud.cancelled, 1 << HudLayer::ArmorLevel as u32);
        assert_eq!(hud.drawn, 1 << HudLayer::PlayerHealth as u32);
        let mut old = Out::default();
        old.int(5);
        assert!(read_hud(&old.0).is_none());
    }

    #[test]
    fn a_cancelled_layer_skips_native_and_a_drawn_one_is_javas() {
        let (cancelled, drawn) = (
            1 << HudLayer::FoodLevel as u32,
            1 << HudLayer::Hotbar as u32,
        );
        assert_eq!(
            hud_layer_draws(cancelled, drawn, HudLayer::FoodLevel),
            (false, false)
        );
        assert_eq!(
            hud_layer_draws(cancelled, drawn, HudLayer::Hotbar),
            (false, true)
        );
        assert_eq!(
            hud_layer_draws(cancelled, drawn, HudLayer::Chat),
            (true, false)
        );
    }

    #[test]
    fn a_step_java_drew_is_not_drawn_by_native_and_widgets_are_both() {
        let routed = 1 << Leaf::Tooltip as u32 | 1 << Leaf::Widgets as u32;
        assert_eq!(leaf_draws(routed, Leaf::Tooltip), (false, true));
        assert_eq!(leaf_draws(routed, Leaf::Slots), (true, false));
        assert_eq!(leaf_draws(0, Leaf::Widgets), (true, true));
    }

    #[test]
    fn the_shell_inputs_write_what_events_reads() {
        let mut out = Out::default();
        Input::Screen(true, false, 1).write(&mut out);
        Input::ShellLayout([76, 83, 176, 166, -1]).write(&mut out);
        let mut read = In(&out.0);
        assert_eq!(read.byte(), Some(6));
        assert_eq!(
            (read.bool(), read.bool(), read.byte()),
            (Some(true), Some(false), Some(1))
        );
        assert_eq!(read.byte(), Some(16));
        let layout: Vec<i32> = (0..5).map(|_| read.int().unwrap()).collect();
        assert_eq!(layout, [76, 83, 176, 166, -1]);
        assert!(read.byte().is_none());
    }

    #[test]
    fn the_animate_picks_are_a_count_then_their_positions() {
        let mut out = Out::default();
        Input::Animate(&[[1, 2, 3], [-4, 5, 6]]).write(&mut out);
        let mut read = In(&out.0);
        assert_eq!(read.byte(), Some(14));
        assert_eq!(read.int(), Some(2));
        assert_eq!(
            (0..6).map(|_| read.int().unwrap()).collect::<Vec<_>>(),
            [1, 2, 3, -4, 5, 6]
        );
        assert!(read.byte().is_none());
    }

    #[test]
    fn a_status_with_a_known_and_an_unknown_particle_reads_both_in_order() {
        let entry = |out: &mut Out, id: u8| {
            for v in [1.5, 2.5, 3.5, 0.0, 0.1, 0.0] {
                out.double(v);
            }
            out.bytes(&[id]);
        };
        let mut out = Out::default();
        out.int(2);
        entry(&mut out, 4);
        entry(&mut out, 0x7f);
        let found = read_particles(&mut In(&out.0)).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].0, [1.5, 2.5, 3.5]);
        assert_eq!(found[0].1, [0.0, 0.1, 0.0]);
        assert_eq!(found[0].2.as_ref().map(|p| p.id), Some(4));
        assert_eq!(found[1].2.as_ref().map(|p| p.id), Some(0x7f));
        let mut short = Out::default();
        short.int(1);
        assert!(read_particles(&mut In(&short.0)).is_none());
    }

    use super::*;

    fn atlas() -> HudAtlas {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("horizon-java-hud-{}-{n}", std::process::id()));
        let gui = root.join("assets/minecraft/textures/gui");
        std::fs::create_dir_all(&gui).unwrap();
        let file = std::fs::File::create(gui.join("x.png")).unwrap();
        let mut encoder = png::Encoder::new(file, 64, 32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[255; 64 * 32 * 4])
            .unwrap();
        HudAtlas::build(&horizon_assets::resource_source::ResourceSource::directory(
            root,
        ))
        .unwrap()
    }

    fn sprite_quad(wrap: bool, width: f32, uv: [f32; 4]) -> Vec<u8> {
        let mut out = Out::default();
        out.int(1);
        out.0.push(QUAD);
        out.text("minecraft:textures/gui/x.png");
        out.0.push(0);
        out.0.push(u8::from(wrap));
        let [u0, v0, u1, v1] = uv;
        for (x, y, u, v) in [
            (0.0, 0.0, u0, v0),
            (0.0, 32.0, u0, v1),
            (width, 32.0, u1, v1),
            (width, 0.0, u1, v0),
        ] {
            for f in [x, y, u, v] {
                out.float(f);
            }
            out.int(-1);
        }
        out.bool(false);
        out.0
    }

    fn painted(frame: &[u8]) -> (Vec<HudVertex>, HudAtlas) {
        let atlas = atlas();
        let mut batch = HudBatch::scratch([854, 480]);
        assert!(paint_frame(&mut batch, &atlas, frame, |_, _| Some(())).is_some());
        (batch.vertices, atlas)
    }

    fn white_quad(out: &mut Out, x: f32, y: f32, size: f32) {
        out.0.push(QUAD);
        out.text("");
        out.0.push(0);
        out.0.push(0);
        for (cx, cy) in [(x, y), (x, y + size), (x + size, y + size), (x + size, y)] {
            for v in [cx, cy, 0.0, 0.0] {
                out.float(v);
            }
            out.int(0x80ff_ffffu32 as i32);
        }
        out.bool(false);
    }

    #[test]
    fn one_frame_request_is_in_flight_and_none_while_a_tick_runs() {
        let mut q = Sequence::default();
        let first = q.start_frame().unwrap();
        assert!(q.start_frame().is_none());
        q.finish_frame();
        let second = q.start_frame().unwrap();
        assert!(second > first);
        q.finish_frame();
        q.start_tick();
        assert!(q.start_frame().is_none());
    }

    #[test]
    fn a_frame_older_than_an_applied_tick_is_dropped_and_a_newer_one_kept() {
        let mut q = Sequence::default();
        let old = q.start_frame().unwrap();
        q.finish_frame();
        let tick = q.start_tick();
        q.finish_tick(tick);
        assert!(!q.fresh(old));
        let new = q.start_frame().unwrap();
        assert!(q.fresh(new));
    }

    #[test]
    fn sequence_numbers_follow_the_order_of_sending() {
        let mut q = Sequence::default();
        let frame = q.start_frame().unwrap();
        q.finish_frame();
        let tick = q.start_tick();
        assert!(frame < tick);
        assert!(q.in_flight());
        q.finish_tick(tick);
        assert!(!q.in_flight());
    }

    #[test]
    fn a_frame_that_took_a_tick_status_records_nothing_and_the_next_does() {
        assert!(!hud_frame_wanted(true, true, false));
        assert!(hud_frame_wanted(true, false, false));
        assert!(!hud_frame_wanted(true, false, true));
        assert!(!hud_frame_wanted(false, false, false));
    }

    #[test]
    fn a_recorded_hud_paints_in_call_order_and_stops_at_a_broken_entry() {
        let atlas = atlas();
        let mut out = Out::default();
        out.int(2);
        white_quad(&mut out, 10.0, 20.0, 4.0);
        white_quad(&mut out, 30.0, 40.0, 8.0);
        let mut batch = HudBatch::scratch([854, 480]);
        assert!(paint_frame(&mut batch, &atlas, &out.0, |_, _| Some(())).is_some());
        assert_eq!(batch.vertices.len(), 12);
        assert_eq!(batch.vertices[0].pos, [10.0, 20.0]);
        assert_eq!(batch.vertices[6].pos, [30.0, 40.0]);
        assert_eq!(batch.vertices[8].pos, [38.0, 48.0]);

        let mut cut = Out::default();
        cut.int(2);
        white_quad(&mut cut, 1.0, 2.0, 3.0);
        cut.0.push(QUAD);
        let mut batch = HudBatch::scratch([854, 480]);
        assert!(paint_frame(&mut batch, &atlas, &cut.0, |_, _| Some(())).is_none());
        assert_eq!(batch.vertices.len(), 6);

        let mut batch = HudBatch::scratch([854, 480]);
        assert!(paint_frame(&mut batch, &atlas, &Out::default().0, |_, _| None).is_some());
        assert!(batch.vertices.is_empty());
    }

    #[test]
    fn a_repeating_quad_splits_into_a_cell_per_tile() {
        let (v, atlas) = painted(&sprite_quad(true, 64.0, [0.0, 0.0, 2.0, 1.0]));
        let r = atlas.sprites["gui/x"];
        assert_eq!(v.len(), 12);
        for (cell, x) in [(0, [0.0, 32.0]), (6, [32.0, 64.0])] {
            assert_eq!(v[cell].pos, [x[0], 0.0]);
            assert_eq!(v[cell + 2].pos, [x[1], 32.0]);
            assert_eq!(v[cell].uv, [r.u0, r.v0]);
            assert!((v[cell + 2].uv[0] - r.u1).abs() < 1e-6);
            assert!((v[cell + 2].uv[1] - r.v1).abs() < 1e-6);
        }
    }

    #[test]
    fn a_partial_last_tile_ends_part_way_through_the_sprite() {
        let (v, atlas) = painted(&sprite_quad(true, 48.0, [0.0, 0.0, 1.5, 1.0]));
        let r = atlas.sprites["gui/x"];
        assert_eq!(v.len(), 12);
        assert_eq!(v[6].pos[0], 32.0);
        assert_eq!(v[8].pos[0], 48.0);
        assert_eq!(v[6].uv[0], r.u0);
        assert!((v[8].uv[0] - (r.u0 + 0.5 * (r.u1 - r.u0))).abs() < 1e-6);
    }

    #[test]
    fn a_negative_range_splits_at_zero() {
        let (v, atlas) = painted(&sprite_quad(true, 64.0, [-0.5, 0.0, 0.5, 1.0]));
        let r = atlas.sprites["gui/x"];
        assert_eq!(v.len(), 12);
        assert_eq!(v[0].pos[0], 0.0);
        assert_eq!(v[2].pos[0], 32.0);
        assert_eq!(v[8].pos[0], 64.0);
        let half = r.u0 + 0.5 * (r.u1 - r.u0);
        assert!((v[0].uv[0] - half).abs() < 1e-6);
        assert!((v[2].uv[0] - r.u1).abs() < 1e-6);
        assert!((v[6].uv[0] - r.u0).abs() < 1e-6);
        assert!((v[8].uv[0] - half).abs() < 1e-6);
    }

    #[test]
    fn a_clamped_or_in_range_quad_stays_one() {
        let (v, _) = painted(&sprite_quad(false, 64.0, [0.0, 0.0, 2.0, 1.0]));
        assert_eq!(v.len(), 6);
        let (v, _) = painted(&sprite_quad(true, 64.0, [0.0, 0.0, 1.0, 1.0]));
        assert_eq!(v.len(), 6);
    }

    fn triangle(clip: Option<[i32; 4]>) -> Vec<u8> {
        let mut out = Out::default();
        out.int(1);
        out.0.push(QUAD);
        out.text("");
        out.0.push(0);
        out.0.push(0);
        for (x, y) in [(0.0, 0.0), (20.0, 0.0), (0.0, 20.0), (0.0, 20.0)] {
            for f in [x, y, 0.0, 0.0] {
                out.float(f);
            }
            out.int(0x80ff_ffffu32 as i32);
        }
        out.bool(clip.is_some());
        for v in clip.into_iter().flatten() {
            out.int(v);
        }
        out.0
    }

    #[test]
    fn a_triangle_recorded_as_a_degenerate_quad_draws_once() {
        let (v, _) = painted(&triangle(None));
        assert_eq!(v.len(), 6);
        let tri = |a: usize, b: usize, c: usize| [v[a].pos, v[b].pos, v[c].pos];
        assert_eq!(tri(0, 1, 2), [[0.0, 0.0], [20.0, 0.0], [0.0, 20.0]]);
        assert_eq!(tri(3, 4, 5), [[0.0, 0.0], [0.0, 20.0], [0.0, 20.0]]);
    }

    #[test]
    fn a_scissor_that_cuts_a_triangle_keeps_its_visible_part() {
        let (v, _) = painted(&triangle(Some([0, 0, 10, 10])));
        assert!(!v.is_empty());
        assert!(v.iter().all(|p| p.pos[0] <= 10.0 && p.pos[1] <= 10.0));
    }

    #[test]
    fn a_blur_entry_is_reported() {
        let atlas = atlas();
        let mut blurred = Out::default();
        blurred.int(1);
        blurred.0.push(BLUR);
        let mut batch = HudBatch::scratch([854, 480]);
        let painted = paint_frame(&mut batch, &atlas, &blurred.0, |_, _| Some(()));
        assert!(painted.is_some_and(|p| p.blur));
        let plain = Out::default();
        let painted = paint_frame(&mut batch, &atlas, &plain.0, |_, _| Some(()));
        assert!(painted.is_some_and(|p| !p.blur));
    }
}
