// game-module-cfg: feature java-mods
//! What mods draw in the level's frame, as the Java client records it: applied from a tick's status, and recorded per window frame only while it changes.

use super::*;
use horizon_java_host::wire::{In, Out};
use horizon_client::world_renders::{OutlineAnswer, OutlineKind};
use std::cell::{Cell, RefCell};

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
    static OUTLINE: Cell<OutlineDraw> = const { Cell::new(OutlineDraw::VANILLA) };
    static LAST_SYNCED: Cell<Option<OutlineKey>> = const { Cell::new(None) };
    static LAST_AT: Cell<Option<(Option<OutlineKey>, Option<[f64; 3]>)>> = const { Cell::new(None) };
    static POINT_MOVED: Cell<bool> = const { Cell::new(false) };
    static SEEN: Cell<bool> = const { Cell::new(false) };
}

/// What native draws of the block outline this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OutlineDraw {
    /// Vanilla's lines, drawn natively.
    pub lines: bool,
    /// The mods' outline pieces of the world recording.
    pub pieces: bool,
}

impl OutlineDraw {
    const VANILLA: OutlineDraw = OutlineDraw {
        lines: true,
        pieces: false,
    };
}

/// The key an answer must match: block, face (3D data value), state, gate, high contrast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct OutlineKey {
    pub pos: [i32; 3],
    pub face: u8,
    pub state: u32,
    pub render: bool,
    pub high_contrast: bool,
}

fn answer_matches(answer: &OutlineAnswer, key: &OutlineKey) -> bool {
    answer.block == Some((key.pos, key.face, key.state))
        && answer.render == key.render
        && answer.high_contrast == key.high_contrast
}

