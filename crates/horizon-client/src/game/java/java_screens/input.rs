use super::*;

impl Game<'_> {
/// Whether the world has the window's input: no screen of the window's own is open over it.
pub(super) fn java_world_has_input(&self) -> bool {
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

pub(super) fn java_queue(&mut self, input: Input) {
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

/// Input over a shell: Java's screen events and mods' widgets first, in vanilla's order; native acts only when this says `Native`.
pub(super) fn java_shell_input(
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
pub(super) fn java_modifiers(&self) -> i32 {
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
pub(super) fn java_input(&mut self, inputs: &[Input]) {
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
pub(super) fn java_cursor(&self) -> Option<Input<'static>> {
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
