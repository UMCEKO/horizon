//! The `gui_layer_pre` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0144

pub trait Hooks {
    /// Before one of vanilla's HUD layers is drawn (`RenderGuiLayerEvent.Pre`):
    /// draw here to put something below the layer, or set `canceled` to hide
    /// it. `layer` is the layer's id, in the order Horizon draws them:
    /// `minecraft:crosshair`, `hotbar`, `effects`, `boss_overlay`, the bars
    /// `player_health`, `armor_level`, `food_level`, `vehicle_health` (riding a
    /// mount) and `air_level` (a hidden bar adds no height), `contextual_info_bar`
    /// (the jump bar, the locator bar or the experience bar with its level),
    /// `selected_item_name`, `sleep_overlay`, `overlay_message`
    /// (the action bar), `title`, `scoreboard_sidebar`, `subtitle_overlay`,
    /// `chat` and `tab_list`. A layer F1 hides fires nothing, and the chat
    /// screen draws the focused chat itself, so `chat` is only the unfocused
    /// lines. The camera overlays and the debug screen are not layers here:
    /// they are not drawn in this pass. Draw with `native_fill`, `native_text`
    /// and the sprite helpers; `draw.size()` is the GUI
    /// size. A layer a mod registered (`register_gui_layers`) fires this
    /// and `gui_layer_post` too, with its own id, in its place among
    /// these.
    #[hook(
        side = "client",
        contract = "cancel",
        out = "canceled",
        neoforge = "RenderGuiLayerEvent.Pre",
        cancellable = true
    )]
    fn gui_layer_pre();
}
