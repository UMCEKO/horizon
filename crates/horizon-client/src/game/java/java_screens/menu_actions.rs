//! What mods do through a shell reaches native: the Java client's packets on native's menus, performed as native's own UI would, and a mod's change to the screen's input state.
use super::*;
use horizon_client::container_screen::Button;
use horizon_client::menu::Input as Click;
use horizon_protocol::packets::play::{self, Serverbound as Sb};

/// A click on the open menu as `menu_ops` takes it: the game slot (`None` outside), the button, the input.
pub(super) type Op = (Option<usize>, i32, Click);

/// `AbstractContainerScreen`'s input fields native models (`Clicks` and the quick craft drag): `isQuickCrafting`, `quickCraftingType`, `quickCraftingButton`, `quickCraftSlots` as menu indices, `skipNextRelease`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::game) struct ShellState {
    quick_crafting: bool,
    kind: i32,
    button: i32,
    slots: Vec<i32>,
    skip_next_release: bool,
}

impl ShellState {
    pub(super) fn write(&self, out: &mut Out) {
        out.bool(self.quick_crafting);
        out.int(self.kind);
        out.int(self.button);
        out.int(self.slots.len() as i32);
        self.slots.iter().for_each(|slot| out.int(*slot));
        out.bool(self.skip_next_release);
    }

    fn read(reply: &mut In) -> Option<Self> {
        let quick_crafting = reply.bool()?;
        let kind = reply.int()?;
        let button = reply.int()?;
        let mut slots = Vec::new();
        for _ in 0..reply.int()? {
            slots.push(reply.int()?);
        }
        Some(Self {
            quick_crafting,
            kind,
            button,
            slots,
            skip_next_release: reply.bool()?,
        })
    }

    /// Native's state as Java's fields hold it; the slots sorted, as Java's set has no order.
    fn of(
        quick_crafting: Option<Button>,
        skip_next_release: bool,
        drag: Option<&(i32, Vec<usize>)>,
        to_menu: impl Fn(usize) -> Option<usize>,
    ) -> Self {
        let (kind, mut slots) = drag.map_or((0, Vec::new()), |(kind, slots)| {
            let slots = slots
                .iter()
                .filter_map(|slot| to_menu(*slot))
                .map(|slot| slot as i32);
            (*kind, slots.collect())
        });
        slots.sort_unstable();
        Self {
            quick_crafting: quick_crafting.is_some(),
            kind,
            button: quick_crafting.map_or(0, Button::menu_button),
            slots,
            skip_next_release,
        }
    }

    /// `Clicks`' quick craft button and skip, and the drag in game slots, as a mod left them; no drag once `isQuickCrafting` is false.
    fn native(
        &self,
        to_game: impl Fn(i32) -> Option<usize>,
    ) -> (Option<Button>, bool, Option<(i32, Vec<usize>)>) {
        let button = match self.button {
            0 => Some(Button::Left),
            1 => Some(Button::Right),
            2 => Some(Button::Middle),
            _ => None,
        };
        let quick = button.filter(|_| self.quick_crafting);
        let drag = quick.map(|_| {
            let slots = self.slots.iter().filter_map(|slot| to_game(*slot));
            (self.kind, slots.collect())
        });
        (quick, self.skip_next_release, drag)
    }
}

/// A reply's or a status's outbox: `int n`, then each packet's bytes (VarInt id, then its body).
pub(super) fn read_outbox(reply: &mut In) -> Option<Vec<Vec<u8>>> {
    let mut packets = Vec::new();
    for _ in 0..reply.int()? {
        packets.push(reply.bytes()?.to_vec());
    }
    Some(packets)
}

/// A reply's `bool changed`, then the state a mod left when it changed one.
pub(super) fn read_state_change(reply: &mut In) -> Option<Option<ShellState>> {
    if reply.bool()? {
        ShellState::read(reply).map(Some)
    } else {
        Some(None)
    }
}

/// A packet of the outbox as native's serverbound codec reads it.
fn decode(bytes: &[u8]) -> horizon_protocol::Result<Sb> {
    let mut reader = horizon_protocol::buf::Reader::new(bytes);
    let id = reader.varint()?;
    Sb::decode(id, &bytes[bytes.len() - reader.remaining()..])
}

/// `ContainerInput` by its ordinal.
fn click_input(ordinal: i32) -> Option<Click> {
    Some(match ordinal {
        0 => Click::Pickup,
        1 => Click::QuickMove,
        2 => Click::Swap,
        3 => Click::Clone,
        4 => Click::Throw,
        5 => Click::QuickCraft,
        6 => Click::PickupAll,
        _ => return None,
    })
}

