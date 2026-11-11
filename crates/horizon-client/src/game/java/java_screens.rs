// game-module-cfg: feature java-mods
//! Java mods' screens in Horizon's window: the Java client in the same JVM opens them from what the
//! player's server view sent it, draws them into a list Rust paints, and takes the window's mouse
//! and keys while one is open.

use super::java_hooks_client::{HudLayer, JavaClick, JavaKeyAction, Leaf};
use super::*;

/// One segment before the first layer, then two per layer.
const HUD_SEGMENTS: usize = 49;

/// Two recorded segments per `Leaf`.
const SEGMENTS: usize = 28;

pub(in crate::game) mod live {
    use super::*;
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
        items: Option<(Vec<super::super::java_models::ItemKey>, Vec<u8>)>,
        decorations: Option<(Vec<Vec<u8>>, Vec<u8>)>,
        item_text: Option<(Vec<super::super::java_item_text::TextKey>, Vec<u8>)>,
    }

    #[derive(Default)]
    struct Done {
        wire: Option<Vec<u8>>,
        tick: Option<Vec<u8>>,
        models: Option<Vec<u8>>,
        renders: Option<Vec<u8>>,
        items: Option<(Vec<super::super::java_models::ItemKey>, Vec<u8>)>,
        decorations: Option<(Vec<Vec<u8>>, Vec<u8>)>,
        item_text: Option<(Vec<super::super::java_item_text::TextKey>, Vec<u8>)>,
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

    const QUAD: u8 = 0;
    const TEXT: u8 = 1;
    const ITEM: u8 = 2;
    const BLUR: u8 = 3;

    type Corner = ([f32; 2], [f32; 2], u32);

    /// What a recorded frame asked of the window beyond its pieces.
    struct Painted {
        blur: bool,
    }

    /// A UV within 1e-4 of an integer is that integer.
    fn snapped(x: f32) -> f32 {
        let r = x.round();
        if (x - r).abs() < 1e-4 { r } else { x }
    }

    /// `[start, integer crossings.., end]` of a UV range from `a` to `b`, in that order, and the fractions they sit at.
    fn uv_cuts(a: f32, b: f32) -> Vec<(f32, f32)> {
        let (a, b) = (snapped(a), snapped(b));
        let mut cuts = vec![(0.0, a)];
        if a != b {
            let (lo, hi) = (a.min(b), a.max(b));
            let mut k = lo.floor() + 1.0;
            let mut inside = Vec::new();
            while k < hi {
                inside.push((((k - a) / (b - a)), k));
                k += 1.0;
            }
            inside.sort_by(|x, y| x.0.total_cmp(&y.0));
            cuts.extend(inside);
        }
        cuts.push((1.0, b));
        cuts
    }

    /// A quad whose UVs leave [0,1] as one quad a tile, each tile's UVs inside [0,1], as REPEAT would sample it.
    fn repeat_cells(corners: [Corner; 4]) -> Vec<[Corner; 4]> {
        let outside = |x: f32| {
            let x = snapped(x);
            !(0.0..=1.0).contains(&x)
        };
        if !corners.iter().any(|c| outside(c.1[0]) || outside(c.1[1])) {
            return vec![corners];
        }
        let across = uv_cuts(corners[0].1[0], corners[3].1[0]);
        let down = uv_cuts(corners[0].1[1], corners[1].1[1]);
        let mix = |s: f32, t: f32| -> Corner {
            let w = [(1.0 - s) * (1.0 - t), (1.0 - s) * t, s * t, s * (1.0 - t)];
            let sum = |f: &dyn Fn(&Corner) -> f32| {
                corners.iter().zip(w).map(|(c, w)| f(c) * w).sum::<f32>()
            };
            let channel = |shift: u32| {
                sum(&|c| ((c.2 >> shift) & 255) as f32)
                    .round()
                    .clamp(0.0, 255.0) as u32
            };
            (
                [sum(&|c| c.0[0]), sum(&|c| c.0[1])],
                [0.0, 0.0],
                channel(24) << 24 | channel(16) << 16 | channel(8) << 8 | channel(0),
            )
        };
        let mut cells = Vec::new();
        for s in across.windows(2) {
            let ku = s[0].1.min(s[1].1).floor();
            for t in down.windows(2) {
                let kv = t[0].1.min(t[1].1).floor();
                let at = |(si, ti): (usize, usize)| {
                    let (s, t) = (s[si], t[ti]);
                    let mut c = mix(s.0, t.0);
                    c.1 = [s.1 - ku, t.1 - kv];
                    c
                };
                cells.push([at((0, 0)), at((0, 1)), at((1, 1)), at((1, 0))]);
            }
        }
        cells
    }

    /// `Identifier` of a texture file to its name in Horizon's GUI atlas.
    fn atlas_key(id: &str) -> Option<String> {
        let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
        let path = path.strip_prefix("textures/")?.strip_suffix(".png")?;
        Some(if namespace == "minecraft" {
            path.to_owned()
        } else {
            format!("{namespace}:{path}")
        })
    }

    fn scissor(reply: &mut In) -> Option<Option<[f32; 4]>> {
        Some(if reply.bool()? {
            Some([
                reply.int()? as f32,
                reply.int()? as f32,
                reply.int()? as f32,
                reply.int()? as f32,
            ])
        } else {
            None
        })
    }

    fn quad(batch: &mut HudBatch, atlas: &HudAtlas, reply: &mut In) -> Option<()> {
        let texture = reply.text()?;
        let kind = reply.byte()?;
        let wrap = reply.byte()? == 1;
        let mut corners = [([0f32; 2], [0f32; 2], 0u32); 4];
        for corner in &mut corners {
            *corner = (
                [reply.float()?, reply.float()?],
                [reply.float()?, reply.float()?],
                horizon_client::hud_layout::abgr(reply.int()? as u32),
            );
        }
        let clip = scissor(reply)?;
        let rect = if texture.is_empty() {
            atlas.sprites.get("horizon/white").copied()
        } else {
            atlas_key(texture).and_then(|key| atlas.sprites.get(&key).copied())
        };
        let Some(r) = rect else {
            thread_local!(static MISSING: RefCell<std::collections::HashSet<String>> = RefCell::default());
            if MISSING.with_borrow_mut(|m| m.insert(texture.to_owned())) {
                log::warn!("Horizon: a Java screen's texture {texture} isn't in the GUI atlas");
            }
            return Some(());
        };
        let start = batch.vertices.len();
        let cells = if wrap && !texture.is_empty() {
            repeat_cells(corners)
        } else {
            vec![corners]
        };
        for cell in cells {
            for i in [0, 1, 2, 0, 2, 3] {
                let (pos, uv, color) = cell[i];
                let uv = if texture.is_empty() {
                    [(r.u0 + r.u1) / 2.0, (r.v0 + r.v1) / 2.0]
                } else {
                    [r.u0 + uv[0] * (r.u1 - r.u0), r.v0 + uv[1] * (r.v1 - r.v0)]
                };
                batch.vertices.push(HudVertex {
                    pos,
                    uv,
                    color,
                    depth: 0.0,
                    glint_uv: [0.0, 0.0],
                    text: 0,
                });
            }
        }
        match kind {
            1 => batch.invert_since(start),
            2 => batch.blend_since(start, horizon_client::hud::Blend::Additive),
            _ => {}
        }
        if let Some([x0, y0, x1, y1]) = clip {
            batch.clip_since(start, x0, y0, x1, y1);
        }
        Some(())
    }

    fn text(batch: &mut HudBatch, atlas: &HudAtlas, reply: &mut In) -> Option<()> {
        let (x, y, sx, sy) = (
            reply.float()?,
            reply.float()?,
            reply.float()?,
            reply.float()?,
        );
        let color = reply.int()? as u32;
        let shadow = reply.bool()?;
        let _background = reply.int()?;
        let mut text = Text::literal("");
        for _ in 0..reply.int()? {
            let run = reply.text()?;
            let rgb = reply.int()?;
            let flags = reply.byte()?;
            let font = reply.text()?;
            let on = |bit: u8| (flags & bit != 0).then_some(true);
            let style = Style {
                color: (rgb >= 0).then_some(rgb as u32),
                bold: on(1),
                italic: on(2),
                underlined: on(4),
                strikethrough: on(8),
                obfuscated: on(16),
                font: (!font.is_empty() && font != "minecraft:default").then(|| font.to_owned()),
                ..Style::default()
            };
            text = text.append(Text::styled(run, style));
        }
        let clip = scissor(reply)?;
        if color >> 24 == 0 {
            return Some(());
        }
        let start = batch.vertices.len();
        batch.text_styled(
            atlas,
            &text,
            x,
            y,
            horizon_client::hud_layout::abgr(color),
            shadow,
        );
        if sx != 1.0 || sy != 1.0 {
            batch.scale_since(start, x, y, sx, sy);
        }
        if let Some([x0, y0, x1, y1]) = clip {
            batch.clip_since(start, x0, y0, x1, y1);
        }
        Some(())
    }

    /// A recorded frame's pieces in call order; `item` paints an item entry. None when it ended in pieces.
    fn paint_frame(
        batch: &mut HudBatch,
        atlas: &HudAtlas,
        frame: &[u8],
        item: impl Fn(&mut HudBatch, &mut In) -> Option<()>,
    ) -> Option<Painted> {
        let mut reply = In(frame);
        let mut painted = Painted { blur: false };
        for _ in 0..reply.int().unwrap_or(0) {
            let ok = match reply.byte() {
                Some(QUAD) => quad(batch, atlas, &mut reply),
                Some(BLUR) => {
                    painted.blur = true;
                    Some(())
                }
                Some(TEXT) => text(batch, atlas, &mut reply),
                Some(ITEM) => item(batch, &mut reply),
                _ => None,
            };
            if ok.is_none() {
                log::warn!("Horizon: a Java frame ended in pieces");
                return None;
            }
        }
        Some(painted)
    }

    /// Whether this frame records the HUD: only while it animates, and not on the frame that took a tick's status or while a tick runs.
    fn hud_frame_wanted(animating: bool, recorded: bool, busy: bool) -> bool {
        animating && !recorded && !busy
    }

    /// A frame recording's reply: whether the HUD still animates, and the recording if it differs from the last.
    fn java_hud_frame_apply(reply: &[u8], spent: u128) {
        let mut reply = In(reply);
        let (Some(animating), Some(changed)) = (reply.bool(), reply.bool()) else {
            return;
        };
        let frame = changed.then(|| reply.bytes()).flatten().map(<[u8]>::to_vec);
        CLIENT.with_borrow_mut(|c| {
            c.hud_frames.0 += 1;
            c.hud_frames.1 += spent;
            if let Some(frame) = frame {
                c.hud = read_hud(&frame).unwrap_or_default();
            }
            if !animating {
                c.hud_animating = false;
                if std::env::var_os("HORIZON_JAVA_DEBUG").is_some() {
                    let (n, total) = c.hud_frames;
                    log::info!(
                        "Horizon: HUD animated over {n} frames, {} µs per frame recording (request included)",
                        total / u128::from(n.max(1))
                    );
                }
                c.hud_frames = (0, 0);
            }
        });
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
            let lockstep = reply.bool()?;
            if lockstep != super::super::java_lockstep::lockstep_next()
                && std::env::var_os("HORIZON_JAVA_DEBUG").is_some()
            {
                log::info!("Horizon: client tick status lockstep {lockstep}");
            }
            super::super::java_lockstep::set_lockstep_next(lockstep);
            super::super::java_world_renders::java_world_status(world.as_deref(), world_animating);
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
                        let name = super::super::java_particles::java_particle_name(id)
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

        /// Whether the world has the window's input: no screen of the window's own is open over it.
        fn java_world_has_input(&self) -> bool {
            self.screen == Screen::Playing && self.title.is_none() && !self.screen_over_playing()
        }

        /// The window's input while no Java screen is open: the world's keys and buttons reach the Java client's callbacks at its next tick, in order; a key the window's own screen took reaches only the keyboard's state.
        pub(in crate::game) fn java_window_input(&mut self, event: &winit::event::WindowEvent) {
            use winit::event::WindowEvent;
            if let WindowEvent::Focused(focused) = event {
                CLIENT.with_borrow_mut(|c| c.focused = Some(*focused));
                return;
            }
            if !CLIENT.with_borrow(|c| c.joined) || self.screen == Screen::Java {
                return;
            }
            if self.java_shell_open() {
                return;
            }
            let input = match event {
                WindowEvent::KeyboardInput { event, .. } => {
                    let PhysicalKey::Code(code) = event.physical_key else {
                        return;
                    };
                    self.java_key_input(code, event.state == ElementState::Pressed, event.repeat);
                    return;
                }
                WindowEvent::MouseInput { state, button, .. } if self.java_world_has_input() => {
                    Input::Button(
                        button_number(*button),
                        i32::from(*state == ElementState::Pressed),
                        self.java_modifiers(),
                    )
                }
                _ => return,
            };
            self.java_queue(input);
        }

        fn java_queue(&mut self, input: Input) {
            CLIENT.with_borrow_mut(|c| {
                let mut out = Out(std::mem::take(&mut c.queue));
                input.write(&mut out);
                c.queue = out.0;
                c.queued += 1;
            });
        }

        /// A key as the window's keyboard callback gets it, in the world or over it; also what `horizon-ctl key` stands in for.
        pub(in crate::game) fn java_key_input(
            &mut self,
            code: KeyCode,
            pressed: bool,
            repeat: bool,
        ) {
            if !CLIENT.with_borrow(|c| c.joined) || self.screen == Screen::Java {
                return;
            }
            if self.java_shell_open() {
                return;
            }
            let Some(name) = horizon_client::keybinds::key_name(code) else {
                return;
            };
            let input = if self.java_world_has_input() {
                let action = match (pressed, repeat) {
                    (false, _) => 0,
                    (true, false) => 1,
                    (true, true) => 2,
                };
                Input::Key(
                    name,
                    action,
                    self.java_modifiers(),
                    sdl_keycode_of(name, None),
                )
            } else {
                Input::KeyState(name, pressed)
            };
            self.java_queue(input);
        }

        /// The Java client runs the click first, with the window's hit, so mods hear it in vanilla's order: `Some` when one canceled it, with the hand to swing.
        pub(in crate::game) fn java_click_canceled(
            &mut self,
            click: JavaClick,
        ) -> Option<Option<Hand>> {
            if !CLIENT.with_borrow(|c| c.joined) || self.entity_targeted() {
                return None;
            }
            let host = horizon_java_host::get()?;
            self.java_publish_client_world();
            let mut out = Out::default();
            out.int(click as i32);
            let hit = self.pick();
            out.bool(hit.is_some());
            if let Some(hit) = hit {
                out.long(BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]).pack());
                out.int(face_index(hit.face));
                for value in hit.point {
                    out.double(value);
                }
            }
            let began = std::time::Instant::now();
            let reply = host.request("client_click", &out.0);
            let waited = began.elapsed();
            if waited > std::time::Duration::from_millis(5)
                || std::env::var_os("HORIZON_JAVA_DEBUG").is_some()
            {
                log::info!(
                    "Horizon: a click waited {:.2} ms on the Java client",
                    waited.as_secs_f64() * 1000.0
                );
            }
            if reply.is_none() {
                log::warn!(
                    "Horizon: the Java client's click failed (its stack is above); the window acts on the click alone"
                );
            }
            CLIENT.with_borrow_mut(|c| {
                c.block_use = reply
                    .as_deref()
                    .and_then(|r| r.get(1))
                    .copied()
                    .unwrap_or(0);
            });
            match reply.as_deref().and_then(|r| r.first()) {
                Some(1) => Some(None),
                Some(2) => Some(Some(Hand::Main)),
                Some(3) => Some(Some(Hand::Off)),
                _ => None,
            }
        }

        /// `getDestroyProgress` on the Java client's player; None before it joined.
        pub(in crate::game) fn java_client_destroy_progress(
            &mut self,
            pos: [i32; 3],
        ) -> Option<f32> {
            if !CLIENT.with_borrow(|c| c.joined) {
                return None;
            }
            let host = horizon_java_host::get()?;
            self.java_publish_client_world();
            let mut out = Out::default();
            out.long(BlockPos::new(pos[0], pos[1], pos[2]).pack());
            host.request("client_destroy_progress", &out.0)
                .and_then(|reply| In(&reply).float())
        }

        /// The Java client's own `useItemOn` result for the click just run, when the block at `pos` is a mod's: it decides whether the click ends there, as the block's code does in vanilla.
        pub(in crate::game) fn java_block_prediction(
            &mut self,
            pos: [i32; 3],
        ) -> Option<horizon_world::level::InteractionResult> {
            use horizon_world::level::InteractionResult as R;
            let state = self.world.block(pos[0], pos[1], pos[2]);
            let answered = CLIENT.with_borrow_mut(|c| std::mem::take(&mut c.block_use));
            if !horizon_java_host::bridge::java_block(state) {
                return None;
            }
            match answered {
                1 => Some(R::Pass),
                2 => Some(R::Success),
                3 => Some(R::Consume),
                _ => None,
            }
        }

        /// `handleKeybinds`' drop, hand swap and hotbar keys, as the window performs them: they reach the Java client in order with the world's other input.
        pub(in crate::game) fn java_key_action(&mut self, action: JavaKeyAction) {
            if !CLIENT.with_borrow(|c| c.joined) {
                return;
            }
            let input = match action {
                JavaKeyAction::Drop(all) => Input::Drop(all),
                JavaKeyAction::SwapHands => Input::SwapHands,
                JavaKeyAction::Hotbar(slot) => Input::Hotbar(slot as i32),
            };
            CLIENT.with_borrow_mut(|c| {
                let mut out = Out(std::mem::take(&mut c.queue));
                input.write(&mut out);
                c.queue = out.0;
                c.queued += 1;
            });
        }

        /// The window's bindings for the mods' key mappings, set on the Java client's own `KeyMapping`s: at its login, and when a binding changes.
        pub(in crate::game) fn java_apply_mod_keys(&self) {
            let table = horizon_client::mod_keys::mod_keys();
            if table.keys.is_empty() {
                return;
            }
            let Some(host) = horizon_java_host::get() else {
                return;
            };
            let mut out = Out::default();
            out.int(table.keys.len() as i32);
            for key in &table.keys {
                let (bound, bits) = self
                    .settings
                    .mod_keys
                    .binding(&key.id, (&key.key, key.modifiers.bits));
                out.text(&key.id);
                out.text(&bound);
                out.text(horizon_client::mod_keys::Modifier::of_bits(bits).name());
            }
            host.request("client_keys_apply", &out.0);
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
            super::super::java_components::prepare_font(out.0);
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
            super::super::java_world_renders::java_world_left();
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
            super::super::java_bridges::client_world::java_forget_client_world();
        }

        /// The start of the window's tick: the Java client's last tick results are applied, before the window's tick reads what they changed.
        pub(in crate::game) fn java_client_tick_begin(
            &mut self,
        ) -> Option<super::super::java_lockstep::LockstepTick> {
            use super::super::java_lockstep as lockstep;
            self.remote_profile()?;
            let host = horizon_java_host::get()?;
            let open = CLIENT.with_borrow(|c| c.open);
            // A screen ticks in step with the window, so its tick waits for any background one first.
            self.java_client_collect(open);
            if !lockstep::lockstep_next() || !CLIENT.with_borrow(|c| c.joined) {
                return None;
            }
            // The previous job must have ended before this one is handed over.
            self.java_client_collect(true);
            if !lockstep::lockstep_next() {
                return None;
            }
            CLIENT.with_borrow_mut(|c| c.owed = 1);
            let guard = lockstep::start();
            if host.request("client_lockstep_announce", &[]).is_none() {
                drop(guard);
                lockstep::finish();
                return None;
            }
            let (job, _) = self.java_client_batch(true);
            WORKER.with_borrow_mut(|w| w.get_or_insert_with(Worker::start).send(job));
            Some(guard)
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
            if let Some(picked) = super::super::java_lockstep::keep_picks(picked) {
                CLIENT.with_borrow_mut(|c| c.picked = picked);
            }
        }

        /// `Minecraft.runTick` for the Java client, once the window's tick has run: what the server sent it and the tick's picks, then the real `Minecraft.tick`.
        pub(in crate::game) fn java_client_tick_send(&mut self) {
            if self.remote_profile().is_none() {
                return;
            }
            let Some(host) = horizon_java_host::get() else {
                return;
            };
            if super::super::java_lockstep::in_lockstep() {
                // The job went out at the tick's start; the picks its player step never took wait for the next batch.
                let unsent = super::super::java_lockstep::finish();
                if !unsent.is_empty() {
                    CLIENT.with_borrow_mut(|c| c.animate.push_back(unsent));
                }
                let open = CLIENT.with_borrow(|c| c.open)
                    || (!self.java_world_has_input() && self.screen != Screen::Java);
                if open {
                    self.java_client_collect(true);
                }
                return;
            }
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
            let (job, open) = self.java_client_batch(false);
            if open {
                self.java_client_collect(true);
                let done = job.run(host);
                self.java_client_apply(done);
            } else {
                WORKER.with_borrow_mut(|w| w.get_or_insert_with(Worker::start).send(job));
            }
        }

        /// One client tick's request, `ClientBridge.tick`'s input; with whether a native screen is over the world, when the tick runs in step with the window.
        fn java_client_batch(&mut self, lockstep: bool) -> (Job, bool) {
            self.java_client_collisions();
            let mut out = Out::default();
            out.int(CLIENT.with_borrow_mut(|c| std::mem::take(&mut c.owed)));
            out.float(self.clock.fraction() as f32);
            out.bool(lockstep);
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
                items: super::super::java_models::java_items_request(),
                decorations: super::super::java_models::java_decorations_request(),
                item_text: super::super::java_item_text::request(),
                remote: CLIENT.with_borrow(|c| c.remote),
                tick: out.0,
            };
            (job, open)
        }

        /// `MouseHandler.onScroll` in the world, run by the Java client: true when a mod's `MouseScrollingEvent` listener canceled it.
        pub(in crate::game) fn java_world_scroll(&self, x: f64, y: f64) -> bool {
            if !CLIENT.with_borrow(|c| c.joined) {
                return false;
            }
            let Some(host) = horizon_java_host::get() else {
                return false;
            };
            let mut out = Out::default();
            out.double(x);
            out.double(y);
            host.request("client_scroll", &out.0)
                .is_some_and(|reply| reply.first() == Some(&1))
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
                    super::super::java_world_renders::java_world_frame_apply(&reply, spent);
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
                super::super::java_item_text::forget_pending();
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
                super::super::java_models::java_items_apply(keys, &reply, self.world.game_time);
            }
            if let Some((keys, reply)) = done.decorations {
                super::super::java_models::java_decorations_apply(keys, &reply);
            }
            if let Some((keys, reply)) = done.item_text {
                super::super::java_item_text::apply(keys, &reply);
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

        /// What a mod's item decorators recorded over this stack, in the slot at `(x, y)`.
        pub(in crate::game) fn draw_java_item_decorations(
            &self,
            batch: &mut HudBatch,
            stack: &ItemStack,
            x: f32,
            y: f32,
        ) {
            let Some(frame) = super::super::java_models::java_decorations(stack) else {
                return;
            };
            let start = batch.vertices.len();
            let mut reply = In(&frame);
            for _ in 0..reply.int().unwrap_or(0) {
                let ok = match reply.byte() {
                    Some(QUAD) => quad(batch, &self.gui.atlas, &mut reply),
                    Some(TEXT) => text(batch, &self.gui.atlas, &mut reply),
                    Some(ITEM) => self.java_screen_item(batch, &mut reply),
                    _ => None,
                };
                if ok.is_none() {
                    log::warn!("Horizon: an item decorator's frame ended in pieces");
                    break;
                }
            }
            for vertex in &mut batch.vertices[start..] {
                vertex.pos[0] += x;
                vertex.pos[1] += y;
            }
        }

        /// This frame's HUD recording request: the window's size and its progress into the next tick.
        fn java_hud_frame_request(&mut self) -> Vec<u8> {
            let mut out = Out::default();
            self.java_client_size(&mut out);
            out.float(self.clock.fraction() as f32);
            out.0
        }

        /// A window frame's collection of the Java client's finished work, without waiting for it, and the next frame recording's request while the HUD or the world animates; a frame that took a tick's status, or a tick still running, asks for none, so the window never waits on Java.
        pub(in crate::game) fn java_client_frame(&mut self) {
            if CLIENT.with_borrow_mut(|c| std::mem::take(&mut c.shell.opening)) {
                self.java_client_collect(true);
            } else {
                self.java_client_collect(false);
            }
            let busy = java_client_busy();
            let selected = self.selected;
            let (animating, recorded) = CLIENT.with_borrow_mut(|c| {
                let hotbar_drawn = c.hud.drawn & (1 << HudLayer::Hotbar as u32) != 0;
                let moved =
                    hotbar_drawn && std::mem::replace(&mut c.hud_selected, selected) != selected;
                c.hud_selected = selected;
                (c.hud_animating || moved, std::mem::take(&mut c.recorded))
            });
            let hud =
                hud_frame_wanted(animating, recorded, busy).then(|| self.java_hud_frame_request());
            let world = self.java_world_frame_request(busy);
            if (hud.is_some() || world.is_some()) && horizon_java_host::get().is_some() {
                WORKER.with_borrow_mut(|w| {
                    w.get_or_insert_with(Worker::start)
                        .send_frame(FrameJob { hud, world });
                });
            }
        }

        /// What mods drew before the first layer.
        pub(in crate::game) fn hud_start(&mut self, batch: &mut HudBatch) {
            CLIENT.with_borrow_mut(|c| c.hud_painted = 0);
            self.paint_hud_segment(batch, 0);
        }

        /// What mods drew after the layers native did not draw this frame, in vanilla's order.
        pub(in crate::game) fn hud_end(&mut self, batch: &mut HudBatch) {
            let painted = CLIENT.with_borrow(|c| c.hud_painted);
            for layer in HudLayer::ALL {
                if painted & (1 << layer as u32) == 0 {
                    self.paint_hud_segment(batch, layer as usize * 2 + 2);
                }
            }
        }

        fn paint_hud_segment(&mut self, batch: &mut HudBatch, index: usize) {
            let frame = CLIENT.with_borrow(|c| c.hud.segments[index].clone());
            if frame.len() > 4 {
                paint_frame(batch, &self.gui.atlas, &frame, |batch, reply| {
                    self.java_screen_item(batch, reply)
                });
            }
        }

        /// Native's vanilla layer, unless a mod cancelled it or Java drew it; then what mods drew after it.
        pub(in crate::game) fn hud_layer<R>(
            &mut self,
            batch: &mut HudBatch,
            layer: Option<HudLayer>,
            native: impl FnOnce(&mut Self, &mut HudBatch) -> R,
        ) -> Option<R> {
            let Some(layer) = layer else {
                return Some(native(self, batch));
            };
            let (cancelled, drawn) = CLIENT.with_borrow_mut(|c| {
                c.hud_painted |= 1 << layer as u32;
                (c.hud.cancelled, c.hud.drawn)
            });
            let (drawn_by_native, drawn_by_java) = hud_layer_draws(cancelled, drawn, layer);
            let index = layer as usize * 2;
            let mut answer = None;
            if drawn_by_java {
                self.paint_hud_segment(batch, index + 1);
            } else if drawn_by_native {
                answer = Some(native(self, batch));
            }
            self.paint_hud_segment(batch, index + 2);
            answer
        }

        /// The open Java screen asked for the menu blur this frame.
        pub(in crate::game) fn java_screen_blurred(&self) -> bool {
            CLIENT.with_borrow(|c| c.blurred)
        }

        /// The open Java screen, drawn as it extracted itself this frame.
        pub(in crate::game) fn draw_java_screen(&mut self, batch: &mut HudBatch) {
            CLIENT.with_borrow_mut(|c| c.blurred = false);
            let Some(host) = horizon_java_host::get() else {
                return;
            };
            self.java_publish_client_world();
            let mut out = Out::default();
            self.java_client_size(&mut out);
            out.double(f64::from(self.cursor.0));
            out.double(f64::from(self.cursor.1));
            out.float(0.0);
            let Some(reply) = host.request("client_frame", &out.0) else {
                return;
            };
            let painted = paint_frame(batch, &self.gui.atlas, &reply, |batch, reply| {
                self.java_screen_item(batch, reply)
            });
            CLIENT.with_borrow_mut(|c| c.blurred = painted.is_some_and(|p| p.blur));
        }

        fn java_screen_item(&self, batch: &mut HudBatch, reply: &mut In) -> Option<()> {
            let (x, y, sx, sy) = (
                reply.float()?,
                reply.float()?,
                reply.float()?,
                reply.float()?,
            );
            let stack = java_mods::read_tag(reply.bytes()?)
                .and_then(|tag| horizon_client::save::item_from_nbt(&tag));
            let clip = scissor(reply)?;
            let Some(stack) = stack else {
                return Some(());
            };
            let start = batch.vertices.len();
            self.draw_item_model(batch, &stack, x, y);
            if sx != 1.0 || sy != 1.0 {
                batch.scale_since(start, x, y, sx, sy);
            }
            if let Some([x0, y0, x1, y1]) = clip {
                batch.clip_since(start, x0, y0, x1, y1);
            }
            Some(())
        }

        /// The vanilla screen a Java shell stands for: 1 the survival inventory.
        fn java_shell_kind(&self) -> u8 {
            u8::from(
                self.screen == Screen::Inventory
                    && self.container.is_none()
                    && self.craft_size == 2
                    && !self.mount_active()
                    && self.station.is_none(),
            )
        }

        /// leftPos, topPos, imageWidth, imageHeight and the hovered menu slot, as native lays the screen out now.
        fn java_shell_layout(&self) -> [i32; 5] {
            let batch = HudBatch::new(
                self.java_client_window_size(),
                self.settings.gui_scale_inputs(),
            );
            let (x, y) = self.inventory_origin(&batch);
            let hovered = self
                .slot_at(&batch, self.cursor.0, self.cursor.1)
                .and_then(|slot| Self::menu_slot_index("inventory", slot));
            [
                x.floor() as i32,
                y.floor() as i32,
                176,
                166,
                hovered.map_or(-1, |slot| slot as i32),
            ]
        }

        fn java_client_window_size(&self) -> [u32; 2] {
            self.renderer
                .as_ref()
                .map_or([854, 480], |r| [r.config.width, r.config.height])
        }

        /// A shell is the Java client's screen and native's inventory is showing.
        pub(in crate::game) fn java_shell_open(&self) -> bool {
            self.java_shell_kind() == 1 && CLIENT.with_borrow(|c| c.shell.open)
        }

        /// Applies a `ShellReply`: its recording, its animation; returns who took the input.
        fn java_shell_apply(&self, reply: &mut In) -> Option<ShellOwner> {
            let (owner, recording, animating) = read_shell_reply(reply)?;
            let at = self.cursor;
            CLIENT.with_borrow_mut(|c| {
                c.shell.animating = animating;
                c.shell.drawn_at = Some(at);
                if let Some((segments, routed)) = recording {
                    c.shell.segments = segments;
                    c.shell.routed = routed;
                }
            });
            Some(owner)
        }

        /// While the shell is open and its recording is stale: the cursor moved or it animates, and no tick status or running tick is newer.
        pub(in crate::game) fn java_shell_frame(&mut self) {
            if !self.java_shell_open() {
                return;
            }
            let (wanted, fresh) = CLIENT.with_borrow_mut(|c| {
                (
                    c.shell.drawn_at != Some(self.cursor) || c.shell.animating,
                    std::mem::take(&mut c.shell.fresh),
                )
            });
            if !wanted || fresh || java_client_busy() {
                return;
            }
            let Some(host) = horizon_java_host::get() else {
                return;
            };
            self.java_publish_client_world();
            let scale = self.java_client_window()[2] as f32;
            let at = (
                f64::from(self.cursor.0 * scale),
                f64::from(self.cursor.1 * scale),
            );
            let mut out = Out::default();
            self.java_client_size(&mut out);
            out.float(self.clock.fraction() as f32);
            out.double(at.0);
            out.double(at.1);
            self.java_shell_layout().iter().for_each(|v| out.int(*v));
            CLIENT.with_borrow_mut(|c| c.sent.cursor = Some(at));
            if let Some(reply) = host.request("client_shell_frame", &out.0) {
                self.java_shell_apply(&mut In(&reply));
            }
        }

        fn paint_shell_segment(&mut self, batch: &mut HudBatch, index: usize) {
            let frame = CLIENT.with_borrow(|c| c.shell.segments[index].clone());
            if frame.len() > 4 {
                paint_frame(batch, &self.gui.atlas, &frame, |batch, reply| {
                    self.java_screen_item(batch, reply)
                });
            }
        }

        /// Native's own step, or Java's when Java drew it; then what mods drew after it.
        pub(in crate::game) fn shell_leaf(
            &mut self,
            batch: &mut HudBatch,
            leaf: Leaf,
            native: impl FnOnce(&mut Self, &mut HudBatch),
        ) {
            if !CLIENT.with_borrow(|c| c.shell.open) {
                return native(self, batch);
            }
            let index = leaf as usize * 2;
            let (drawn_by_native, drawn_by_java) =
                leaf_draws(CLIENT.with_borrow(|c| c.shell.routed), leaf);
            if drawn_by_native {
                native(self, batch);
            }
            if drawn_by_java {
                self.paint_shell_segment(batch, index + 1);
            }
            self.paint_shell_segment(batch, index);
        }

        /// Input over a shell: Java's screen events and mods' widgets first, in vanilla's order; native acts only when this says `Native`.
        fn java_shell_input(
            &mut self,
            inputs: &[Input],
            native_widget: bool,
            native_handles: bool,
        ) -> ShellOwner {
            let Some(host) = horizon_java_host::get() else {
                return ShellOwner::Native;
            };
            if !self.java_shell_open() {
                return ShellOwner::Native;
            }
            self.java_publish_client_world();
            let mut out = Out::default();
            self.java_client_size(&mut out);
            self.java_shell_layout().iter().for_each(|v| out.int(*v));
            out.bool(native_widget);
            out.bool(native_handles);
            out.int(inputs.len() as i32);
            for input in inputs {
                input.write(&mut out);
            }
            host.request("client_shell_input", &out.0)
                .and_then(|reply| self.java_shell_apply(&mut In(&reply)))
                .unwrap_or(ShellOwner::Native)
        }

        /// A mouse button over the shell: true when native acts on it.
        pub(in crate::game) fn java_shell_mouse(
            &mut self,
            button: MouseButton,
            pressed: bool,
        ) -> bool {
            let hit = pressed && self.recipe_book_hit(self.cursor.0, self.cursor.1);
            let mut inputs: Vec<Input> = self.java_cursor().into_iter().collect();
            inputs.push(Input::Button(
                button_number(button),
                i32::from(pressed),
                self.java_modifiers(),
            ));
            self.java_shell_input(&inputs, hit, true) == ShellOwner::Native
        }

        /// The wheel over the shell: true when native scrolls.
        pub(in crate::game) fn java_shell_wheel(&mut self, x: f64, y: f64) -> bool {
            let handles =
                self.recipe_book_hit(self.cursor.0, self.cursor.1) || self.bundle_under_cursor();
            let mut inputs: Vec<Input> = self.java_cursor().into_iter().collect();
            inputs.push(Input::Scroll(x, y));
            self.java_shell_input(&inputs, false, handles) == ShellOwner::Native
        }

        /// A key and its text over the shell: true when native acts on the event. A release always reaches native, which only keeps its held keys.
        pub(in crate::game) fn java_shell_key(&mut self, event: &winit::event::KeyEvent) -> bool {
            let PhysicalKey::Code(code) = event.physical_key else {
                return true;
            };
            let pressed = event.state == ElementState::Pressed;
            if pressed {
                self.keys.insert(code);
            } else {
                self.keys.remove(&code);
            }
            let modifiers = self.java_modifiers();
            let Some(name) = horizon_client::keybinds::key_name(code) else {
                return true;
            };
            let action = match (pressed, event.repeat) {
                (false, _) => 0,
                (true, false) => 1,
                (true, true) => 2,
            };
            let handles = pressed && self.native_inventory_key(code);
            let owner = self.java_shell_input(
                &[Input::Key(
                    name,
                    action,
                    modifiers,
                    sdl_keycode_of(name, horizon_client::keybinds::layout_char(event)),
                )],
                false,
                handles,
            );
            if !pressed || owner == ShellOwner::Native {
                return true;
            }
            let typed: Vec<Input> = event
                .text
                .as_deref()
                .unwrap_or_default()
                .chars()
                .filter(|c| !c.is_control())
                .map(|c| Input::Char(c as u32))
                .collect();
            if !typed.is_empty() {
                let handles = self.recipe_search_focused();
                self.java_shell_input(&typed, false, handles);
            }
            false
        }

        /// The modifier bits of `InputWithModifiers`: SDL's left and right of each.
        fn java_modifiers(&self) -> i32 {
            let bit = |key, value| if self.keys.raw(&key) { value } else { 0 };
            bit(KeyCode::ShiftLeft, 1)
                | bit(KeyCode::ShiftRight, 2)
                | bit(KeyCode::ControlLeft, 64)
                | bit(KeyCode::ControlRight, 128)
                | bit(KeyCode::AltLeft, 256)
                | bit(KeyCode::AltRight, 512)
                | bit(KeyCode::SuperLeft, 1024)
                | bit(KeyCode::SuperRight, 2048)
        }

        /// Input while a Java screen is open goes to the Java client at once, so the screen answers before the window's next frame.
        fn java_input(&mut self, inputs: &[Input]) {
            let Some(host) = horizon_java_host::get() else {
                return;
            };
            self.java_publish_client_world();
            let mut out = Out::default();
            self.java_client_size(&mut out);
            out.int(inputs.len() as i32);
            for input in inputs {
                input.write(&mut out);
            }
            if let Some(reply) = host.request("client_input", &out.0) {
                self.java_client_status(&mut In(&reply));
            }
        }

        /// `MouseHandler.onMove` to where the window's cursor is, in window pixels, when the Java client hasn't heard of it.
        fn java_cursor(&self) -> Option<Input<'static>> {
            let scale = self.java_client_window()[2] as f32;
            let at = (
                f64::from(self.cursor.0 * scale),
                f64::from(self.cursor.1 * scale),
            );
            CLIENT.with_borrow_mut(|c| {
                (c.sent.cursor != Some(at)).then(|| {
                    c.sent.cursor = Some(at);
                    Input::Move(at.0, at.1)
                })
            })
        }

        pub(in crate::game) fn java_screen_click(&mut self, button: MouseButton, pressed: bool) {
            let modifiers = self.java_modifiers();
            let mut inputs: Vec<Input> = self.java_cursor().into_iter().collect();
            inputs.push(Input::Button(
                button_number(button),
                i32::from(pressed),
                modifiers,
            ));
            self.java_input(&inputs);
        }

        /// The screen's moves and drags follow from these at the next frame, as `handleAccumulatedMovement` makes them.
        pub(in crate::game) fn java_screen_moved(&mut self) {
            if let Some(moved) = self.java_cursor() {
                self.java_input(&[moved]);
            }
        }

        pub(in crate::game) fn java_screen_scroll(&mut self, x: f64, y: f64) {
            let mut inputs: Vec<Input> = self.java_cursor().into_iter().collect();
            inputs.push(Input::Scroll(x, y));
            self.java_input(&inputs);
        }

        /// `KeyboardHandler.keyPress` and `charTyped`: the key by its vanilla name, then what it typed.
        pub(in crate::game) fn java_screen_key(&mut self, event: &winit::event::KeyEvent) {
            let pressed = event.state == ElementState::Pressed;
            if let PhysicalKey::Code(code) = event.physical_key {
                let layout = horizon_client::keybinds::layout_char(event);
                self.java_screen_key_code(
                    code,
                    pressed,
                    event.repeat,
                    event.text.as_deref(),
                    layout,
                );
            }
        }

        pub(in crate::game) fn java_screen_key_code(
            &mut self,
            code: KeyCode,
            pressed: bool,
            repeat: bool,
            text: Option<&str>,
            layout: Option<char>,
        ) {
            if pressed {
                self.keys.insert(code);
            } else {
                self.keys.remove(&code);
            }
            let modifiers = self.java_modifiers();
            let mut inputs = Vec::new();
            if let Some(name) = horizon_client::keybinds::key_name(code) {
                let action = match (pressed, repeat) {
                    (false, _) => 0,
                    (true, false) => 1,
                    (true, true) => 2,
                };
                inputs.push(Input::Key(
                    name,
                    action,
                    modifiers,
                    sdl_keycode_of(name, layout),
                ));
            }
            if pressed && let Some(text) = text {
                inputs.extend(
                    text.chars()
                        .filter(|c| !c.is_control())
                        .map(|c| Input::Char(c as u32)),
                );
            }
            self.java_input(&inputs);
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
