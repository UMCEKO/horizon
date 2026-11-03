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
        font: bool,
        owed: i32,
        open: bool,
        shapes: i32,
        states: bool,
        /// Input the world took since the last tick, in order, as `events` reads it.
        queue: Vec<u8>,
        queued: i32,
        focused: Option<bool>,
        /// The Java client's playing sounds by its handle.
        sounds: std::collections::HashMap<i32, horizon_client::sound_engine::InstanceId>,
        sent: Sent,
    }

    /// The window's input as the callbacks of its library would hand it to a real client.
    enum Input<'a> {
        /// `KeyboardHandler.keyPress`: the key's name, the action (release 0, press 1, repeat 2), the modifiers.
        Key(&'a str, i32, i32),
        /// A key the window's own screen took: only the keyboard's state hears of it.
        KeyState(&'a str, bool),
        Char(u32),
        /// `MouseHandler.onButton`: SDL's button number, the action, the modifiers.
        Button(i32, i32, i32),
        /// `MouseHandler.onMove`, in window pixels.
        Move(f64, f64),
        Scroll(f64, f64),
        /// One of the window's own screens opened over the world (and whether it pauses), or closed.
        Screen(bool, bool),
        Focus(bool),
        /// `handleKeybinds`' drop, with the whole stack or not.
        Drop(bool),
        SwapHands,
        Hotbar(i32),
        /// A column of the window's arrived (true) or left.
        Chunk(i32, i32, bool),
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
            };
            out.0.push(kind);
            match *self {
                Input::Key(name, action, modifiers) => {
                    out.text(name);
                    out.int(action);
                    out.int(modifiers);
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
                Input::Screen(open, pauses) => {
                    out.bool(open);
                    out.bool(pauses);
                }
                Input::Focus(focused) | Input::Drop(focused) => out.bool(focused),
                Input::SwapHands => {}
                Input::Hotbar(slot) => out.int(slot),
                Input::Chunk(x, z, arrived) => {
                    out.int(x);
                    out.int(z);
                    out.bool(arrived);
                }
            }
        }
    }

    const SOUND_PLAY: u8 = 0;
    const SOUND_UPDATE: u8 = 1;
    const SOUND_STOP: u8 = 2;

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
        native_screen: Option<(bool, bool)>,
        focused: Option<bool>,
        cursor: Option<(f64, f64)>,
        motion: Option<Motion>,
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

    thread_local! {
        pub(in crate::game) static CLIENT: RefCell<Client> = RefCell::default();
        static WORKER: RefCell<Option<Worker>> = const { RefCell::new(None) };
    }

    /// One client tick's calls: the tick and the geometry its status says changed.
    struct Job {
        tick: Vec<u8>,
        items: Option<(Vec<super::super::java_models::ItemKey>, Vec<u8>)>,
        decorations: Option<(Vec<Vec<u8>>, Vec<u8>)>,
    }

    #[derive(Default)]
    struct Done {
        tick: Option<Vec<u8>>,
        models: Option<Vec<u8>>,
        renders: Option<Vec<u8>>,
        items: Option<(Vec<super::super::java_models::ItemKey>, Vec<u8>)>,
        decorations: Option<(Vec<Vec<u8>>, Vec<u8>)>,
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
            let (models, renders) = status_pending(&tick).unwrap_or((0, 0));
            let take = |op, pending: i32| (pending > 0).then(|| host.request(op, &[])).flatten();
            Done {
                models: take("client_models_take", models),
                renders: take("client_renders_take", renders),
                tick: Some(tick),
                items,
                decorations,
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
        fn java_client_window(&self) -> [i32; 3] {
            let size = self
                .renderer
                .as_ref()
                .map_or([854, 480], |r| [r.config.width, r.config.height]);
            let batch = HudBatch::new(size);
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
            self.java_client_sounds(reply)?;
            let shapes = reply.int()?;
            CLIENT.with_borrow_mut(|c| {
                c.shapes = shapes;
                c.open = open;
            });
            if open && self.screen != Screen::Java {
                self.screen = Screen::Java;
                self.capture(false);
            } else if !open && self.screen == Screen::Java {
                self.screen = Screen::Playing;
                self.capture(true);
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
            let world = self.java_world_has_input();
            let input = match event {
                WindowEvent::KeyboardInput { event, .. } => {
                    let PhysicalKey::Code(code) = event.physical_key else {
                        return;
                    };
                    let Some(name) = horizon_client::keybinds::key_name(code) else {
                        return;
                    };
                    let pressed = event.state == ElementState::Pressed;
                    if world {
                        let action = match (pressed, event.repeat) {
                            (false, _) => 0,
                            (true, false) => 1,
                            (true, true) => 2,
                        };
                        Input::Key(name, action, self.java_modifiers())
                    } else {
                        Input::KeyState(name, pressed)
                    }
                }
                WindowEvent::MouseInput { state, button, .. } if world => Input::Button(
                    button_number(*button),
                    i32::from(*state == ElementState::Pressed),
                    self.java_modifiers(),
                ),
                _ => return,
            };
            CLIENT.with_borrow_mut(|c| {
                let mut out = Out(std::mem::take(&mut c.queue));
                input.write(&mut out);
                c.queue = out.0;
                c.queued += 1;
            });
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
            match reply.as_deref().and_then(|r| r.first()) {
                Some(1) => Some(None),
                Some(2) => Some(Some(Hand::Main)),
                Some(3) => Some(Some(Hand::Off)),
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

        /// Horizon's font's advances, which the Java client measures text with: all of them, ready for its start.
        pub(in crate::game) fn java_client_font(&self) {
            if std::env::var_os("HORIZON_JAVA_RUN").is_none()
                || CLIENT.with_borrow_mut(|c| std::mem::replace(&mut c.font, true))
            {
                return;
            }
            let advances = self.gui.atlas.font.advances();
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
            if !CLIENT.with_borrow(|c| c.joined) {
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
            CLIENT.with_borrow_mut(|c| {
                c.joined = false;
                c.open = false;
                c.states = false;
                c.owed = 0;
                c.sent = Sent::default();
                c.queue.clear();
                c.queued = 0;
            });
            for (_, id) in CLIENT.with_borrow_mut(|c| std::mem::take(&mut c.sounds)) {
                self.audio.stop_and_release(id);
            }
            super::super::java_bridges::client_world::java_forget_client_world();
        }

        /// `Minecraft.runTick` for the Java client, every tick: what the server sent it, then the real `Minecraft.tick`.
        pub(in crate::game) fn java_client_tick(&mut self) {
            if self.remote_profile().is_none() {
                return;
            }
            let Some(host) = horizon_java_host::get() else {
                return;
            };
            let open = CLIENT.with_borrow(|c| c.open);
            // A screen ticks in step with the window, so its tick waits for any background one first.
            self.java_client_collect(open);
            // `Timer.advanceTime`: a tick the last one's length delayed still runs, up to its limit of 10.
            CLIENT.with_borrow_mut(|c| c.owed = (c.owed + 1).min(10));
            if java_client_busy() {
                return;
            }
            self.java_client_collisions();
            let mut out = Out::default();
            out.int(CLIENT.with_borrow_mut(|c| std::mem::take(&mut c.owed)));
            let window = self.java_client_window();
            if CLIENT.with_borrow_mut(|c| changed(&mut out, &mut c.sent.size, window)) {
                for value in window {
                    out.int(value);
                }
            }
            let open = !self.java_world_has_input() && self.screen != Screen::Java;
            let native_screen = (open, open && self.pause);
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
                if c.sent.native_screen != Some(native_screen) {
                    c.sent.native_screen = Some(native_screen);
                    Input::Screen(native_screen.0, native_screen.1).write(&mut events);
                    count += 1;
                }
                // A window starts focused; an offscreen one never hears otherwise.
                let focused = c.focused.unwrap_or(true);
                if c.sent.focused != Some(focused) {
                    c.sent.focused = Some(focused);
                    Input::Focus(focused).write(&mut events);
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
                tick: out.0,
            };
            if open {
                let done = job.run(host);
                self.java_client_apply(done);
            } else {
                WORKER.with_borrow_mut(|w| w.get_or_insert_with(Worker::start).send(job));
            }
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
            // A status ends with whether the login's creative tabs wait to be taken.
            if tick.last() == Some(&1) {
                self.java_creative_tabs_built();
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
            if let Some(models) = done.models {
                self.java_models_apply(&models);
            }
            if let Some(renders) = done.renders {
                self.java_renders_apply(&renders);
            }
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

        /// The open Java screen, drawn as it extracted itself this frame.
        pub(in crate::game) fn draw_java_screen(&mut self, batch: &mut HudBatch) {
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
                self.java_screen_key_code(code, pressed, event.repeat, event.text.as_deref());
            }
        }

        pub(in crate::game) fn java_screen_key_code(
            &mut self,
            code: KeyCode,
            pressed: bool,
            repeat: bool,
            text: Option<&str>,
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
                inputs.push(Input::Key(name, action, modifiers));
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
}

/// `handleKeybinds`' key actions the Java client hears of as the window performs them.
#[derive(Clone, Copy)]
#[cfg_attr(not(feature = "java-mods"), allow(dead_code))]
pub(in crate::game) enum JavaKeyAction {
    Drop(bool),
    SwapHands,
    Hotbar(usize),
}

/// `Direction.get3DDataValue` of a face's normal.
#[cfg_attr(not(feature = "java-mods"), allow(dead_code))]
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

/// A click the window is about to act on, as `handleKeybinds` would start it.
#[derive(Clone, Copy)]
pub(in crate::game) enum JavaClick {
    Attack,
    Use,
    Pick,
}

#[cfg(not(feature = "java-mods"))]
impl Game<'_> {
    pub(super) fn java_click_canceled(&mut self, _click: JavaClick) -> Option<Option<Hand>> {
        None
    }
    pub(super) fn java_key_action(&mut self, _action: JavaKeyAction) {}
    pub(super) fn draw_java_screen(&mut self, _batch: &mut HudBatch) {}
    pub(super) fn java_screen_click(&mut self, _button: MouseButton, _pressed: bool) {}
    pub(super) fn java_screen_moved(&mut self) {}
    pub(super) fn java_screen_scroll(&mut self, _x: f64, _y: f64) {}
    pub(super) fn java_world_scroll(&self, _x: f64, _y: f64) -> bool {
        false
    }
    pub(super) fn java_screen_key(&mut self, _event: &winit::event::KeyEvent) {}
    pub(super) fn java_screen_key_code(
        &mut self,
        _code: KeyCode,
        _pressed: bool,
        _repeat: bool,
        _text: Option<&str>,
    ) {
    }
    pub(super) fn java_window_input(&mut self, _event: &winit::event::WindowEvent) {}
    pub(super) fn java_client_tick(&mut self) {}
    pub(super) fn java_client_left(&mut self) {}
    pub(super) fn java_client_font(&self) {}
}
