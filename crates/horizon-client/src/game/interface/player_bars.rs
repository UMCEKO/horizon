//! The player's bars above the hotbar, one `gui_layer` each so pre, post and cancel act per bar.
use super::Game;
use horizon_client::hud::HudBatch;
use horizon_client::hud_layout::{self, GuiDraw, HealthInputs};

/// One of the bars `HealthHud` lays out on its own.
#[derive(Clone, Copy)]
enum Bar {
    Armor,
    Food,
    Air,
}

impl Game<'_> {
    /// The bar layers in registration order; `health` is `None` in creative or spectator.
    pub(super) fn draw_player_bars(
        &mut self,
        batch: &mut HudBatch,
        health: Option<(HealthInputs, Option<Vec<GuiDraw>>)>,
        mount: Option<(i32, f32)>,
        (w, h): (i32, i32),
    ) {
        self.health_hud.start_bars();
        let mut own = None;
        if let Some((inputs, native)) = &health {
            let replaced = native.is_some();
            self.gui_layer("minecraft:player_health", batch, |g, batch| {
                let laid_out;
                let draws = match native {
                    Some(draws) => draws,
                    None => {
                        laid_out = g.health_hud.health(inputs, w, h);
                        &laid_out
                    }
                };
                hud_layout::paint(batch, &g.gui.atlas, draws);
            });
            own = Some((!replaced).then_some(inputs));
        }
        self.player_bar("minecraft:armor_level", batch, own, Bar::Armor, (w, h));
        self.player_bar("minecraft:food_level", batch, own, Bar::Food, (w, h));
        if let Some((hearts, mount_health)) = mount {
            self.gui_layer("minecraft:vehicle_health", batch, |g, batch| {
                let draws = g.health_hud.vehicle(w, h, hearts, mount_health);
                hud_layout::paint(batch, &g.gui.atlas, &draws);
            });
        }
        self.player_bar("minecraft:air_level", batch, own, Bar::Air, (w, h));
        // Hud.playAirBubblePoppedSound: Player.playSound (PLAYERS), heard by this client only.
        if let Some((volume, pitch)) = self.health_hud.bubble_pop_sound.take()
            && let Some(p) = &self.player
        {
            let at = [p.position.x, p.position.y, p.position.z];
            let players = horizon_client::settings::SOUND_SOURCES
                .iter()
                .position(|s| *s == "player")
                .unwrap_or(0);
            let capture = self.audio.capture.take();
            self.audio
                .play_on(players, "ui.hud.bubble_pop", Some(at), volume, pitch);
            self.audio.capture = capture;
        }
    }

    /// One survival bar's layer: `Some(None)` fires it empty, as a `health_hud` answer drew it.
    fn player_bar(
        &mut self,
        layer: &str,
        batch: &mut HudBatch,
        own: Option<Option<&HealthInputs>>,
        bar: Bar,
        (w, h): (i32, i32),
    ) {
        let Some(own) = own else {
            return;
        };
        self.gui_layer(layer, batch, |g, batch| {
            let Some(inputs) = own else {
                return;
            };
            let draws = match bar {
                Bar::Armor => g.health_hud.armor(inputs, w, h),
                Bar::Food => g.health_hud.food(inputs, w, h),
                Bar::Air => g.health_hud.air(inputs, w, h),
            };
            hud_layout::paint(batch, &g.gui.atlas, &draws);
        });
    }
}
