//! Clicks on the server's open menu, sent as `MultiPlayerGameMode.handleContainerInput` does, and its local close.
use super::*;

impl Game<'_> {
    pub(super) fn send_clicks(&mut self, container: i32, clicks: Vec<Predicted>) {
        // A result the engine claims to have changed is the one the server
        // agrees to (`click_agreed`), so it sends none: keep it. A result the
        // engine doesn't claim (a recipe it doesn't know) the server sends.
        if matches!(
            self.remote_menu().1.as_str(),
            "inventory" | "minecraft:crafting"
        ) && let Some(r) = self.remote.as_mut()
        {
            for (.., changed, _) in &clicks {
                if let Some((_, result)) = changed.iter().find(|(slot, _)| *slot == 0) {
                    r.craft_result = result.clone();
                }
            }
        }
        let state = self.remote.as_ref().map_or(0, |r| r.state_id);
        let hashed = |g: &Self, s: &Option<ItemStack>| g.hashed_stack(s);
        for (slot, button, input, changed, carried) in clicks {
            let changed = changed.iter().map(|(i, s)| (*i, hashed(self, s))).collect();
            let carried = hashed(self, &carried);
            self.send(S::ContainerClick(play::ContainerClick {
                container,
                state,
                slot,
                button: button as i8,
                input: input_ordinal(input),
                changed,
                carried,
            }));
            self.java_menu_click(container, i32::from(slot), button, input_ordinal(input));
        }
    }
    /// A click outside a server's window (slot -999): predicted like
    /// `MultiPlayerGameMode.handleContainerInput` (the carried stack after
    /// `doClick`; the server drops the stack and swings the hand).
    pub(in crate::game) fn remote_throw(&mut self, right: bool, input: horizon_client::menu::Input) {
        use horizon_client::menu::Input;
        let container = self
            .remote
            .as_ref()
            .and_then(|r| r.menu.as_ref())
            .map_or(0, |m| m.0);
        if input == Input::Pickup
            && let Some(c) = self.carried.as_mut()
        {
            if right && c.count > 1 {
                c.count -= 1;
            } else {
                self.carried = None;
            }
            self.swing_arm();
        }
        let carried = self.hashed_stack(&self.carried);
        let state = self.remote.as_ref().map_or(0, |r| r.state_id);
        let input = if input == Input::Throw { 4 } else { 0 };
        self.send(S::ContainerClick(play::ContainerClick {
            container,
            state,
            slot: -999,
            button: i8::from(right),
            input,
            changed: Vec::new(),
            carried,
        }));
        self.java_menu_click(container, -999, i32::from(right), input);
    }
    pub(super) fn remote_close_local(&mut self) {
        self.java_menu_closed();
        // `Gui.setScreen` clears the focus of the screen it takes down: the
        // container's keyboard focus must not outlive it (a Tab bound to the
        // inventory moves the focus and closes the container in one press).
        self.pad_clear_focus();
        self.recipe_ui.reset_screen();
        if let Some(r) = self.remote.as_mut() {
            r.menu.set(None);
            r.set_menu_title(None);
            r.craft_result = None;
        }
        self.mount_menu_reset();
        // The client's menu has no level access: what it holds is the server's.
        self.station = None;
        // AbstractContainerMenu.removed: the server gives the grid and the
        // carried stack back to the inventory and syncs it; the copies here
        // are stale the moment the window closes.
        self.craft_grid.fill(None);
        self.carried = None;
        self.craft_size = 2;
        if let Some(key) = self.container.take() {
            // Server containers stay renderable but lose their copied slots.
            if let Some(entity) = self.world.block_entities.get_mut(&key) {
                for s in entity.slots_mut() {
                    *s = None;
                }
            }
        }
    }
    /// The server's open menu is the inventory or a chest-like container
    /// (chests, barrels, ender chests: `generic_9xN`; shulker boxes).
    pub(in crate::game) fn remote_storage_open(&self) -> bool {
        match self.remote.as_ref().and_then(|r| r.menu.as_ref()) {
            None => true,
            Some((_, name)) => {
                name == "inventory"
                    || name.starts_with("minecraft:generic_9x")
                    || name == "minecraft:shulker_box"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_click_kind_goes_out_as_the_ordinal_vanilla_declares() {
        use super::super::input_ordinal;
        use horizon_client::menu::Input;
        // ContainerInput: PICKUP, QUICK_MOVE, SWAP, CLONE, THROW, QUICK_CRAFT, PICKUP_ALL.
        for (name, ordinal) in [
            ("PICKUP", 0),
            ("QUICK_MOVE", 1),
            ("SWAP", 2),
            ("CLONE", 3),
            ("THROW", 4),
            ("QUICK_CRAFT", 5),
            ("PICKUP_ALL", 6),
        ] {
            assert_eq!(input_ordinal(Input::parse(name).unwrap()), ordinal, "{name}");
        }
    }
}
