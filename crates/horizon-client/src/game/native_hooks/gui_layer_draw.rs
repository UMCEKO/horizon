use super::*;

/// The hook points. Each does nothing in Horizon.
impl Game<'_> {
    /// Draw one HUD layer a mod registered (`native_register_gui_layers`), between its
    /// `native_gui_layer_pre` and `native_gui_layer_post`; not asked when a mod hid it.
    pub(in crate::game) fn native_gui_layer_draw(&self, layer: &str, draw: &mut HudBatch) {
        self.fire_gui_layer_draw(layer, draw);
    }
    /// The HUD pass starts: the mods' layers draw among vanilla's while it shows, after what Java mods drew first.
    pub(in crate::game) fn hud_begin(&mut self, show_hud: bool, batch: &mut HudBatch) {
        self.hud_layer_next = show_hud.then_some(0);
        if show_hud {
            self.hud_start(batch);
        }
    }
    /// The HUD pass ends: what Java mods drew after layers not drawn, then the mod layers above the last vanilla one.
    pub(in crate::game) fn hud_finish(&mut self, show_hud: bool, batch: &mut HudBatch) {
        if show_hud {
            self.hud_end(batch);
        }
        self.draw_mod_gui_layers(None, batch);
        self.hud_layer_next = None;
    }
    /// Draws one vanilla HUD layer between the mods' hooks: first the mods' layers
    /// placed below it (`draw_mod_gui_layers`), then `native_gui_layer_pre` (which
    /// may hide it), the layer, then `native_gui_layer_post`. `None` when a mod hid
    /// the layer.
    pub(in crate::game) fn gui_layer<R>(
        &mut self,
        layer: &str,
        batch: &mut HudBatch,
        draw: impl FnOnce(&mut Self, &mut HudBatch) -> R,
    ) -> Option<R> {
        self.draw_mod_gui_layers(Some(layer), batch);
        let mut canceled = false;
        self.native_gui_layer_pre(layer, batch, &mut canceled);
        let java = crate::game::java_hooks_client::HudLayer::of(layer);
        let drawn = if canceled {
            None
        } else {
            self.hud_layer(batch, java, draw)
        };
        self.native_gui_layer_post(layer, batch);
        drawn
    }
    /// `GuiLayerManager.renderInner` for the mods' layers (`native_register_gui_layers`) not
    /// drawn yet this frame and placed below vanilla layer `until`, or for all that are left
    /// with `None`, at the HUD's end. Each fires `native_gui_layer_pre` (which may hide it),
    /// `native_gui_layer_draw`, then `native_gui_layer_post`, as a vanilla layer does. So a
    /// vanilla layer that is not drawn this frame keeps its place: the mods' layers around it
    /// draw at the next layer that is. Nothing while the HUD is hidden, and nothing to look up
    /// when no mod registered a layer.
    pub(in crate::game) fn draw_mod_gui_layers(&mut self, until: Option<&str>, batch: &mut HudBatch) {
        let order = horizon_client::gui_layers::order();
        let Some(next) = self.hud_layer_next.filter(|_| order.has_mod_layers()) else {
            return;
        };
        let end = match until {
            Some(layer) => match order.position(layer) {
                Some(at) => at,
                None => return,
            },
            None => order.len(),
        };
        for at in next..end {
            let Some(layer) = order.mod_layer(at) else {
                continue;
            };
            let mut canceled = false;
            self.native_gui_layer_pre(layer, batch, &mut canceled);
            if !canceled {
                self.native_gui_layer_draw(layer, batch);
            }
            self.native_gui_layer_post(layer, batch);
        }
        // Past `until` itself: it is drawn next, by `gui_layer`.
        self.hud_layer_next = Some(next.max(end + usize::from(until.is_some())));
    }
}
