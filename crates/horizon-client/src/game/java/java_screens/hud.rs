use super::*;

/// Whether this frame records the HUD: only while it animates, and not on the frame that took a tick's status or while a tick runs.
pub(super) fn hud_frame_wanted(animating: bool, recorded: bool, busy: bool) -> bool {
    animating && !recorded && !busy
}

/// Whether a mod listens to `RenderFrameEvent` and the Java client is up to post it.
fn java_frame_events() -> bool {
    CLIENT.with_borrow(|c| c.frame_events && c.joined)
}

/// A frame recording's reply: whether the HUD still animates, and the recording if it differs from the last.
pub(super) fn java_hud_frame_apply(reply: &[u8], spent: u128) {
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
    /// This frame's HUD recording request: the window's size and its progress into the next tick.
    pub(super) fn java_hud_frame_request(&mut self) -> Vec<u8> {
        let mut out = Out::default();
        self.java_client_size(&mut out);
        out.float(self.clock.fraction() as f32);
        out.0
    }

    /// `RenderFrameEvent.Pre` (phase 0) or `.Post` (1) in the Java client, only while a mod listens; its sounds play as a tick's do.
    pub(in crate::game) fn java_render_frame(&mut self, phase: u8) {
        if !java_frame_events() {
            return;
        }
        let Some(host) = horizon_java_host::get() else {
            return;
        };
        let mut out = Out::default();
        out.0.push(phase);
        out.float(self.clock.fraction() as f32);
        if let Some(reply) = host.request("client_render_frame", &out.0)
            && !reply.is_empty()
        {
            self.java_client_sounds(&mut In(&reply));
        }
    }

    /// A window frame's collection of the Java client's finished work, without waiting for it, and the next frame recording's request while the HUD or the world animates; a frame that took a tick's status, or a tick still running, asks for none, so the window never waits on Java.
    pub(in crate::game) fn java_client_frame(&mut self) {
        self.java_render_frame(0);
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
        let toasts = self.java_toasts_frame_request(busy);
        if (hud.is_some() || world.is_some() || toasts.is_some())
            && horizon_java_host::get().is_some()
        {
            WORKER.with_borrow_mut(|w| {
                w.get_or_insert_with(Worker::start)
                    .send_frame(FrameJob { hud, world, toasts });
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

    pub(super) fn paint_hud_segment(&mut self, batch: &mut HudBatch, index: usize) {
        let frame = CLIENT.with_borrow(|c| c.hud.segments[index].clone());
        if frame.len() > 4 {
            paint_frame(
                batch,
                &self.gui.atlas,
                &frame,
                [0.0, 0.0],
                |batch, reply| self.java_screen_item(batch, reply),
            );
        }
    }

    /// Native's vanilla layer, unless `gui_layer_pre` cancelled it or Java drew it; then what mods drew after it.
    pub(in crate::game) fn hud_layer<R>(
        &mut self,
        batch: &mut HudBatch,
        layer: Option<HudLayer>,
        canceled: bool,
        native: impl FnOnce(&mut Self, &mut HudBatch) -> R,
    ) -> Option<R> {
        let Some(layer) = layer else {
            return (!canceled).then(|| native(self, batch));
        };
        let drawn = CLIENT.with_borrow_mut(|c| {
            c.hud_painted |= 1 << layer as u32;
            c.hud.drawn
        });
        let (drawn_by_native, drawn_by_java) = hud_layer_draws(canceled, drawn, layer);
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
}
