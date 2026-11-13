// game-module-cfg: feature java-mods
//! What mods draw in the level's frame, as the Java client records it: applied from a tick's status, and recorded per window frame only while it changes.

use super::*;
use horizon_java_host::wire::{In, Out};
use horizon_client::world_renders::{OutlineAnswer, OutlineKind};
use std::cell::RefCell;

/// What native draws of the block outline this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OutlineDraw {
    /// Vanilla's lines, drawn natively.
    pub lines: bool,
    /// The mods' outline pieces of the world recording.
    pub pieces: bool,
}

const VANILLA_LINES: OutlineDraw = OutlineDraw {
    lines: true,
    pieces: false,
};
const NOTHING: OutlineDraw = OutlineDraw {
    lines: false,
    pieces: false,
};

/// The key an answer must match: block, face (3D data value), state, gate, high contrast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OutlineKey {
    pub pos: [i32; 3],
    pub face: u8,
    pub state: u32,
    pub render: bool,
    pub high_contrast: bool,
}

fn answers(answer: &OutlineAnswer, key: OutlineKey) -> bool {
    answer.block == Some((key.pos, key.face, key.state))
        && answer.render == key.render
        && answer.high_contrast == key.high_contrast
}

/// What native draws of the outline for the aimed block, given the Java recording's answer.
pub(super) fn outline_draw(
    answer: Option<&OutlineAnswer>,
    key: Option<OutlineKey>,
    native_canceled: bool,
) -> OutlineDraw {
    let Some(key) = key.filter(|k| k.render && !native_canceled) else {
        return NOTHING;
    };
    let Some(answer) = answer.filter(|a| answers(a, key)) else {
        return VANILLA_LINES;
    };
    match answer.kind {
        OutlineKind::None => VANILLA_LINES,
        OutlineKind::Canceled => NOTHING,
        OutlineKind::Vanilla => OutlineDraw {
            lines: true,
            pieces: true,
        },
        OutlineKind::Replaced => OutlineDraw {
            lines: false,
            pieces: true,
        },
    }
}

/// Whether the frame waits for Java's answer: only for a key not yet waited on.
fn should_sync(answer_matches: bool, key: Option<OutlineKey>, last_synced: Option<OutlineKey>) -> bool {
    !answer_matches && key.is_some() && key != last_synced
}

#[derive(Default)]
struct World {
    /// The key the window last waited on Java for.
    synced: Option<OutlineKey>,
    /// A mod has changed the outline since the join.
    outline_seen: bool,
    /// The last frame's aimed block and point.
    aimed: Option<(OutlineKey, [f64; 3])>,
    /// The point moved inside one block face: the next frame recording is asked for.
    point_moved: bool,
    /// This frame's decision, before the native mods' cancel.
    draw: Option<OutlineDraw>,
    /// Native hid the outline this frame: the HUD is off or a native mod cancelled it.
    native_off: bool,
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

/// The window's crosshair target as Java's block outline sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct OutlineHit {
    /// The hit block's position, face normal and point; none for an entity or a miss.
    pub block: Option<([i32; 3], [i32; 3], [f64; 3])>,
    /// `GameRenderer.shouldRenderBlockOutline` for that block.
    pub render: bool,
    pub high_contrast: bool,
}

