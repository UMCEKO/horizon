// game-module-cfg: feature java-mods
//! W12 SPEC-16: while a menu route is on, a vanilla menu native opened is the real server menu on the player's Java view.

use super::Game;
use super::java_mods::{read_tag, write_tag};
use horizon_client::items::ItemStack;
use horizon_client::lan::menu::Slots;
use horizon_java_host::wire::{In, Out};
use std::collections::{HashMap, HashSet};

/// `ItemStack#overrideOtherStackedOnMe`, which `doClick` calls with the menu's slot.
const STACKED_ON_ME: &str = "net/minecraft/world/item/ItemStack#overrideOtherStackedOnMe(";
const MENU_CLASS: &str = "net/minecraft/world/inventory/AbstractContainerMenu";

/// A vanilla menu type's class chain below `AbstractContainerMenu` (which every chain ends in).
fn menu_classes(menu: &str) -> Option<&'static [&'static str]> {
    macro_rules! chain {
        ($($class:literal),+) => {{
            const CHAIN: &[&str] = &[$(concat!("net/minecraft/world/inventory/", $class)),+];
            CHAIN
        }};
    }
    Some(match menu.strip_prefix("minecraft:")? {
        "generic_9x1" | "generic_9x2" | "generic_9x3" | "generic_9x4" | "generic_9x5"
        | "generic_9x6" => chain!("ChestMenu"),
        "generic_3x3" => chain!("DispenserMenu"),
        "crafter_3x3" => chain!("CrafterMenu"),
        "shulker_box" => chain!("ShulkerBoxMenu"),
        "hopper" => chain!("HopperMenu"),
        "furnace" => chain!("FurnaceMenu", "AbstractFurnaceMenu", "RecipeBookMenu"),
        "blast_furnace" => chain!("BlastFurnaceMenu", "AbstractFurnaceMenu", "RecipeBookMenu"),
        "smoker" => chain!("SmokerMenu", "AbstractFurnaceMenu", "RecipeBookMenu"),
        "brewing_stand" => chain!("BrewingStandMenu"),
        "beacon" => chain!("BeaconMenu"),
        "crafting" => chain!("CraftingMenu", "AbstractCraftingMenu", "RecipeBookMenu"),
        "anvil" => chain!("AnvilMenu", "ItemCombinerMenu"),
        "smithing" => chain!("SmithingMenu", "ItemCombinerMenu"),
        "grindstone" => chain!("GrindstoneMenu"),
        "stonecutter" => chain!("StonecutterMenu"),
        "loom" => chain!("LoomMenu"),
        "cartography_table" => chain!("CartographyTableMenu"),
        "enchantment" => chain!("EnchantmentMenu"),
        "lectern" => chain!("LecternMenu"),
        "merchant" => chain!("MerchantMenu"),
        _ => return None,
    })
}

/// Whether an on route's mixin reaches a menu of this type: an owner in its class chain, or the stack hook `doClick` calls.
pub(super) fn menu_routed<'a>(menu: &str, on: impl IntoIterator<Item = &'a str>) -> bool {
    let Some(chain) = menu_classes(menu) else {
        return false;
    };
    on.into_iter().any(|key| {
        let owner = key.split('#').next().unwrap_or(key);
        key.starts_with(STACKED_ON_ME) || owner == MENU_CLASS || chain.contains(&owner)
    })
}

fn menu_routed_now(menu: &str) -> bool {
    horizon_java_host::routes::get().is_some_and(|routes| {
        menu_routed(
            menu,
            routes.iter().filter(|(_, r)| r.on).map(|(_, r)| &*r.key),
        )
    })
}

/// Where Java makes the menu from (`menu_open`'s `source`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MenuSource {
    /// The block entity at the position is the `MenuProvider`.
    BlockEntity = 0,
    /// `BlockState.getMenuProvider` at the position.
    Block = 1,
}

