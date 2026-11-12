// game-module-cfg: feature java-mods
//! What mods draw in the level's frame, as the Java client records it: applied from a tick's status, and recorded per window frame only while it changes.

use super::*;
use horizon_java_host::wire::{In, Out};
use std::cell::RefCell;

#[derive(Default)]
struct World {
    /// The Java client says the world is changing: each frame is recorded with its own timing until two alike.
    animating: bool,
    /// A tick's status was applied this frame: its recording is this frame's.
    recorded: bool,
    frames: (u32, u128),
}

thread_local! {
    static WORLD: RefCell<World> = RefCell::default();
}

/// Whether this frame records the world: only while it animates, and not on the frame that took a tick's status or while a tick runs.
fn world_frame_wanted(animating: bool, recorded: bool, busy: bool) -> bool {
    animating && !recorded && !busy
}

/// A tick's status: its recording when it changed, whether the world keeps changing, and what the custom sky, cloud and weather renderers replaced.
pub(super) fn java_world_status(blob: Option<&[u8]>, animating: bool, environment: i32) {
    if let Some(blob) = blob {
        horizon_client::world_renders::apply(blob);
    }
    horizon_client::world_renders::set_environment(environment);
    WORLD.with_borrow_mut(|w| {
        w.animating = animating;
        w.recorded = true;
    });
}

/// A status's attribute tail: whether a mod patched the probe, then its values by attribute, as the last tick and this one left them.
pub(super) fn java_probe_status(reply: &mut In) -> Option<()> {
    use horizon_client::environment::{Probed, ProbedValues};
    let routed = reply.bool()?;
    let mut values = Vec::new();
    for _ in 0..reply.int()? {
        let id = reply.text()?.to_owned();
        let tag = reply.int()?;
        fn pair<const N: usize>(reply: &mut In) -> Option<([f32; N], [f32; N])> {
            let mut read = || -> Option<[f32; N]> {
                let mut out = [0.0; N];
                for c in &mut out {
                    *c = reply.float()?;
                }
                Some(out)
            };
            Some((read()?, read()?))
        }
        let probed = match tag {
            0 => pair::<1>(reply).map(|(a, b)| Probed::Float(a[0], b[0])),
            1 => pair::<3>(reply).map(|(a, b)| Probed::Rgb(a, b)),
            2 => pair::<4>(reply).map(|(a, b)| Probed::Argb(a, b)),
            3 => pair::<1>(reply).map(|(a, b)| Probed::Angle(a[0], b[0])),
            _ => None,
        }?;
        values.push((id, probed));
    }
    horizon_client::world_renders::set_probed(
        (routed && !values.is_empty()).then_some(ProbedValues { values }),
    );
    Some(())
}

/// A frame recording's reply: whether the world still animates, and the recording if it differs from the last.
pub(super) fn java_world_frame_apply(reply: &[u8], spent: u128) {
    let mut reply = In(reply);
    let (Some(animating), Some(changed)) = (reply.bool(), reply.bool()) else {
        return;
    };
    if changed && let Some(blob) = reply.bytes() {
        horizon_client::world_renders::apply(blob);
    }
    WORLD.with_borrow_mut(|w| {
        w.frames.0 += 1;
        w.frames.1 += spent;
        if !animating {
            w.animating = false;
            if std::env::var_os("HORIZON_JAVA_DEBUG").is_some() {
                let (n, total) = w.frames;
                let (pieces, meshes) = horizon_client::world_renders::counts();
                log::info!(
                    "Horizon: world animated over {n} frames, {} µs per frame recording (request included), {pieces} pieces, {meshes} meshes",
                    total / u128::from(n.max(1))
                );
            }
            w.frames = (0, 0);
        }
    });
}

/// The camera Java is told: the locator's, else the aligned view's (backed off, turned round in front view); none before a view was aligned.
fn camera_for_java(
    view: &horizon_client::camera::Camera,
    fov: f32,
    far: f32,
    locator: Option<([f64; 3], f32, f32, f32)>,
) -> Option<([f64; 3], [f32; 4])> {
    if let Some((at, yaw, pitch, fov)) = locator {
        return Some((at, [yaw, pitch, fov, far]));
    }
    (view.position != [0.0; 3]).then_some((view.position, [view.y_rot, view.x_rot, fov, far]))
}

/// The window's world recordings and registry end with its server.
pub(super) fn java_world_left() {
    horizon_client::world_renders::clear();
    WORLD.with_borrow_mut(|w| *w = World::default());
}

impl Game<'_> {
    /// The camera the world is recorded from: where it is, how it looks, its field of view and far plane.
    pub(super) fn java_world_camera(&self) -> Option<([f64; 3], [f32; 4])> {
        let far =
            horizon_client::camera::depth_far(self.terrain_distance(), self.settings.cloud_range);
        let fov = self.fov_degrees(self.clock.fraction() as f32, self.view.position);
        camera_for_java(&self.view, fov, far, self.locator_camera).or_else(|| {
            self.eye_position()
                .map(|at| (at, [self.yaw, self.pitch, 70.0, far]))
        })
    }

    /// While the world animates, this frame's recording request as the level's frame would extract it; none on the frame that took a tick's status or while a tick runs.
    pub(super) fn java_world_frame_request(&mut self, busy: bool) -> Option<Vec<u8>> {
        let (animating, recorded) =
            WORLD.with_borrow_mut(|w| (w.animating, std::mem::take(&mut w.recorded)));
        if !world_frame_wanted(animating, recorded, busy) {
            return None;
        }
        let (at, view) = self.java_world_camera()?;
        let mut out = Out::default();
        for value in self.java_client_window() {
            out.int(value);
        }
        out.float(self.clock.fraction() as f32);
        at.iter().for_each(|v| out.double(*v));
        view.iter().for_each(|v| out.float(*v));
        Some(out.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_is_told_the_detached_view_and_the_locator_wins() {
        let mut view = horizon_client::camera::Camera::default();
        assert_eq!(camera_for_java(&view, 70.0, 256.0, None), None);
        // Third person front at yaw 30, pitch 10: Camera.setup turns round to yaw + 180, -pitch.
        view.position = [0.0, 64.0, -4.0];
        view.y_rot = 30.0 + 180.0;
        view.x_rot = -10.0;
        assert_eq!(
            camera_for_java(&view, 80.0, 256.0, None),
            Some(([0.0, 64.0, -4.0], [210.0, -10.0, 80.0, 256.0]))
        );
        assert_eq!(
            camera_for_java(&view, 80.0, 256.0, Some(([1.0, 2.0, 3.0], 5.0, 6.0, 90.0))),
            Some(([1.0, 2.0, 3.0], [5.0, 6.0, 90.0, 256.0]))
        );
    }

    #[test]
    fn a_frame_is_recorded_only_while_the_world_animates_and_nothing_else_serves_it() {
        assert!(world_frame_wanted(true, false, false));
        assert!(!world_frame_wanted(true, true, false));
        assert!(!world_frame_wanted(true, false, true));
        assert!(!world_frame_wanted(false, false, false));
    }

    #[test]
    fn a_status_with_sky_and_weather_replaced_leaves_native_clouds() {
        java_world_status(None, false, 5);
        let replaced = horizon_client::world_renders::environment();
        assert!(replaced.sky && replaced.weather);
        assert!(!replaced.clouds && !replaced.rain_tick);
        java_world_left();
        assert_eq!(
            horizon_client::world_renders::environment(),
            Default::default()
        );
    }
}
