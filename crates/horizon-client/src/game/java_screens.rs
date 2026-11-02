//! Java mods' screens in Horizon's window: the Java client in the same JVM opens them from what the
//! player's server view sent it, draws them into a list Rust paints, and takes the window's mouse
//! and keys while one is open.

use super::*;

#[cfg(feature = "java-mods")]
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
        font: u32,
        pending: i32,
        open: bool,
        held: Option<i32>,
        shapes: i32,
        models: i32,
        states: bool,
        last: (f64, f64),
    }

    thread_local! {
        pub(in crate::game) static CLIENT: RefCell<Client> = RefCell::default();
        static WORKER: RefCell<Option<Worker>> = const { RefCell::new(None) };
    }

    /// One client tick's calls: the tick, the dynamic states once, and the geometry its status says changed.
    struct Job {
        font: Option<Vec<u8>>,
        tick: Vec<u8>,
        states: bool,
        items: Option<(Vec<super::super::java_models::ItemKey>, Vec<u8>)>,
    }

    #[derive(Default)]
    struct Done {
        tick: Option<Vec<u8>>,
        states: Option<Vec<u8>>,
        models: Option<Vec<u8>>,
        renders: Option<Vec<u8>>,
        external: Option<Vec<u8>>,
        items: Option<(Vec<super::super::java_models::ItemKey>, Vec<u8>)>,
    }

    impl Job {
        fn run(self, host: &horizon_java_host::Host) -> Done {
            if let Some(font) = &self.font {
                host.request("client_font", font);
            }
            let Some(tick) = host.request("client_tick", &self.tick) else {
                return Done::default();
            };
            let (states, external) = if self.states {
                (
                    host.request("client_models_states", &[]),
                    host.request("client_items_external", &[]),
                )
            } else {
                (None, None)
            };
            let items = self.items.and_then(|(keys, request)| {
                Some((keys, host.request("client_items_take", &request)?))
            });
            let (models, renders) = status_pending(&tick).unwrap_or((0, 0));
            let take = |op, pending: i32| (pending > 0).then(|| host.request(op, &[])).flatten();
            Done {
                models: take("client_models_take", models),
                renders: take("client_renders_take", renders),
                tick: Some(tick),
                states,
                external,
                items,
            }
        }
    }

    /// The pending geometry and renderer counts at the end of a status reply.
    fn status_pending(status: &[u8]) -> Option<(i32, i32)> {
        let mut status = In(status);
        status.bool()?;
        status.int()?;
        status.bool()?;
        for _ in 0..status.int()? {
            status.text()?;
            status.float()?;
            status.float()?;
        }
        status.int()?;
        Some((status.int()?, status.int()?))
    }

    /// Runs the Java client's ticks off the window's thread while no Java screen is open, as chunk meshing runs off it.
    struct Worker {
        jobs: std::sync::mpsc::Sender<Job>,
        done: std::sync::mpsc::Receiver<Done>,
        busy: bool,
    }

    impl Worker {
        fn start() -> Self {
            let (jobs, queue) = std::sync::mpsc::channel::<Job>();
            let (finished, done) = std::sync::mpsc::channel();
            let _ = std::thread::Builder::new()
                .name("Java client".into())
                .spawn(move || {
                    let Some(host) = horizon_java_host::get() else {
                        return;
                    };
                    for job in queue {
                        if finished.send(job.run(host)).is_err() {
                            return;
                        }
                    }
                });
            Self {
                jobs,
                done,
                busy: false,
            }
        }

        fn send(&mut self, job: Job) {
            self.busy = self.jobs.send(job).is_ok();
        }

        fn collect(&mut self, wait: bool) -> Option<Done> {
            if !self.busy {
                return None;
            }
            let done = if wait {
                self.done.recv().map_err(|_| TryRecvError::Disconnected)
            } else {
                self.done.try_recv()
            };
            self.busy = matches!(done, Err(TryRecvError::Empty));
            done.ok()
        }
    }

    /// Whether a background client tick is running, during which the window doesn't wait on Java.
    pub(in crate::game) fn java_client_busy() -> bool {
        WORKER.with_borrow(|w| w.as_ref().is_some_and(|w| w.busy))
    }

    /// Changes whenever a block entity's client copy was made, dropped or updated, so cached shapes may be stale.
    pub(in crate::game) fn java_shape_generation() -> i32 {
        CLIENT.with_borrow(|c| c.shapes)
    }

    const QUAD: u8 = 0;
    const TEXT: u8 = 1;
    const ITEM: u8 = 2;

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
            Some(atlas.white())
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
        for i in [0, 1, 2, 0, 2, 3] {
            let (pos, uv, color) = corners[i];
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
            });
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

    impl Game<'_> {
        fn java_client_size(&self, out: &mut Out) {
            let size = self
                .renderer
                .as_ref()
                .map_or([854, 480], |r| [r.config.width, r.config.height]);
            let batch = HudBatch::new(size);
            out.int(size[0] as i32);
            out.int(size[1] as i32);
            out.int(batch.scale as i32);
        }

        fn java_client_status(&mut self, reply: &mut In) -> Option<()> {
            let open = reply.bool()?;
            let _opened = reply.int()?;
            let _pause = reply.bool()?;
            for _ in 0..reply.int()? {
                let id = reply.text()?;
                let (volume, pitch) = (reply.float()?, reply.float()?);
                let event = id.strip_prefix("minecraft:").unwrap_or(id);
                self.audio.play(event, None, volume, pitch);
            }
            let shapes = reply.int()?;
            let models = reply.int()?;
            let renders = reply.int()?;
            CLIENT.with_borrow_mut(|c| {
                c.shapes = shapes;
                c.models = models + renders;
            });
            CLIENT.with_borrow_mut(|c| c.open = open);
            if open && self.screen != Screen::Java {
                self.screen = Screen::Java;
                self.capture(false);
            } else if !open && self.screen == Screen::Java {
                self.screen = Screen::Playing;
                self.capture(true);
            }
            Some(())
        }

        /// The next range of Horizon's font's advances, which the Java client measures text with; None once all are sent.
        fn java_client_font(&self) -> Option<Vec<u8>> {
            const COUNT: u32 = 0x10000;
            let start = CLIENT.with_borrow(|c| c.font);
            if start >= COUNT {
                return None;
            }
            // About a millisecond's worth a tick, as each glyph is rasterized to be measured.
            let began = std::time::Instant::now();
            let mut advances = Vec::new();
            for c in start..COUNT {
                advances.push(
                    char::from_u32(c).map_or(0.0, |ch| self.gui.atlas.font.glyph(ch).advance),
                );
                if began.elapsed() >= std::time::Duration::from_millis(1) {
                    break;
                }
            }
            let mut out = Out::default();
            out.text(&self.settings.language);
            out.int(start as i32);
            out.int(advances.len() as i32);
            for advance in &advances {
                out.float(*advance);
            }
            CLIENT.with_borrow_mut(|c| c.font = start + advances.len() as u32);
            Some(out.0)
        }

        /// `Minecraft.tick`'s part for the Java client: what the server sent it, then the open screen's tick.
        pub(in crate::game) fn java_client_tick(&mut self) {
            let profile = self.remote_profile();
            // Before a world the window never waits for the JVM, which may still be loading.
            if profile.is_none() && !CLIENT.with_borrow(|c| c.joined) {
                return;
            }
            let Some(host) = horizon_java_host::get() else {
                return;
            };
            let Some((profile, name)) = profile else {
                self.java_client_collect(true);
                host.request("client_leave", &[]);
                horizon_client::dynamic_models::clear();
                horizon_client::dynamic_renders::clear();
                CLIENT.with_borrow_mut(|c| {
                    c.joined = false;
                    c.open = false;
                    c.states = false;
                });
                return;
            };
            let pending = host
                .request("client_pending", &[])
                .and_then(|b| In(&b).int())
                .unwrap_or(0);
            let open = CLIENT.with_borrow(|c| c.open);
            // A screen ticks in step with the window, so its tick waits for any background one first.
            self.java_client_collect(open);
            let (joined, seen, models, states, font) =
                CLIENT.with_borrow(|c| (c.joined, c.pending, c.models, c.states, c.font));
            // Recorded renderers are checked against the server once a second.
            let poll =
                self.world.game_time % 20 == 0 && !horizon_client::dynamic_renders::is_empty();
            if java_client_busy()
                || (joined
                    && !open
                    && pending == seen
                    && models == 0
                    && font >= 0x10000
                    && !poll
                    && !super::super::java_models::java_items_queued())
            {
                return;
            }
            let mut out = Out::default();
            out.long((profile >> 64) as i64);
            out.long(profile as i64);
            out.text(&name);
            out.text(self.dim.id());
            self.java_client_size(&mut out);
            let job = Job {
                items: super::super::java_models::java_items_request(),
                font: self.java_client_font(),
                tick: out.0,
                states: !states,
            };
            CLIENT.with_borrow_mut(|c| c.pending = pending);
            if open {
                let done = job.run(host);
                self.java_client_apply(done);
            } else {
                WORKER.with_borrow_mut(|w| w.get_or_insert_with(Worker::start).send(job));
            }
        }

        /// The background tick's results, once it finished; `wait` blocks until it has.
        fn java_client_collect(&mut self, wait: bool) {
            let done = WORKER.with_borrow_mut(|w| w.as_mut().and_then(|w| w.collect(wait)));
            if let Some(done) = done {
                self.java_client_apply(done);
            }
        }

        fn java_client_apply(&mut self, done: Done) {
            let Some(tick) = done.tick else {
                return;
            };
            CLIENT.with_borrow_mut(|c| c.joined = true);
            self.java_client_status(&mut In(&tick));
            if let Some(states) = done.states {
                self.java_models_states(&states);
                CLIENT.with_borrow_mut(|c| c.states = true);
            }
            if let Some(external) = done.external {
                super::super::java_models::java_items_external(&external);
            }
            if let Some((keys, reply)) = done.items {
                super::super::java_models::java_items_apply(keys, &reply);
            }
            let taken = done.models.is_some() || done.renders.is_some();
            if let Some(models) = done.models {
                self.java_models_apply(&models);
            }
            if let Some(renders) = done.renders {
                self.java_renders_apply(&renders);
            }
            if taken {
                CLIENT.with_borrow_mut(|c| c.models = 0);
            }
        }

        /// The open Java screen, drawn as it extracted itself this frame.
        pub(in crate::game) fn draw_java_screen(&mut self, batch: &mut HudBatch) {
            let Some(host) = horizon_java_host::get() else {
                return;
            };
            let mut out = Out::default();
            self.java_client_size(&mut out);
            out.double(f64::from(self.cursor.0));
            out.double(f64::from(self.cursor.1));
            out.float(0.0);
            let Some(reply) = host.request("client_frame", &out.0) else {
                return;
            };
            let mut reply = In(&reply);
            let count = reply.int().unwrap_or(0);
            for _ in 0..count {
                let ok = match reply.byte() {
                    Some(QUAD) => quad(batch, &self.gui.atlas, &mut reply),
                    Some(TEXT) => text(batch, &self.gui.atlas, &mut reply),
                    Some(ITEM) => self.java_screen_item(batch, &mut reply),
                    _ => None,
                };
                if ok.is_none() {
                    log::warn!("Horizon: a Java screen's frame ended in pieces");
                    return;
                }
            }
        }

        fn java_screen_item(&mut self, batch: &mut HudBatch, reply: &mut In) -> Option<()> {
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

        /// The modifier bits of `InputWithModifiers`: SDL's left and right of each.
        fn java_modifiers(&self) -> i32 {
            let bit = |key, value| if self.keys.contains(&key) { value } else { 0 };
            bit(KeyCode::ShiftLeft, 1)
                | bit(KeyCode::ShiftRight, 2)
                | bit(KeyCode::ControlLeft, 64)
                | bit(KeyCode::ControlRight, 128)
                | bit(KeyCode::AltLeft, 256)
                | bit(KeyCode::AltRight, 512)
                | bit(KeyCode::SuperLeft, 1024)
                | bit(KeyCode::SuperRight, 2048)
        }

        fn java_input(&mut self, kind: i32, a: i32, b: i32, delta: (f64, f64), key: &str) {
            let Some(host) = horizon_java_host::get() else {
                return;
            };
            let mut out = Out::default();
            out.int(kind);
            out.double(f64::from(self.cursor.0));
            out.double(f64::from(self.cursor.1));
            out.int(a);
            out.int(b);
            out.int(self.java_modifiers());
            out.double(delta.0);
            out.double(delta.1);
            out.text(key);
            if let Some(reply) = host.request("client_input", &out.0) {
                let mut reply = In(&reply);
                if reply.bool().is_some() {
                    self.java_client_status(&mut reply);
                }
            }
        }

        /// `MouseHandler.onButton`: SDL's numbering, left 1, middle 2, right 3.
        pub(in crate::game) fn java_screen_click(&mut self, button: MouseButton, pressed: bool) {
            let number = match button {
                MouseButton::Left => 1,
                MouseButton::Middle => 2,
                MouseButton::Right => 3,
                MouseButton::Back => 4,
                MouseButton::Forward => 5,
                MouseButton::Other(n) => i32::from(n) + 1,
            };
            CLIENT.with_borrow_mut(|c| {
                c.held = pressed.then_some(number);
                c.last = (f64::from(self.cursor.0), f64::from(self.cursor.1));
            });
            self.java_input(if pressed { 0 } else { 1 }, number, 0, (0.0, 0.0), "");
        }

        /// `MouseHandler.onMove`: a move, and a drag while a button is held.
        pub(in crate::game) fn java_screen_moved(&mut self) {
            let now = (f64::from(self.cursor.0), f64::from(self.cursor.1));
            let (held, last) =
                CLIENT.with_borrow_mut(|c| (c.held, std::mem::replace(&mut c.last, now)));
            self.java_input(2, 0, 0, (0.0, 0.0), "");
            if let Some(button) = held {
                self.java_input(3, button, 0, (now.0 - last.0, now.1 - last.1), "");
            }
        }

        pub(in crate::game) fn java_screen_scroll(&mut self, x: f64, y: f64) {
            self.java_input(4, 0, 0, (x, y), "");
        }

        /// `KeyboardHandler.keyPress` and `charTyped`: the key by its vanilla name, then what it typed.
        pub(in crate::game) fn java_screen_key(&mut self, event: &winit::event::KeyEvent) {
            let pressed = event.state == ElementState::Pressed;
            if let PhysicalKey::Code(code) = event.physical_key {
                self.java_screen_key_code(code, pressed, event.text.as_deref());
            }
        }

        pub(in crate::game) fn java_screen_key_code(
            &mut self,
            code: KeyCode,
            pressed: bool,
            text: Option<&str>,
        ) {
            if pressed {
                self.keys.insert(code);
            } else {
                self.keys.remove(&code);
            }
            if let Some(name) = horizon_client::keybinds::key_name(code) {
                self.java_input(if pressed { 5 } else { 6 }, 0, 0, (0.0, 0.0), name);
            }
            if pressed
                && self.screen == Screen::Java
                && let Some(text) = text
            {
                for ch in text.chars().filter(|c| !c.is_control()) {
                    self.java_input(7, ch as i32, 0, (0.0, 0.0), "");
                }
            }
        }
    }
}

#[cfg(not(feature = "java-mods"))]
impl Game<'_> {
    pub(super) fn draw_java_screen(&mut self, _batch: &mut HudBatch) {}
    pub(super) fn java_screen_click(&mut self, _button: MouseButton, _pressed: bool) {}
    pub(super) fn java_screen_moved(&mut self) {}
    pub(super) fn java_screen_scroll(&mut self, _x: f64, _y: f64) {}
    pub(super) fn java_screen_key(&mut self, _event: &winit::event::KeyEvent) {}
    pub(super) fn java_screen_key_code(
        &mut self,
        _code: KeyCode,
        _pressed: bool,
        _text: Option<&str>,
    ) {
    }
    pub(super) fn java_client_tick(&mut self) {}
}
