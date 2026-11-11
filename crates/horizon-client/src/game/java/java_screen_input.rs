// game-module-cfg: feature java-mods
//! What the Java client's screens ask of native's open inventory and connection: which keys and clicks native keeps.

use super::*;

impl Game<'_> {
    /// Whether native's inventory acts on a key (`AbstractContainerScreen.keyPressed`, the recipe book's search box) or the window does: Escape, the inventory key, a hotbar, swap or drop key over a slot, any key while the book's search is focused, and the window's own function keys and modifiers.
    pub(in crate::game) fn native_inventory_key(&self, code: KeyCode) -> bool {
        use KeyCode::*;
        if self.recipe_search_focused()
            || matches!(
                code,
                Escape
                    | F1
                    | F2
                    | F3
                    | F4
                    | F5
                    | F6
                    | F7
                    | F8
                    | F9
                    | F10
                    | F11
                    | F12
                    | ShiftLeft
                    | ShiftRight
                    | ControlLeft
                    | ControlRight
                    | AltLeft
                    | AltRight
                    | SuperLeft
                    | SuperRight
            )
        {
            return true;
        }
        let hovered = self.renderer.as_ref().is_some_and(|r| {
            let batch = HudBatch::new(
                [r.config.width, r.config.height],
                self.settings.gui_scale_inputs(),
            );
            self.slot_at(&batch, self.cursor.0, self.cursor.1).is_some()
        });
        self.settings
            .keys
            .translate_all(code)
            .into_iter()
            .any(|mapped| match mapped {
                Ok(KeyE) => true,
                Ok(KeyT) => self.recipe_search_focused(),
                Ok(key) => hovered && (HOTBAR_DIGITS.contains(&key) || matches!(key, KeyF | KeyQ)),
                Err(_) => false,
            })
    }

    /// Whether GUI point (`x`, `y`) is on the recipe book's button or its open panel.
    pub(in crate::game) fn recipe_book_hit(&self, x: f32, y: f32) -> bool {
        let Some(book) = self.active_book() else {
            return false;
        };
        let Some(batch) = self.batch_for_size() else {
            return false;
        };
        let (ox, _) = self.inventory_origin(&batch);
        let (bx, by) = self.book_button_pos(book, ox, &batch);
        let inside =
            |[left, top, w, h]: [f32; 4]| x >= left && y >= top && x < left + w && y < top + h;
        if inside([bx, by, 20.0, 18.0]) {
            return true;
        }
        // The tabs hang 30 pixels left of the panel.
        self.recipe_book_rect(&batch)
            .is_some_and(|[left, top, w, h]| inside([left - 30.0, top, w + 30.0, h]))
    }

    /// Whether the cursor is over a bundle in a slot that selects in it.
    pub(in crate::game) fn bundle_under_cursor(&mut self) -> bool {
        let Some(slot) = self.inventory_target().0 else {
            return false;
        };
        if !self.bundle_slot(slot) {
            return false;
        }
        let stack = self.slot_mut(slot).clone();
        let items = self.items();
        stack.is_some_and(|s| items.get(&s.item).is_some_and(|i| i.bundle))
    }

    /// The joined server's profile of this window's player: its uuid and name.
    pub(in crate::game) fn remote_profile(&self) -> Option<(u128, String)> {
        let r = self.remote.as_ref().filter(|r| r.joined)?;
        Some((r.profile(), r.profile_name.clone()))
    }
}
