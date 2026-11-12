//! The server's container menus on the Java client: their packets go to it while a mod needs vanilla's screen of the menu, and native's clicks replay on it.
use super::*;
use horizon_protocol::packets::play::Clientbound as C;

macro_rules! screen {
    ($name:literal) => {
        concat!("net/minecraft/client/gui/screens/inventory/", $name)
    };
}
macro_rules! menu {
    ($name:literal) => {
        concat!("net/minecraft/world/inventory/", $name)
    };
}

const CHEST: &[&str] = &[
    screen!("ContainerScreen"),
    screen!("AbstractContainerScreen"),
    menu!("ChestMenu"),
    menu!("AbstractContainerMenu"),
];
const DISPENSER: &[&str] = &[
    screen!("DispenserScreen"),
    screen!("AbstractContainerScreen"),
    menu!("DispenserMenu"),
    menu!("AbstractContainerMenu"),
];
const CRAFTER: &[&str] = &[
    screen!("CrafterScreen"),
    screen!("AbstractContainerScreen"),
    menu!("CrafterMenu"),
    menu!("AbstractContainerMenu"),
];
const HOPPER: &[&str] = &[
    screen!("HopperScreen"),
    screen!("AbstractContainerScreen"),
    menu!("HopperMenu"),
    menu!("AbstractContainerMenu"),
];
const SHULKER_BOX: &[&str] = &[
    screen!("ShulkerBoxScreen"),
    screen!("AbstractContainerScreen"),
    menu!("ShulkerBoxMenu"),
    menu!("AbstractContainerMenu"),
];
const CRAFTING: &[&str] = &[
    screen!("CraftingScreen"),
    screen!("AbstractRecipeBookScreen"),
    screen!("AbstractContainerScreen"),
    menu!("CraftingMenu"),
    menu!("AbstractCraftingMenu"),
    menu!("RecipeBookMenu"),
    menu!("AbstractContainerMenu"),
];
const FURNACE: &[&str] = &[
    screen!("FurnaceScreen"),
    screen!("AbstractFurnaceScreen"),
    screen!("AbstractRecipeBookScreen"),
    screen!("AbstractContainerScreen"),
    menu!("FurnaceMenu"),
    menu!("AbstractFurnaceMenu"),
    menu!("RecipeBookMenu"),
    menu!("AbstractContainerMenu"),
];
const BLAST_FURNACE: &[&str] = &[
    screen!("BlastFurnaceScreen"),
    screen!("AbstractFurnaceScreen"),
    screen!("AbstractRecipeBookScreen"),
    screen!("AbstractContainerScreen"),
    menu!("BlastFurnaceMenu"),
    menu!("AbstractFurnaceMenu"),
    menu!("RecipeBookMenu"),
    menu!("AbstractContainerMenu"),
];
const SMOKER: &[&str] = &[
    screen!("SmokerScreen"),
    screen!("AbstractFurnaceScreen"),
    screen!("AbstractRecipeBookScreen"),
    screen!("AbstractContainerScreen"),
    menu!("SmokerMenu"),
    menu!("AbstractFurnaceMenu"),
    menu!("RecipeBookMenu"),
    menu!("AbstractContainerMenu"),
];

/// The menu types the Java client can mirror, each with the vanilla classes of its screen and menu (`MenuScreens`), up to the abstract bases.
const MENU_CLASSES: &[(&str, &[&str])] = &[
    ("minecraft:generic_9x1", CHEST),
    ("minecraft:generic_9x2", CHEST),
    ("minecraft:generic_9x3", CHEST),
    ("minecraft:generic_9x4", CHEST),
    ("minecraft:generic_9x5", CHEST),
    ("minecraft:generic_9x6", CHEST),
    ("minecraft:generic_3x3", DISPENSER),
    ("minecraft:crafter_3x3", CRAFTER),
    ("minecraft:hopper", HOPPER),
    ("minecraft:shulker_box", SHULKER_BOX),
    ("minecraft:crafting", CRAFTING),
    ("minecraft:furnace", FURNACE),
    ("minecraft:blast_furnace", BLAST_FURNACE),
    ("minecraft:smoker", SMOKER),
];

const CLIENT_LISTENER: &str = "net/minecraft/client/multiplayer/ClientPacketListener";
const CLIENT_LISTENER_METHODS: &[&str] = &[
    "handleOpenScreen",
    "handleContainerContent",
    "handleContainerSetSlot",
    "handleContainerSetData",
    "handleContainerClose",
];
const CLIENT_MODE: &str = "net/minecraft/client/multiplayer/MultiPlayerGameMode";
const CLIENT_MODE_METHOD: &str = "handleContainerInput";

/// Whether the Java client can build the vanilla screen of this menu type.
pub(super) fn mirrorable(menu_type: &str) -> bool {
    MENU_CLASSES.iter().any(|(name, _)| *name == menu_type)
}

/// A route key (`pkg/Class#method(desc)`) as its class and method name.
fn route_parts(key: &str) -> Option<(&str, &str)> {
    let (class, rest) = key.split_once('#')?;
    Some((class, rest.split('(').next().unwrap_or(rest)))
}

