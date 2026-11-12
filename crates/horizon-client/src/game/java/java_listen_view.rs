// game-module-cfg: feature java-mods
// java-listener-family
//! `ViewportEvent.ComputeFov`, `CalculateDetachedCameraDistanceEvent` and `CalculatePlayerTurnEvent`: native's values go to the Java client with the tick, and the answer of its last firing applies.

use super::java_listener::{Family, Subscriptions};
use super::java_view;
use mod_api::{
    CalculatePlayerTurnEvent, CameraDistanceEvent, ClientHost, ComputeFovEvent, HookId, Listener,
};

const HOOKS: &[HookId] = &[
    HookId::ComputeFov,
    HookId::CameraDistance,
    HookId::CalculatePlayerTurn,
];
static SUBSCRIBED: Subscriptions = Subscriptions::new();

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(View),
    }
}

struct View;

/// A value the Java client answered, unless it is not one a camera can use.
fn answered(asked: f32, answer: Option<f32>) -> f32 {
    answer.filter(|v| v.is_finite()).unwrap_or(asked)
}

impl mod_api::Hooks for View {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED, false)
    }

    fn listeners(&self) -> &'static [Listener] {
        super::java_listener::listeners(HOOKS, &SUBSCRIBED, false)
    }

    fn compute_fov(&self, _: &dyn ClientHost, event: &mut ComputeFovEvent) {
        java_view::record_fov(event.degrees);
        event.degrees = answered(event.degrees, java_view::effects().fov);
    }

    fn camera_distance(&self, _: &dyn ClientHost, event: &mut CameraDistanceEvent<'_>) {
        let living = event.entity.living.as_ref();
        let scale = living.map_or(1.0, |l| l.scale);
        let distance = living.map_or(4.0, |l| l.camera_distance as f32);
        let (mount_scale, mount_distance) = event
            .entity
            .mount
            .map_or((scale, distance), |(s, d)| (s, d as f32));
        java_view::record_detached([scale, distance, mount_scale, mount_distance]);
        event.distance = answered(event.distance, java_view::effects().distance);
    }

    fn calculate_player_turn(&self, _: &dyn ClientHost, event: &mut CalculatePlayerTurnEvent) {
        if let Some((sensitivity, smooth)) = java_view::effects().turn
            && sensitivity.is_finite()
        {
            event.mouse_sensitivity = sensitivity;
            event.cinematic_camera_enabled = smooth;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_that_is_not_a_number_leaves_the_asked_value() {
        assert_eq!(answered(70.0, Some(35.0)), 35.0);
        assert_eq!(answered(70.0, Some(f32::NAN)), 70.0);
        assert_eq!(answered(70.0, None), 70.0);
    }
}
