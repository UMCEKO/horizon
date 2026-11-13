use super::*;

impl Game<'_> {
    /// The vanilla screen a Java shell stands for: 1 the survival inventory, 2 the creative screen.
    pub(super) fn java_shell_kind(&self) -> u8 {
        if self.screen == Screen::Creative {
            return 2;
        }
        u8::from(
            self.screen == Screen::Inventory
                && self.container.is_none()
                && self.craft_size == 2
                && !self.mount_active()
                && self.station.is_none(),
        )
    }

    /// leftPos, topPos, imageWidth, imageHeight and the hovered menu slot, as native lays the screen out now.
    pub(super) fn java_shell_layout(&self) -> [i32; 5] {
        let batch = HudBatch::new(
            self.java_client_window_size(),
            self.settings.gui_scale_inputs(),
        );
        if self.screen == Screen::Creative {
            let (x, y) = Self::creative_origin(&batch);
            return [
                x.floor() as i32,
                y.floor() as i32,
                Creative::WIDTH as i32,
                Creative::HEIGHT as i32,
                -1,
            ];
        }
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

    /// The creative screen's selected tab and hovered slot, as `Input::ShellCreative` carries them.
    pub(super) fn java_creative_state(&self) -> CreativeState {
        let batch = HudBatch::new(
            self.java_client_window_size(),
            self.settings.gui_scale_inputs(),
        );
        let tab = self.creative_screen.tab();
        let trash = Self::creative_trash_hovered(&batch, self.cursor);
        let slot = self.slot_at(&batch, self.cursor.0, self.cursor.1);
        let first = self.creative_screen.first_row(&self.pack.names);
        let (hovered, index, stack) = creative_hover(tab.inventory, trash, slot, |cell| {
            self.creative_screen
                .shown(&self.pack.names, first * 9 + cell)
        });
        let stack = stack
            .and_then(|s| {
                super::super::java_mods::write_tag(horizon_items::item_nbt::item_to_nbt(&s))
            })
            .unwrap_or_default();
        (tab.id.clone(), hovered, index, stack)
    }

    pub(super) fn java_client_window_size(&self) -> [u32; 2] {
        self.renderer
            .as_ref()
            .map_or([854, 480], |r| [r.config.width, r.config.height])
    }

    /// A shell is the Java client's screen and native's screen of its kind is showing.
    pub(in crate::game) fn java_shell_open(&self) -> bool {
        let kind = CLIENT.with_borrow(|c| c.shell.kind);
        kind != 0 && kind == self.java_shell_kind()
    }

    /// Applies a `ShellReply`: its recording, its animation; returns who took the input.
    pub(super) fn java_shell_apply(&self, reply: &mut In) -> Option<ShellOwner> {
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
        self.java_shell_creative_inputs(&[], &mut out);
        CLIENT.with_borrow_mut(|c| c.sent.cursor = Some(at));
        if let Some(reply) = host.request("client_shell_frame", &out.0) {
            self.java_shell_apply(&mut In(&reply));
        }
    }

    pub(super) fn paint_shell_segment(&mut self, batch: &mut HudBatch, index: usize) {
        let frame = CLIENT.with_borrow(|c| c.shell.segments[index].clone());
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

    /// Native's own step, or Java's when Java drew it; then what mods drew after it.
    pub(in crate::game) fn shell_leaf(
        &mut self,
        batch: &mut HudBatch,
        leaf: Leaf,
        native: impl FnOnce(&mut Self, &mut HudBatch),
    ) {
        if !self.java_shell_open() {
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

    /// `int n` and the inputs, led by the creative screen's state when it differs from what Java last heard.
    pub(super) fn java_shell_creative_inputs(&self, inputs: &[Input], out: &mut Out) {
        let state = (self.screen == Screen::Creative).then(|| self.java_creative_state());
        let state = state.filter(|state| {
            CLIENT.with_borrow_mut(|c| {
                let differs = c.sent.creative.as_ref() != Some(state);
                if differs {
                    c.sent.creative = Some(state.clone());
                }
                differs
            })
        });
        out.int(inputs.len() as i32 + i32::from(state.is_some()));
        if let Some((tab, hovered, index, stack)) = &state {
            Input::ShellCreative(tab, *hovered, *index, stack).write(out);
        }
        for input in inputs {
            input.write(out);
        }
    }
}