/// `WorldFrame.writeHit`'s layout.
pub(super) fn write_hit(out: &mut Out, hit: &OutlineHit) {
    out.bool(hit.block.is_some());
    if let Some((pos, face, point)) = hit.block {
        out.long(horizon_core::position::BlockPos::new(pos[0], pos[1], pos[2]).pack());
        out.int(super::java_screens::face_index(face));
        point.iter().for_each(|v| out.double(*v));
    }
    out.bool(hit.render);
    out.bool(hit.high_contrast);
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
        let (animating, recorded) = WORLD.with_borrow_mut(|w| {
            let moved = std::mem::take(&mut w.point_moved);
            (w.animating || moved, std::mem::take(&mut w.recorded))
        });
        if !world_frame_wanted(animating, recorded, busy) {
            return None;
        }
        self.java_world_frame_request_now()
    }

    /// A frame recording request as `java_world_frame_request` builds it, without asking whether one is wanted.
    pub(super) fn java_world_frame_request_now(&self) -> Option<Vec<u8>> {
        let (at, view) = self.java_world_camera()?;
        let mut out = Out::default();
        for value in self.java_client_window() {
            out.int(value);
        }
        out.float(self.clock.fraction() as f32);
        at.iter().for_each(|v| out.double(*v));
        view.iter().for_each(|v| out.float(*v));
        write_hit(&mut out, &self.java_outline_hit());
        Some(out.0)
    }

    /// The window's outline decision for this frame, made before drawing: waits for Java's answer once per changed aimed block.
    pub(super) fn java_outline_frame(&mut self) {
        let hit = self.java_outline_hit();
        let key = hit.block.map(|(pos, face, _)| OutlineKey {
            pos,
            face: super::java_screens::face_index(face) as u8,
            state: self.world.block(pos[0], pos[1], pos[2]).0,
            render: hit.render,
            high_contrast: hit.high_contrast,
        });
        let point = hit.block.map(|(_, _, point)| point);
        let seen = WORLD.with_borrow_mut(|w| {
            w.outline_seen |= horizon_client::world_renders::outline().is_some();
            w.outline_seen
        });
        if !seen || !self.remote.is_some() || horizon_java_host::get().is_none() {
            WORLD.with_borrow_mut(|w| w.draw = Some(VANILLA_LINES));
            return;
        }
        let matches = |answer: Option<OutlineAnswer>| {
            key.is_some_and(|k| answer.is_some_and(|a| answers(&a, k)))
        };
        let last = WORLD.with_borrow(|w| w.synced);
        if should_sync(matches(horizon_client::world_renders::outline()), key, last) {
            WORLD.with_borrow_mut(|w| w.synced = key);
            self.java_outline_wait(|| matches(horizon_client::world_renders::outline()));
        }
        WORLD.with_borrow_mut(|w| {
            let same_face = w.aimed.map(|(k, _)| k) == key;
            w.point_moved |= same_face && w.aimed.map(|(_, p)| p) != point;
            w.aimed = key.zip(point);
            w.draw = Some(outline_draw(
                horizon_client::world_renders::outline().as_ref(),
                key,
                false,
            ));
        });
    }

    /// Native's own outline was hidden this frame (the HUD is off, or a native mod cancelled it): no piece is drawn either.
    pub(super) fn java_outline_native_off(&self, off: bool) {
        WORLD.with_borrow_mut(|w| w.native_off = off);
    }

    /// What native draws of the outline this frame.
    pub(super) fn java_outline_draw(&self) -> OutlineDraw {
        WORLD.with_borrow(|w| match w.draw {
            _ if w.native_off => NOTHING,
            Some(draw) => draw,
            None => VANILLA_LINES,
        })
    }

    /// The block the crosshair is on, as `Minecraft.hitResult` would hold it, with the outline's gate and option.
    pub(super) fn java_outline_hit(&self) -> OutlineHit {
        let block = if self.entity_targeted() {
            None
        } else {
            self.pick().map(|h| (h.pos, h.face, h.point))
        };
        OutlineHit {
            block,
            render: block.is_some_and(|(pos, _, _)| self.vanilla_outline_gate(pos)),
            high_contrast: self.settings.file.bool("highContrastBlockOutline"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: OutlineKey = OutlineKey {
        pos: [1, 2, 3],
        face: 1,
        state: 7,
        render: true,
        high_contrast: false,
    };

    fn answer(kind: OutlineKind, key: OutlineKey) -> OutlineAnswer {
        OutlineAnswer {
            kind,
            block: Some((key.pos, key.face, key.state)),
            render: key.render,
            high_contrast: key.high_contrast,
        }
    }

    fn draw(kind: OutlineKind, answered: OutlineKey, native: bool) -> (bool, bool) {
        let d = outline_draw(Some(&answer(kind, answered)), Some(KEY), native);
        (d.lines, d.pieces)
    }

    #[test]
    fn the_outline_draw_follows_the_matching_answer_only() {
        let replaced = OutlineKind::Replaced;
        assert_eq!(draw(replaced, KEY, true), (false, false));
        let no_hit = outline_draw(Some(&answer(replaced, KEY)), None, false);
        assert_eq!((no_hit.lines, no_hit.pieces), (false, false));
        let gated = OutlineKey { render: false, ..KEY };
        let d = outline_draw(Some(&answer(replaced, gated)), Some(gated), false);
        assert_eq!((d.lines, d.pieces), (false, false));
        let d = outline_draw(None, Some(KEY), false);
        assert_eq!((d.lines, d.pieces), (true, false));
        for stale in [
            OutlineKey { pos: [1, 2, 4], ..KEY },
            OutlineKey { face: 2, ..KEY },
            OutlineKey { state: 8, ..KEY },
            OutlineKey { high_contrast: true, ..KEY },
        ] {
            assert_eq!(draw(replaced, stale, false), (true, false));
        }
        assert_eq!(draw(OutlineKind::None, KEY, false), (true, false));
        assert_eq!(draw(OutlineKind::Canceled, KEY, false), (false, false));
        assert_eq!(draw(OutlineKind::Vanilla, KEY, false), (true, true));
        assert_eq!(draw(replaced, KEY, false), (false, true));
    }

    #[test]
    fn a_key_is_waited_on_once() {
        let other = OutlineKey { state: 9, ..KEY };
        assert!(should_sync(false, Some(KEY), None));
        assert!(!should_sync(false, Some(KEY), Some(KEY)));
        assert!(should_sync(false, Some(other), Some(KEY)));
        assert!(!should_sync(true, Some(other), Some(KEY)));
        assert!(!should_sync(false, None, None));
    }

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