/// A click in sending order, as `AbstractContainerMenu.clicked` takes a quick craft: its start and added slots are kept, and all of them go at its end (header 2 of `getQuickcraftHeader`).
fn quick_craft_step(kept: &mut Vec<Op>, op: Op) -> Option<Vec<Op>> {
    if op.2 != Click::QuickCraft {
        return Some(vec![op]);
    }
    match op.1 & 3 {
        0 => {
            kept.clear();
            kept.push(op);
            None
        }
        2 => {
            kept.push(op);
            Some(std::mem::take(kept))
        }
        _ => {
            kept.push(op);
            None
        }
    }
}

fn debug(what: std::fmt::Arguments) {
    if std::env::var_os("HORIZON_JAVA_DEBUG").is_some() {
        log::info!("Horizon: {what}");
    }
}

impl Game<'_> {
    /// The open shell's input state, while a mod reads it: the inventory's and a mirrored menu's, not creative's.
    pub(super) fn java_shell_state(&self) -> Option<ShellState> {
        if !CLIENT.with_borrow(|c| c.shell_state_wanted) || !matches!(self.java_shell_kind(), 1 | 3)
        {
            return None;
        }
        let (_, menu) = self.remote_menu();
        Some(ShellState::of(
            self.inv_clicks.quick_crafting(),
            self.inv_clicks.skips_next_release(),
            self.inv_drag.as_ref(),
            |slot| Self::menu_slot_index(&menu, slot),
        ))
    }

    /// A mod changed the screen's input state: native's `Clicks` and drag take it before acting on the input.
    pub(super) fn java_shell_state_back(&mut self, state: &ShellState) {
        let (_, menu) = self.remote_menu();
        let (quick, skip, drag) = state.native(|slot| Self::inventory_screen_slot(&menu, slot));
        self.inv_clicks.restore(quick, skip);
        self.inv_drag = drag;
    }

    /// The Java client's packets on native's menus, in its order, each done as native's own UI does it and sent as native sends it.
    pub(super) fn java_perform(&mut self, packets: Vec<Vec<u8>>) {
        if packets.is_empty() {
            return;
        }
        CLIENT.with_borrow_mut(|c| c.performing = true);
        for bytes in packets {
            match decode(&bytes) {
                Ok(packet) => self.java_perform_one(packet),
                Err(e) => log::warn!("Horizon: a mod's menu packet didn't read: {e}"),
            }
        }
        CLIENT.with_borrow_mut(|c| c.performing = false);
    }

    /// Whether `container` is the menu native has open now (0, the inventory, always is).
    fn java_menu_open(&self, container: i32, what: &str) -> bool {
        let open = container == self.remote_menu().0 && self.remote.is_some();
        if !open {
            debug(format_args!(
                "a mod's {what} for menu {container}, which is not open, dropped"
            ));
        }
        open
    }

    fn java_perform_one(&mut self, packet: Sb) {
        match packet {
            Sb::ContainerClick(p) => self.java_perform_click(&p),
            Sb::SelectBundleItem(p) => {
                let (_, menu) = self.remote_menu();
                match Self::inventory_screen_slot(&menu, p.slot) {
                    Some(slot) => self.select_bundle(slot, p.index),
                    None => debug(format_args!(
                        "a mod's bundle selection on slot {}, not one of the menu's",
                        p.slot
                    )),
                }
            }
            Sb::ContainerClose(p) => {
                if self.java_menu_open(p.container, "close") {
                    self.close_container();
                    self.set_screen(Screen::Playing);
                    self.capture(true);
                }
            }
            Sb::SetCreativeModeSlot(p) => self.java_perform_creative_slot(p),
            Sb::RenameItem(p) => {
                if let Some(s) = self
                    .station
                    .as_mut()
                    .filter(|s| s.kind == super::super::stations::Kind::Anvil)
                {
                    if !p.name.is_empty() {
                        s.name.clone_from(&p.name);
                        s.rename.set_value(&p.name);
                    }
                    s.anvil_sent = Some(p.name.clone());
                }
                self.send(Sb::RenameItem(p));
            }
            Sb::ContainerSlotStateChanged(p) => {
                if self.java_menu_open(p.container, "slot toggle") {
                    if let (Ok(slot), "minecraft:crafter_3x3") =
                        (usize::try_from(p.slot), self.remote_menu().1.as_str())
                    {
                        self.menu_session(|menu, _, _| {
                            menu.set_slot_state(slot, p.enabled);
                            Some(true)
                        });
                    }
                    self.send(Sb::ContainerSlotStateChanged(p));
                }
            }
            Sb::ContainerButtonClick(p) => {
                if self.java_menu_open(p.container, "button") {
                    self.send(Sb::ContainerButtonClick(p));
                }
            }
            Sb::PlaceRecipe(p) => {
                if self.java_menu_open(p.container, "recipe") {
                    self.send(Sb::PlaceRecipe(p));
                }
            }
            packet @ (Sb::SelectTrade(_) | Sb::SetBeacon(_)) => self.send(packet),
            other => debug(format_args!(
                "a mod's {} is no menu packet, dropped",
                other.name()
            )),
        }
    }

    /// `AbstractContainerMenu.clicked` through native's own click path: predicted on native's menu and sent, a quick craft whole at its end.
    fn java_perform_click(&mut self, p: &play::ContainerClick) {
        if !self.java_menu_open(p.container, "click") {
            return;
        }
        let (_, menu) = self.remote_menu();
        let slot = match p.slot {
            -999 => None,
            slot => match Self::inventory_screen_slot(&menu, i32::from(slot)) {
                Some(slot) => Some(slot),
                None => {
                    return debug(format_args!(
                        "a mod's click on slot {slot}, not one of {menu}'s"
                    ));
                }
            },
        };
        let Some(input) = click_input(p.input) else {
            return debug(format_args!("a mod's click of unknown input {}", p.input));
        };
        let op = (slot, i32::from(p.button), input);
        let Some(ops) = CLIENT.with_borrow_mut(|c| quick_craft_step(&mut c.quick_craft, op)) else {
            return;
        };
        if self.screen == Screen::Creative {
            self.menu_ops(&ops);
            return;
        }
        // BundleMouseActions.onSlotClicked, as `menu_ops` runs it first.
        for &(slot, _, input) in &ops {
            if let (Some(slot), Click::QuickMove | Click::Swap) = (slot, input) {
                self.unselect_bundle(slot);
            }
        }
        self.remote_menu_ops(&ops);
    }

    /// `handleCreativeModeItemAdd` (or, for slot -1, `handleCreativeModeItemDrop`): the inventory slot takes the stack and goes out as native's creative screen sends it.
    fn java_perform_creative_slot(&mut self, p: play::SetCreativeModeSlot) {
        let stack = self.to_stack(&p.item);
        if p.slot == -1 {
            if let Some(stack) = stack {
                self.remote_creative_throw(stack);
            }
            return;
        }
        let Some(slot) = Self::inventory_screen_slot("inventory", i32::from(p.slot)) else {
            return debug(format_args!(
                "a mod's creative slot {} is not the inventory's",
                p.slot
            ));
        };
        *self.slot_mut(slot) = stack;
        self.send_creative_slots();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chest_menu_slot(slot: i32) -> Option<usize> {
        Game::inventory_screen_slot("minecraft:generic_9x3", slot)
    }

    #[test]
    fn an_outbox_click_reads_back_as_the_container_click_it_was() {
        let click = Sb::ContainerClick(play::ContainerClick {
            container: 3,
            state: 7,
            slot: 30,
            button: 0,
            input: 1,
            changed: Vec::new(),
            carried: None,
        });
        let mut out = Out::default();
        out.int(1);
        out.bytes(&click.encode());
        let packets = read_outbox(&mut In(&out.0)).unwrap();
        let Ok(Sb::ContainerClick(read)) = decode(&packets[0]) else {
            panic!("not a click");
        };
        assert_eq!(
            (read.container, read.slot, read.button, read.input),
            (3, 30, 0, 1)
        );
        assert_eq!(click_input(read.input), Some(Click::QuickMove));
        // A 27-slot chest's slot 30 is the main inventory's fourth: game slot 12.
        assert_eq!(chest_menu_slot(i32::from(read.slot)), Some(12));
        assert_eq!(chest_menu_slot(4), Some(304));
    }

    #[test]
    fn a_quick_craft_waits_for_its_end_then_goes_whole_in_order() {
        let mut kept = Vec::new();
        let start = (None, 1 << 2, Click::QuickCraft);
        let add_a = (Some(300), 1 | (1 << 2), Click::QuickCraft);
        let add_b = (Some(301), 1 | (1 << 2), Click::QuickCraft);
        let end = (None, 2 | (1 << 2), Click::QuickCraft);
        for op in [start, add_a, add_b] {
            assert!(quick_craft_step(&mut kept, op).is_none());
        }
        assert_eq!(
            quick_craft_step(&mut kept, end),
            Some(vec![start, add_a, add_b, end])
        );
        assert!(kept.is_empty());
        let pickup = (Some(5), 0, Click::Pickup);
        assert_eq!(quick_craft_step(&mut kept, pickup), Some(vec![pickup]));
    }

    #[test]
    fn a_new_quick_craft_start_drops_an_unfinished_one() {
        let mut kept = vec![(Some(300), 1, Click::QuickCraft)];
        let start = (None, 0, Click::QuickCraft);
        assert!(quick_craft_step(&mut kept, start).is_none());
        assert_eq!(kept, vec![start]);
    }

    #[test]
    fn the_input_state_round_trips_in_the_order_java_reads() {
        let drag = (1, vec![305, 302, 12]);
        let state = ShellState::of(Some(Button::Right), false, Some(&drag), |slot| {
            Game::menu_slot_index("minecraft:generic_9x3", slot)
        });
        assert_eq!(state.slots, vec![2, 5, 30]);
        let mut out = Out::default();
        state.write(&mut out);
        let mut read = In(&out.0);
        assert_eq!(
            (read.bool(), read.int(), read.int()),
            (Some(true), Some(1), Some(1))
        );
        assert_eq!(read.int(), Some(3));
        assert_eq!(
            (read.int(), read.int(), read.int()),
            (Some(2), Some(5), Some(30))
        );
        assert_eq!(read.bool(), Some(false));
        assert!(read.byte().is_none());
        let mut reply = Out::default();
        reply.bool(true);
        state.write(&mut reply);
        assert_eq!(read_state_change(&mut In(&reply.0)), Some(Some(state)));
    }

    #[test]
    fn a_mod_ending_the_quick_craft_clears_natives_drag() {
        let ended = ShellState {
            quick_crafting: false,
            kind: 1,
            button: 1,
            slots: vec![2, 5],
            skip_next_release: true,
        };
        let (quick, skip, drag) = ended.native(chest_menu_slot);
        assert_eq!((quick, skip, drag), (None, true, None));
        let running = ShellState {
            quick_crafting: true,
            ..ended
        };
        let (quick, _, drag) = running.native(chest_menu_slot);
        assert_eq!(quick, Some(Button::Right));
        assert_eq!(drag, Some((1, vec![302, 305])));
    }

    #[test]
    #[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
    fn a_mods_quick_move_on_the_inventory_moves_the_stack_and_is_not_replayed() {
        crate::game::native_entity_refs::tests::with_client_game(|game| {
            let (pipe, _server) = horizon_protocol::connection::Pipe::pair();
            game.connect_local(pipe, "ShellClick").unwrap();
            game.screen = Screen::Inventory;
            game.inventory[0] = Some(ItemStack::new("minecraft:cobblestone", 64));
            CLIENT.with_borrow_mut(|c| {
                c.joined = true;
                c.shell.kind = 1;
            });
            // `handleContainerInput(0, 36, 0, QUICK_MOVE)`: the hotbar's first slot of the inventory menu.
            let click = Sb::ContainerClick(play::ContainerClick {
                container: 0,
                state: 0,
                slot: 36,
                button: 0,
                input: 1,
                changed: Vec::new(),
                carried: None,
            });
            game.java_perform(vec![click.encode()]);
            let replayed = CLIENT.with_borrow_mut(|c| std::mem::take(&mut c.queue));
            CLIENT.with_borrow_mut(|c| *c = Client::default());
            assert!(replayed.is_empty(), "the mod's own click went back to Java");
            assert!(game.inventory[0].is_none());
            let moved = game.inventory[9..36]
                .iter()
                .flatten()
                .any(|s| &*s.item == "minecraft:cobblestone" && s.count == 64);
            assert!(moved, "{:?}", &game.inventory[9..36]);
        });
    }

    #[test]
    fn a_menu_input_kind_is_the_one_events_reads() {
        let state = ShellState::default();
        let mut out = Out::default();
        Input::ShellState(&state).write(&mut out);
        let mut read = In(&out.0);
        assert_eq!(read.byte(), Some(23));
        assert_eq!(ShellState::read(&mut read), Some(state));
    }
}