/// Whether a route key patches `MultiPlayerGameMode.handleContainerInput`, which every click native replays runs.
fn input_patched(key: &str) -> bool {
    route_parts(key) == Some((CLIENT_MODE, CLIENT_MODE_METHOD))
}

/// Whether a route key patches something the Java client runs for this menu type.
fn route_touches(menu_type: &str, key: &str) -> bool {
    let Some((class, method)) = route_parts(key) else {
        return false;
    };
    match class {
        CLIENT_LISTENER => CLIENT_LISTENER_METHODS.contains(&method),
        CLIENT_MODE => method == CLIENT_MODE_METHOD,
        _ => MENU_CLASSES
            .iter()
            .any(|(name, classes)| *name == menu_type && classes.contains(&class)),
    }
}

/// Whether the Java client mirrors a menu of this type: it is one of the table's, and a mod listens to screen events or an `on` route patches what the Java client would run for it.
fn menu_mirrored<'a>(
    screens_heard: bool,
    menu_type: &str,
    on_routes: impl IntoIterator<Item = &'a str>,
) -> bool {
    mirrorable(menu_type)
        && (screens_heard
            || on_routes
                .into_iter()
                .any(|key| route_touches(menu_type, key)))
}

/// The keys of the boot's routes that are on.
fn on_routes() -> impl Iterator<Item = &'static str> {
    horizon_java_host::routes::get()
        .into_iter()
        .flat_map(|routes| routes.iter())
        .filter(|(_, route)| route.on)
        .map(|(_, route)| &*route.key)
}