/// What native draws for this hit given the recording's answer, by the patched renderer's rules.
pub(super) fn outline_draw(
    answer: Option<&OutlineAnswer>,
    key: Option<OutlineKey>,
    native_canceled: bool,
) -> OutlineDraw {
    let none = OutlineDraw {
        lines: false,
        pieces: false,
    };
    let Some(key) = key.filter(|k| k.render && !native_canceled) else {
        return none;
    };
    let Some(answer) = answer else {
        return OutlineDraw::VANILLA;
    };
    if !answer_matches(answer, &key) {
        return OutlineDraw::VANILLA;
    }
    match answer.kind {
        OutlineKind::None => OutlineDraw::VANILLA,
        OutlineKind::Canceled => none,
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

/// Whether a mismatching answer is worth one wait: only the first time for a key.
fn should_sync(answer_matches: bool, key: Option<OutlineKey>, last_synced: Option<OutlineKey>) -> bool {
    !answer_matches && key.is_some() && key != last_synced
}

/// What the frame's outline draws; vanilla's lines alone without an outline-changing mod.
pub(super) fn java_outline_draw() -> OutlineDraw {
    OUTLINE.get()
}

/// A native mod cancelled the highlight this frame: no lines of Java's either.
pub(super) fn java_outline_native_canceled() {
    OUTLINE.set(OutlineDraw {
        lines: false,
        pieces: false,
    });
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
    OUTLINE.set(OutlineDraw::VANILLA);
    LAST_SYNCED.set(None);
    LAST_AT.set(None);
    POINT_MOVED.set(false);
    SEEN.set(false);
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
        if !world_frame_wanted(animating || POINT_MOVED.take(), recorded, busy) {
            return None;
        }
        self.java_world_frame_request_now()
    }

    /// The world frame request without asking whether one is wanted.
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

    /// This frame's outline decision, made before drawing: waits once for the answer when the aimed block changed.
    pub(super) fn java_outline_frame(&mut self) {
        OUTLINE.set(OutlineDraw::VANILLA);
        if !super::java_screens::java_client_joined() || horizon_java_host::get().is_none() {
            return;
        }
        if horizon_client::world_renders::outline().is_some() {
            SEEN.set(true);
        }
        if !SEEN.get() {
            return;
        }
        let hit = self.java_outline_hit();
        let key = hit.block.map(|(pos, face, _)| OutlineKey {
            pos,
            face: super::java_screens::face_index(face) as u8,
            state: self.world.block(pos[0], pos[1], pos[2]).0,
            render: hit.render,
            high_contrast: hit.high_contrast,
        });
        let matches = |key: &Option<OutlineKey>| {
            horizon_client::world_renders::outline()
                .zip(key.as_ref())
                .is_some_and(|(a, k)| answer_matches(&a, k))
        };
        if should_sync(matches(&key), key, LAST_SYNCED.get()) {
            horizon_client::spans::time("java_outline_wait", || {
                LAST_SYNCED.set(key);
                self.java_client_collect(true);
                if !matches(&key)
                    && let Some(request) = self.java_world_frame_request_now()
                {
                    super::java_screens::send_world_frame(request);
                    self.java_client_collect(true);
                }
            });
        }
        let at = (key, hit.block.map(|(_, _, point)| point));
        let last = LAST_AT.replace(Some(at));
        if key.is_some() && last.is_some_and(|(k, p)| k == key && p != at.1) {
            POINT_MOVED.set(true);
        }
        let answer = horizon_client::world_renders::outline();
        OUTLINE.set(outline_draw(answer.as_ref(), key, false));
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

    fn key() -> OutlineKey {
        OutlineKey {
            pos: [1, 2, 3],
            face: 1,
            state: 7,
            render: true,
            high_contrast: false,
        }
    }

    fn answer(kind: OutlineKind, key: OutlineKey) -> OutlineAnswer {
        OutlineAnswer {
            kind,
            block: Some((key.pos, key.face, key.state)),
            render: key.render,
            high_contrast: key.high_contrast,
        }
    }

    fn draws(lines: bool, pieces: bool) -> OutlineDraw {
        OutlineDraw { lines, pieces }
    }

    #[test]
    fn the_outline_follows_the_answer_for_the_aimed_block_only() {
        let k = key();
        let replaced = answer(OutlineKind::Replaced, k);
        assert_eq!(outline_draw(Some(&replaced), Some(k), true), draws(false, false));
        assert_eq!(outline_draw(Some(&replaced), None, false), draws(false, false));
        let gated = OutlineKey { render: false, ..k };
        assert_eq!(outline_draw(Some(&answer(OutlineKind::Replaced, gated)), Some(gated), false), draws(false, false));
        assert_eq!(outline_draw(None, Some(k), false), draws(true, false));
        for stale in [
            OutlineKey { pos: [9, 2, 3], ..k },
            OutlineKey { face: 2, ..k },
            OutlineKey { state: 8, ..k },
            OutlineKey { high_contrast: true, ..k },
        ] {
            assert_eq!(outline_draw(Some(&replaced), Some(stale), false), draws(true, false));
        }
        let by = |kind| outline_draw(Some(&answer(kind, k)), Some(k), false);
        assert_eq!(by(OutlineKind::None), draws(true, false));
        assert_eq!(by(OutlineKind::Canceled), draws(false, false));
        assert_eq!(by(OutlineKind::Vanilla), draws(true, true));
        assert_eq!(by(OutlineKind::Replaced), draws(false, true));
    }

    #[test]
    fn a_mismatching_key_waits_once() {
        let k = key();
        let other = OutlineKey { state: 9, ..k };
        assert!(should_sync(false, Some(k), None));
        assert!(!should_sync(false, Some(k), Some(k)));
        assert!(should_sync(false, Some(other), Some(k)));
        assert!(!should_sync(true, Some(k), None));
        assert!(!should_sync(false, None, None));
    }

    #[test]
    fn a_moved_point_asks_a_frame_like_animation_does() {
        assert!(world_frame_wanted(true, false, false));
        assert!(!world_frame_wanted(true, false, true));
        assert!(!world_frame_wanted(true, true, false));
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