/// The acting player's open menu as its view's fill needs it: only state the inventory and block entities don't hold.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct MenuSnapshot {
    pub(super) container_id: i32,
    pub(super) carried: Option<ItemStack>,
    /// Menu slots whose container is the menu's own, by menu slot index.
    pub(super) slots: Vec<(i32, Option<ItemStack>)>,
    /// Data slots the menu owns, by index.
    pub(super) data: Vec<(i32, i32)>,
}

/// What the view's menu last heard, per player and menu id.
#[derive(Debug, Default)]
pub(super) struct SentMenu {
    container_id: i32,
    carried: Option<ItemStack>,
    slots: HashMap<i32, Option<ItemStack>>,
    data: HashMap<i32, i32>,
}

thread_local! {
    /// Per acting player's uuid: the container id of the menu its Java view holds.
    static MENU_VIEWS: std::cell::RefCell<HashMap<[i32; 4], i32>> = Default::default();
    /// Menu types without a block, reported once each.
    static UNVIEWED: std::cell::RefCell<HashSet<String>> = Default::default();
}

/// Forgets the menu views of one player, or of all, whose Java player is gone.
pub(super) fn forget_menu_views(uuid: Option<[i32; 4]>) {
    MENU_VIEWS.with_borrow_mut(|views| match uuid {
        Some(uuid) => {
            views.remove(&uuid);
        }
        None => views.clear(),
    });
}

fn write_stack(stack: &Option<ItemStack>, out: &mut Out) -> Option<()> {
    match stack {
        Some(stack) => out.bytes(&write_tag(horizon_client::save::item_to_nbt(stack))?),
        None => out.int(0),
    }
    Some(())
}

fn read_stack(reply: &mut In<'_>) -> Option<Option<ItemStack>> {
    let bytes = reply.bytes()?;
    if bytes.is_empty() {
        return Some(None);
    }
    Some(horizon_client::save::item_from_nbt(&read_tag(bytes)?))
}

/// The MENU group (`int container_id, bytes carried, n x (int slot, bytes stack), m x (int data, int value)`) when it differs from `sent`, which then holds `now`; Some(false) when unchanged, None when a stack can't be encoded.
pub(super) fn encode_menu_group(
    now: &MenuSnapshot,
    sent: &mut Option<SentMenu>,
    full: bool,
    out: &mut Out,
) -> Option<bool> {
    let fresh = full
        || sent
            .as_ref()
            .is_none_or(|m| m.container_id != now.container_id);
    let last = match sent {
        Some(last) if !fresh => last,
        _ => sent.insert(SentMenu {
            container_id: now.container_id,
            ..SentMenu::default()
        }),
    };
    let slots: Vec<_> = now
        .slots
        .iter()
        .filter(|(i, stack)| fresh || last.slots.get(i) != Some(stack))
        .collect();
    let data: Vec<_> = now
        .data
        .iter()
        .filter(|(i, value)| fresh || last.data.get(i) != Some(value))
        .collect();
    if !fresh && last.carried == now.carried && slots.is_empty() && data.is_empty() {
        return Some(false);
    }
    out.int(now.container_id);
    write_stack(&now.carried, out)?;
    last.carried.clone_from(&now.carried);
    out.int(slots.len() as i32);
    for (i, stack) in slots {
        out.int(*i);
        write_stack(stack, out)?;
        last.slots.insert(*i, stack.clone());
    }
    out.int(data.len() as i32);
    for (i, value) in data {
        out.int(*i);
        out.int(*value);
        last.data.insert(*i, *value);
    }
    Some(true)
}

/// A reply's menu block (`MenuViews.writeChanges`): what Java's menu holds now.
#[derive(Debug, PartialEq)]
struct MenuReply {
    container_id: i32,
    carried: Option<ItemStack>,
    slots: Vec<(i32, Option<ItemStack>)>,
    data: Vec<(i32, i32)>,
}

fn read_menu_reply(reply: &mut In<'_>) -> Option<Option<MenuReply>> {
    let container_id = reply.int()?;
    if container_id == -1 {
        return Some(None);
    }
    let carried = read_stack(reply)?;
    let slots = (0..reply.int()?.max(0))
        .map(|_| Some((reply.int()?, read_stack(reply)?)))
        .collect::<Option<Vec<_>>>()?;
    let data = (0..reply.int()?.max(0))
        .map(|_| Some((reply.int()?, reply.int()?)))
        .collect::<Option<Vec<_>>>()?;
    Some(Some(MenuReply {
        container_id,
        carried,
        slots,
        data,
    }))
}

