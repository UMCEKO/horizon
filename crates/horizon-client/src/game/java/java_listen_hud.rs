// game-module-cfg: feature java-mods
// java-listener-family
//! `RenderGuiLayerEvent.Pre` on `gui_layer_pre`: a Java mod's cancel hides the vanilla layer by the hook's contract.

use super::java_hooks_client::HudLayer;
use super::java_listener::Family;
use mod_api::{ClientHost, Flow, GuiLayerPreEvent, HookId};
use std::sync::OnceLock;

const HOOKS: &[HookId] = &[HookId::GuiLayerPre];
static SUBSCRIBED: OnceLock<Vec<Option<HookId>>> = OnceLock::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(Hud),
    }
}

struct Hud;

impl mod_api::Hooks for Hud {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED)
    }

    // The Java client's own HUD pass posts the event on its thread as it records the frame; posting it again here would run its listeners twice and wait on the JVM every frame.
    fn gui_layer_pre(&self, _: &dyn ClientHost, event: &mut GuiLayerPreEvent<'_>) -> Flow {
        verdict(
            HudLayer::of(event.layer),
            super::java_screens::live::java_hud_layer_cancelled,
        )
    }
}

/// The recorded verdict of a vanilla layer; a layer Java does not record is never its to cancel.
fn verdict(layer: Option<HudLayer>, cancelled: impl FnOnce(HudLayer) -> bool) -> Flow {
    match layer {
        Some(layer) if cancelled(layer) => Flow::Cancel,
        _ => Flow::Continue,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_recorded_cancel_cancels_the_bar_by_its_gui_layer_id() {
        let food = |layer| layer == HudLayer::FoodLevel;
        assert_eq!(
            verdict(HudLayer::of("minecraft:food_level"), food),
            Flow::Cancel
        );
        assert_eq!(
            verdict(HudLayer::of("minecraft:player_health"), food),
            Flow::Continue
        );
        assert_eq!(verdict(HudLayer::of("compass:bar"), |_| true), Flow::Continue);
    }
}
