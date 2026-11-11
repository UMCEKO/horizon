//! The Java client's window shims: cfg-free for their call sites, empty without `--features java-mods`.

use super::*;

/// The draw steps of a container screen, in vanilla's order; ids are wire values shared with `ShellLeaves.java`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub(in crate::game) enum Leaf {
    Start,
    Background,
    Effects,
    Widgets,
    Labels,
    HighlightBack,
    Slots,
    HighlightFront,
    RecipeBook,
    Carried,
    Tooltip,
    Deferred,
    Extra,
    BookTooltip,
}

/// The HUD layers of `VanillaGuiLayers`, in vanilla's order; ids are wire values shared with `HudLayers.java`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub(in crate::game) enum HudLayer {
    CameraOverlays,
    Crosshair,
    AfterCameraDecorations,
    Hotbar,
    PlayerHealth,
    ArmorLevel,
    FoodLevel,
    VehicleHealth,
    AirLevel,
    ContextualInfoBarBackground,
    ExperienceLevel,
    ContextualInfoBar,
    SelectedItemName,
    SpectatorTooltip,
    Effects,
    BossOverlay,
    SleepOverlay,
    DemoOverlay,
    ScoreboardSidebar,
    OverlayMessage,
    Title,
    Chat,
    TabList,
    SubtitleOverlay,
}

impl HudLayer {
    pub(in crate::game) const ALL: [HudLayer; 24] = [
        HudLayer::CameraOverlays,
        HudLayer::Crosshair,
        HudLayer::AfterCameraDecorations,
        HudLayer::Hotbar,
        HudLayer::PlayerHealth,
        HudLayer::ArmorLevel,
        HudLayer::FoodLevel,
        HudLayer::VehicleHealth,
        HudLayer::AirLevel,
        HudLayer::ContextualInfoBarBackground,
        HudLayer::ExperienceLevel,
        HudLayer::ContextualInfoBar,
        HudLayer::SelectedItemName,
        HudLayer::SpectatorTooltip,
        HudLayer::Effects,
        HudLayer::BossOverlay,
        HudLayer::SleepOverlay,
        HudLayer::DemoOverlay,
        HudLayer::ScoreboardSidebar,
        HudLayer::OverlayMessage,
        HudLayer::Title,
        HudLayer::Chat,
        HudLayer::TabList,
        HudLayer::SubtitleOverlay,
    ];

    /// The vanilla layer a `gui_layer` id names; a mod's layer is none.
    pub(in crate::game) fn of(id: &str) -> Option<HudLayer> {
        const IDS: [&str; 24] = [
            "minecraft:camera_overlays",
            "minecraft:crosshair",
            "minecraft:after_camera_decorations",
            "minecraft:hotbar",
            "minecraft:player_health",
            "minecraft:armor_level",
            "minecraft:food_level",
            "minecraft:vehicle_health",
            "minecraft:air_level",
            "minecraft:contextual_info_bar_background",
            "minecraft:experience_level",
            "minecraft:contextual_info_bar",
            "minecraft:selected_item_name",
            "minecraft:spectator_tooltip",
            "minecraft:effects",
            "minecraft:boss_overlay",
            "minecraft:sleep_overlay",
            "minecraft:demo_overlay",
            "minecraft:scoreboard_sidebar",
            "minecraft:overlay_message",
            "minecraft:title",
            "minecraft:chat",
            "minecraft:tab_list",
            "minecraft:subtitle_overlay",
        ];
        IDS.iter().position(|i| *i == id).map(|at| HudLayer::ALL[at])
    }
}