impl SentMenu {
    fn applied(&mut self, reply: &MenuReply) {
        self.carried.clone_from(&reply.carried);
        for (i, stack) in &reply.slots {
            self.slots.insert(*i, stack.clone());
        }
        for (i, value) in &reply.data {
            self.data.insert(*i, *value);
        }
    }
}

impl Game<'_> {
    /// The game's own menu engine behind window `id` of guest `conn`: a crafting table's or a workstation's.
    fn java_engine_menu(&mut self, conn: u32, id: i32) -> Option<&mut horizon_client::menu::Menu> {
        let play = self.lan.as_mut()?.play.get_mut(&conn)?;
        if let Some((window, _, menu, _)) = play.crafting.table.as_mut()
            && *window == id
        {
            return Some(menu);
        }
        play.station
            .as_mut()
            .filter(|s| s.window == id)
            .map(|s| &mut *s.menu)
    }

    /// The acting guest's open menu, when its Java view holds one.
    pub(super) fn java_menu_snapshot(&mut self) -> Option<MenuSnapshot> {
        let id = MENU_VIEWS.with_borrow(|views| views.get(&self.acting_uuid()).copied())?;
        let conn = self.lan_acting?;
        let carried = self.lan.as_ref()?.hub.guest(conn)?.carried.clone();
        let (slots, data) = match self.java_engine_menu(conn, id) {
            Some(menu) => (
                (0..menu.slot_count())
                    .filter(|&i| !menu.slot_is_players(i))
                    .map(|i| (i as i32, menu.slot(i).cloned()))
                    .collect(),
                menu.data
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (i as i32, *v))
                    .collect(),
            ),
            // A block entity's container and data come through its own view.
            None => (Vec::new(), Vec::new()),
        };
        Some(MenuSnapshot {
            container_id: id,
            carried,
            slots,
            data,
        })
    }

    /// The menu block of a reply into native's menu: the carried stack, the menu's own slots and data slots.
    pub(super) fn java_apply_menu(&mut self, reply: &mut In<'_>) -> Option<()> {
        let Some(menu) = read_menu_reply(reply)? else {
            return Some(());
        };
        let uuid = self.acting_uuid();
        if MENU_VIEWS.with_borrow(|views| views.get(&uuid).copied()) != Some(menu.container_id) {
            return Some(());
        }
        let Some(conn) = self.lan_acting else {
            return Some(());
        };
        if let Some(host) = self.lan.as_mut() {
            host.hub
                .update_guest(conn, |g| g.carried.clone_from(&menu.carried));
        }
        if let Some(engine) = self.java_engine_menu(conn, menu.container_id) {
            for (i, stack) in &menu.slots {
                let i = *i as usize;
                if i < engine.slot_count() && !engine.slot_is_players(i) {
                    engine.load_slot(i, stack.clone());
                }
            }
            for &(i, value) in &menu.data {
                if (i as usize) < engine.data.len() {
                    engine.set_data(i as usize, value);
                }
            }
        }
        super::java_mods::with_sent(uuid, |sent| {
            if let Some(last) = sent
                .menu
                .as_mut()
                .filter(|m| m.container_id == menu.container_id)
            {
                last.applied(&menu);
            }
        });
        Some(())
    }

    /// Where Java makes the view of guest `conn`'s menu `window` from; None for a menu it gets no view of.
    fn java_menu_source(
        &self,
        conn: u32,
        window: i32,
        pos: Option<[i32; 3]>,
    ) -> Option<MenuSource> {
        let host = self.lan.as_ref()?;
        if let Some(m) = host
            .hub
            .guest(conn)?
            .menu
            .as_ref()
            .filter(|m| m.window == window)
        {
            if m.mod_type.is_some() {
                return None;
            }
            return match m.slots {
                Slots::Block(_) => Some(MenuSource::BlockEntity),
                // The block's provider joins both halves in vanilla's order.
                Slots::Double(..) => Some(MenuSource::Block),
                Slots::Ender | Slots::Native => None,
            };
        }
        pos.map(|_| MenuSource::Block)
    }

    /// Native opened vanilla menu `menu` as window `window` for the acting guest `conn`: its Java view gets the real menu.
    pub(super) fn java_menu_open(
        &mut self,
        conn: u32,
        window: i32,
        menu: &str,
        pos: Option<[i32; 3]>,
    ) {
        if horizon_java_host::get().is_none()
            || self.lan_acting != Some(conn)
            || !menu_routed_now(menu)
        {
            return;
        }
        let Some(source) = self.java_menu_source(conn, window, pos) else {
            if UNVIEWED.with_borrow_mut(|types| types.insert(menu.to_owned())) {
                log::info!("Horizon: Java mods get no view of menu {menu}: it has no block");
            }
            return;
        };
        let [x, y, z] = pos.unwrap_or_default();
        let uuid = self.acting_uuid();
        MENU_VIEWS.with_borrow_mut(|views| views.insert(uuid, window));
        super::java_mods::with_sent(uuid, |sent| sent.menu = None);
        self.java_bind_states();
        let Some(mut request) = self.java_use_request() else {
            MENU_VIEWS.with_borrow_mut(|views| views.remove(&uuid));
            return;
        };
        let mut tail = Out::default();
        tail.int(window);
        tail.text(menu);
        tail.0.push(source as u8);
        tail.long(horizon_core::position::BlockPos::new(x, y, z).pack());
        request.extend(tail.0);
        let applied = self
            .java_player_request("menu_open", &request)
            .and_then(|reply| self.java_apply_use_tail(&mut In(&reply)));
        if applied.is_none() {
            log::warn!("Horizon: Java's view of menu {menu} answered in pieces");
            MENU_VIEWS.with_borrow_mut(|views| views.remove(&uuid));
        }
        self.java_drain_block_events();
    }

    /// Native closed window `window` of the acting guest: its Java view's menu is the inventory's again.
    pub(super) fn java_menu_close(&mut self, window: i32) {
        let uuid = self.acting_uuid();
        let viewed = MENU_VIEWS.with_borrow_mut(|views| {
            let open = views.get(&uuid) == Some(&window);
            if open {
                views.remove(&uuid);
            }
            open
        });
        if !viewed || horizon_java_host::get().is_none() {
            return;
        }
        super::java_mods::with_sent(uuid, |sent| sent.menu = None);
        self.java_bind_states();
        let Some(mut request) = self.java_use_request() else {
            return;
        };
        let mut tail = Out::default();
        tail.int(window);
        request.extend(tail.0);
        let applied = self
            .java_player_request("menu_close", &request)
            .and_then(|reply| self.java_apply_use_tail(&mut In(&reply)));
        if applied.is_none() {
            log::warn!("Horizon: Java's view of a closed menu answered in pieces");
        }
        self.java_drain_block_events();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sword() -> ItemStack {
        ItemStack {
            item: "minecraft:iron_sword".into(),
            count: 1,
            damage: 0,
            extra: None,
        }
    }

    fn tag(stack: &ItemStack) -> Vec<u8> {
        write_tag(horizon_client::save::item_to_nbt(stack)).unwrap()
    }

    fn anvil() -> MenuSnapshot {
        MenuSnapshot {
            container_id: 3,
            carried: None,
            slots: vec![(0, None), (1, None), (2, None)],
            data: vec![(0, 0)],
        }
    }

    fn encode(now: &MenuSnapshot, sent: &mut Option<SentMenu>, full: bool) -> Option<Vec<u8>> {
        let mut out = Out::default();
        encode_menu_group(now, sent, full, &mut out)
            .unwrap()
            .then_some(out.0)
    }

    #[test]
    fn menu_open_is_full() {
        let mut want = Out::default();
        want.int(3);
        want.int(0);
        want.int(3);
        for slot in 0..3 {
            want.int(slot);
            want.int(0);
        }
        want.int(1);
        want.int(0);
        want.int(0);
        assert_eq!(encode(&anvil(), &mut None, false), Some(want.0.clone()));
        let mut sent = None;
        encode(&anvil(), &mut sent, false);
        assert_eq!(
            encode(&anvil(), &mut sent, true),
            Some(want.0),
            "a full player fill"
        );
        let mut other = anvil();
        other.container_id = 4;
        let first = encode(&other, &mut sent, false).unwrap();
        assert_eq!(first[..4], [0, 0, 0, 4], "another menu id starts over");
        assert_eq!(first.len(), encode(&other, &mut None, false).unwrap().len());
    }

    #[test]
    fn menu_group_delta() {
        let mut sent = None;
        let mut now = anvil();
        encode(&now, &mut sent, false);
        assert_eq!(encode(&now, &mut sent, false), None, "nothing changed");
        now.carried = Some(sword());
        let mut want = Out::default();
        want.int(3);
        want.bytes(&tag(&sword()));
        want.int(0);
        want.int(0);
        assert_eq!(encode(&now, &mut sent, false), Some(want.0));
        now.slots[0].1 = Some(sword());
        now.data[0].1 = 5;
        let mut want = Out::default();
        want.int(3);
        want.bytes(&tag(&sword()));
        want.int(1);
        want.int(0);
        want.bytes(&tag(&sword()));
        want.int(1);
        want.int(0);
        want.int(5);
        assert_eq!(encode(&now, &mut sent, false), Some(want.0));
    }

    #[test]
    fn applied_reply_is_not_sent_back() {
        let mut sent = None;
        let mut now = anvil();
        encode(&now, &mut sent, false);
        let mut reply = Out::default();
        reply.int(3);
        reply.bytes(&tag(&sword()));
        reply.int(1);
        reply.int(2);
        reply.bytes(&tag(&sword()));
        reply.int(1);
        reply.int(0);
        reply.int(7);
        let read = read_menu_reply(&mut In(&reply.0)).unwrap().unwrap();
        sent.as_mut().unwrap().applied(&read);
        now.carried = Some(sword());
        now.slots[2].1 = Some(sword());
        now.data[0].1 = 7;
        assert_eq!(encode(&now, &mut sent, false), None);
    }

    #[test]
    fn no_menu_block_reads_as_none() {
        let mut reply = Out::default();
        reply.int(-1);
        reply.int(42);
        let mut reply = In(&reply.0);
        assert_eq!(read_menu_reply(&mut reply), Some(None));
        assert_eq!(reply.int(), Some(42), "nothing more is read");
    }

    #[test]
    fn menu_routes_follow_the_class_chain() {
        let do_click = "net/minecraft/world/inventory/AbstractContainerMenu#doClick(IILnet/minecraft/world/inventory/ContainerInput;Lnet/minecraft/world/entity/player/Player;)V";
        let anvil_result = "net/minecraft/world/inventory/AnvilMenu#createResultInternal()V";
        let stacked = "net/minecraft/world/item/ItemStack#overrideOtherStackedOnMe(Lnet/minecraft/world/item/ItemStack;Lnet/minecraft/world/inventory/Slot;Lnet/minecraft/world/inventory/ClickAction;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/entity/SlotAccess;)Z";
        assert!(menu_routed("minecraft:generic_9x3", [do_click]));
        assert!(menu_routed("minecraft:anvil", [anvil_result]));
        assert!(!menu_routed("minecraft:generic_9x3", [anvil_result]));
        assert!(!menu_routed("minecraft:smithing", [anvil_result]));
        assert!(menu_routed("minecraft:crafting", [stacked]));
        assert!(!menu_routed(
            "minecraft:anvil",
            ["net/minecraft/world/entity/player/Inventory#setSelectedSlot(I)V"]
        ));
        assert!(
            !menu_routed("horizon_java_test:machine", [do_click]),
            "a mod's menu is not vanilla's"
        );
    }
}