impl Game<'_> {
    /// Whether the Java client mirrors the server's menu of this type (`menu_mirrored` with what the last status and the boot's routes say).
    fn java_mirrors(&self, menu_type: &str) -> bool {
        let heard = CLIENT.with_borrow(|c| c.screens_heard);
        menu_mirrored(heard, menu_type, on_routes())
    }

    /// Forwards the server's menu packets to the Java client, in arrival order, while it mirrors the menu: it then builds what vanilla's client builds, from the same packets.
    pub(in crate::game) fn java_menu_packet(&mut self, packet: &C) {
        if !CLIENT.with_borrow(|c| c.joined) {
            return;
        }
        let mirrored = CLIENT.with_borrow(|c| c.mirrored);
        let forward = match packet {
            C::OpenScreen(p) => {
                let menu = self
                    .remote
                    .as_ref()
                    .and_then(|r| {
                        r.builtin
                            .get("minecraft:menu")?
                            .name(p.menu)
                            .map(str::to_owned)
                    })
                    .unwrap_or_default();
                let mirror = p.container != 0 && self.java_mirrors(&menu);
                if !mirror && std::env::var_os("HORIZON_JAVA_DEBUG").is_some() {
                    log::info!("Horizon: menu {menu} not mirrored");
                }
                CLIENT.with_borrow_mut(|c| c.mirrored = mirror.then_some(p.container));
                mirror
            }
            C::ContainerSetContent(p) => mirrored == Some(p.container),
            C::ContainerSetSlot(p) => mirrored == Some(p.container),
            C::ContainerSetData(p) => mirrored == Some(p.container),
            C::ContainerClose(p) => {
                let forward = mirrored == Some(p.container);
                if forward {
                    CLIENT.with_borrow_mut(|c| c.mirrored = None);
                }
                forward
            }
            C::SetCursorItem(_) => mirrored.is_some(),
            _ => false,
        };
        if forward {
            self.java_queue(Input::Packet(&packet.encode()));
        }
    }

    /// Native closed its screen: the menu is no longer the Java client's to mirror.
    pub(in crate::game) fn java_menu_closed(&mut self) {
        CLIENT.with_borrow_mut(|c| c.mirrored = None);
    }

    /// A click native made on a menu, replayed through the Java client's `handleContainerInput` while it mirrors that menu (or, for the inventory, while a shell is open or the method is patched).
    pub(in crate::game) fn java_menu_click(
        &mut self,
        container: i32,
        slot: i32,
        button: i32,
        input: i32,
    ) {
        let replay = CLIENT.with_borrow(|c| {
            c.joined
                && if container == 0 {
                    c.shell.kind != 0 || on_routes().any(input_patched)
                } else {
                    c.mirrored == Some(container)
                }
        });
        if replay {
            self.java_queue(Input::MenuClick(container, slot, button, input));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIRRORED: [&str; 14] = [
        "minecraft:generic_9x1",
        "minecraft:generic_9x2",
        "minecraft:generic_9x3",
        "minecraft:generic_9x4",
        "minecraft:generic_9x5",
        "minecraft:generic_9x6",
        "minecraft:generic_3x3",
        "minecraft:crafter_3x3",
        "minecraft:hopper",
        "minecraft:shulker_box",
        "minecraft:crafting",
        "minecraft:furnace",
        "minecraft:blast_furnace",
        "minecraft:smoker",
    ];

    #[test]
    fn every_menu_type_of_the_spec_has_a_row_and_nothing_else_does() {
        for name in MIRRORED {
            assert!(mirrorable(name), "{name}");
        }
        assert_eq!(MENU_CLASSES.len(), MIRRORED.len());
        for name in [
            "minecraft:anvil",
            "minecraft:brewing_stand",
            "horizon:mount_inventory",
            "inventory",
        ] {
            assert!(!mirrorable(name), "{name}");
        }
    }

    #[test]
    fn a_chest_row_names_its_screen_and_menu_down_to_the_abstract_bases() {
        let chest = MENU_CLASSES
            .iter()
            .find(|(n, _)| *n == "minecraft:generic_9x3")
            .unwrap()
            .1;
        for class in [
            "net/minecraft/client/gui/screens/inventory/ContainerScreen",
            "net/minecraft/client/gui/screens/inventory/AbstractContainerScreen",
            "net/minecraft/world/inventory/ChestMenu",
            "net/minecraft/world/inventory/AbstractContainerMenu",
        ] {
            assert!(chest.contains(&class), "{class}");
        }
        let furnace = MENU_CLASSES
            .iter()
            .find(|(n, _)| *n == "minecraft:smoker")
            .unwrap()
            .1;
        assert!(
            furnace.contains(&"net/minecraft/client/gui/screens/inventory/AbstractFurnaceScreen")
        );
        assert!(furnace.contains(&"net/minecraft/world/inventory/RecipeBookMenu"));
    }

    #[test]
    fn nothing_is_mirrored_with_no_listeners_and_no_routes() {
        for name in MIRRORED {
            assert!(!menu_mirrored(false, name, []), "{name}");
        }
    }

    #[test]
    fn a_listening_mod_mirrors_every_listed_menu_and_no_other() {
        for name in MIRRORED {
            assert!(menu_mirrored(true, name, []), "{name}");
        }
        assert!(!menu_mirrored(true, "minecraft:anvil", []));
    }

    #[test]
    fn a_patch_of_a_chest_screen_mirrors_only_chest_menus() {
        let routes = [
            "net/minecraft/client/gui/screens/inventory/ContainerScreen#extractLabels(Lnet/minecraft/client/gui/GuiGraphicsExtractor;II)V",
        ];
        for name in MIRRORED {
            let chest = name.starts_with("minecraft:generic_9x");
            assert_eq!(menu_mirrored(false, name, routes), chest, "{name}");
        }
    }

    #[test]
    fn a_patch_of_a_base_class_reaches_the_menus_built_on_it() {
        let screen =
            "net/minecraft/client/gui/screens/inventory/AbstractContainerScreen#slotClicked(I)V";
        let menu = "net/minecraft/world/inventory/RecipeBookMenu#clicked(I)V";
        assert!(menu_mirrored(false, "minecraft:furnace", [screen]));
        assert!(menu_mirrored(false, "minecraft:hopper", [screen]));
        assert!(menu_mirrored(false, "minecraft:crafting", [menu]));
        assert!(!menu_mirrored(false, "minecraft:hopper", [menu]));
    }

    #[test]
    fn a_patch_of_a_client_handler_mirrors_every_listed_menu() {
        let open = "net/minecraft/client/multiplayer/ClientPacketListener#handleOpenScreen(Lx;)V";
        let input = "net/minecraft/client/multiplayer/MultiPlayerGameMode#handleContainerInput(IIILy;Lz;)V";
        let other = "net/minecraft/client/multiplayer/ClientPacketListener#handleAddEntity(Lx;)V";
        for name in MIRRORED {
            assert!(menu_mirrored(false, name, [open]), "{name}");
            assert!(menu_mirrored(false, name, [input]), "{name}");
            assert!(!menu_mirrored(false, name, [other]), "{name}");
        }
        assert!(input_patched(input));
        assert!(!input_patched(open));
    }

    #[test]
    fn the_menu_inputs_write_what_events_reads() {
        let mut out = Out::default();
        Input::Packet(&[0x13, 1, 2]).write(&mut out);
        Input::MenuClick(5, -999, 1, 4).write(&mut out);
        Input::ShellBook(true).write(&mut out);
        let mut read = In(&out.0);
        assert_eq!(read.byte(), Some(20));
        assert_eq!(read.bytes(), Some(&[0x13u8, 1, 2][..]));
        assert_eq!(read.byte(), Some(21));
        assert_eq!(
            (read.int(), read.int(), read.int(), read.int()),
            (Some(5), Some(-999), Some(1), Some(4))
        );
        assert_eq!(read.byte(), Some(22));
        assert_eq!(read.bool(), Some(true));
        assert!(read.byte().is_none());
    }

    #[test]
    fn a_status_with_the_menu_shell_reads_kind_three() {
        let mut out = Out::default();
        out.bool(false);
        out.int(3);
        out.bool(false);
        out.int(0);
        out.bool(false);
        out.bool(true);
        let (_, kind, recording, _, heard) = read_shell_tail(&mut In(&out.0)).unwrap();
        assert!(kind == 3 && recording.is_none() && heard);
        assert!(matches!(shell_screen(kind), Some(Screen::Inventory)));
    }
}