/// `handleKeybinds`' key actions the Java client hears of as the window performs them.
#[derive(Clone, Copy)]
pub(in crate::game) enum JavaKeyAction {
    Drop(#[cfg_attr(not(feature = "java-mods"), expect(dead_code, reason = "only the Java client reads it"))] bool),
    SwapHands,
    Hotbar(#[cfg_attr(not(feature = "java-mods"), expect(dead_code, reason = "only the Java client reads it"))] usize),
}

/// A click the window is about to act on, as `handleKeybinds` would start it.
#[derive(Clone, Copy)]
pub(in crate::game) enum JavaClick {
    Attack,
    Use,
    Pick,
}

impl Game<'_> {
    /// This tick's picked positions Java runs `animateTick` at, which the tick's end sends.
    pub(super) fn java_animate_picked(&mut self, picked: Vec<[i32; 3]>) {
        #[cfg(feature = "java-mods")]
        self.java_set_animate_picked(picked);
        #[cfg(not(feature = "java-mods"))]
        let _ = picked;
    }

    /// The Java client ran the click first and a mod canceled it: the window swings as it said and acts no further.
    pub(in crate::game) fn java_click_handled(&mut self, click: JavaClick) -> bool {
        let Some(swing) = self.java_click_canceled(click) else {
            return false;
        };
        if let Some(hand) = swing {
            self.swing_hand(hand, horizon_client::first_person::SwingAnimation::DEFAULT);
        }
        true
    }

    /// Whether the Java shell hears the window's input: it is open and no prompt, dialog or closing save is over it.
    pub(in crate::game) fn java_shell_accepts(&self) -> bool {
        self.java_shell_open()
            && self.pending_save.is_none()
            && !self.dialog_open()
            && self.link_prompt.is_none()
            && self.code_of_conduct.is_none()
            && self.server_packs.prompt.is_none()
    }
}

#[cfg(not(feature = "java-mods"))]
impl Game<'_> {
    pub(super) fn java_click_canceled(&mut self, _click: JavaClick) -> Option<Option<Hand>> {
        None
    }
    pub(super) fn java_block_prediction(
        &mut self,
        _pos: [i32; 3],
    ) -> Option<horizon_world::level::InteractionResult> {
        None
    }
    pub(super) fn java_key_action(&mut self, _action: JavaKeyAction) {}
    pub(super) fn draw_java_screen(&mut self, _batch: &mut HudBatch) {}
    pub(super) fn java_screen_blurred(&self) -> bool {
        false
    }
    pub(super) fn hud_start(&mut self, _batch: &mut HudBatch) {}
    pub(super) fn hud_end(&mut self, _batch: &mut HudBatch) {}
    pub(super) fn hud_layer<R>(
        &mut self,
        batch: &mut HudBatch,
        _layer: Option<HudLayer>,
        native: impl FnOnce(&mut Self, &mut HudBatch) -> R,
    ) -> Option<R> {
        Some(native(self, batch))
    }
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
        _layout: Option<char>,
    ) {
    }
    pub(super) fn java_window_input(&mut self, _event: &winit::event::WindowEvent) {}
    pub(super) fn java_key_input(&mut self, _code: KeyCode, _pressed: bool, _repeat: bool) {}
    pub(super) fn java_client_tick_send(&mut self) {}
    pub(super) fn java_client_frame(&mut self) {}
    pub(super) fn java_client_left(&mut self) {}
    pub(super) fn java_client_font(&self) {}
    pub(super) fn java_remote_joined(&mut self, _java: bool) {}
    pub(super) fn java_apply_mod_keys(&self) {}
    pub(super) fn java_wire_in(&mut self, _frame: &[u8]) {}
    pub(super) fn java_shell_open(&self) -> bool {
        false
    }
    pub(super) fn java_shell_frame(&mut self) {}
    pub(super) fn shell_leaf(
        &mut self,
        batch: &mut HudBatch,
        _leaf: Leaf,
        native: impl FnOnce(&mut Self, &mut HudBatch),
    ) {
        native(self, batch);
    }
    pub(super) fn java_shell_mouse(&mut self, _button: MouseButton, _pressed: bool) -> bool {
        true
    }
    pub(super) fn java_shell_wheel(&mut self, _x: f64, _y: f64) -> bool {
        true
    }
    pub(super) fn java_shell_key(&mut self, _event: &winit::event::KeyEvent) -> bool {
        true
    }
}
