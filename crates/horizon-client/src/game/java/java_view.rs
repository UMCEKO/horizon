// game-module-cfg: feature java-mods
//! The view events (camera angles, detached distance, field of view, mouse turn) on native's camera: the window's values go to the Java client once a tick and per frame only while the answers change.

use horizon_client::fog::{FogData, FogEnvironmentKind, FogType};
use horizon_java_host::wire::{In, Out};
use std::cell::RefCell;

/// What `FogRenderer.setupFog` computed, as the fog events start from it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(in crate::game) struct FogIn {
    pub(in crate::game) fog_type: FogType,
    pub(in crate::game) environment: FogEnvironmentKind,
    pub(in crate::game) render_distance_chunks: i32,
    pub(in crate::game) darken: f32,
    pub(in crate::game) fog: FogData,
}

/// The window's camera values as the Java client's view events read them.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub(in crate::game) struct ViewInputs {
    partial: f32,
    camera: [f64; 3],
    yaw: f32,
    pitch: f32,
    /// 0 first person, 1 back, 2 front.
    mode: u8,
    minecart: bool,
    /// Scale, distance, mount scale and mount distance, while detached.
    detached: Option<[f32; 4]>,
    fov: f32,
    hud_fov: f32,
    sensitivity: f64,
    smooth: bool,
    grabbed: bool,
    fog: Option<FogIn>,
}

/// What the mods changed, each `None` when no listener did.
#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub(in crate::game) struct ViewEffects {
    pub(in crate::game) angles: Option<[f32; 3]>,
    pub(in crate::game) distance: Option<f32>,
    pub(in crate::game) fov: Option<f32>,
    pub(in crate::game) hud_fov: Option<f32>,
    pub(in crate::game) turn: Option<(f64, bool)>,
    pub(in crate::game) fog_color: Option<[f32; 3]>,
    pub(in crate::game) fog: Option<FogData>,
}

/// The values `align_with_entity` reads when a living entity gives none.
const DEFAULT_DETACHED: [f32; 4] = [1.0, 4.0, 1.0, 4.0];

impl ViewInputs {
    /// The bytes of `Input` 36 and of `client_view_frame`'s request.
    pub(in crate::game) fn write(&self, out: &mut Out) {
        out.float(self.partial);
        self.camera.iter().for_each(|v| out.double(*v));
        out.float(self.yaw);
        out.float(self.pitch);
        out.0.push(self.mode);
        out.bool(self.minecart);
        if self.mode != 0 {
            let [scale, distance, mount_scale, mount_distance] =
                self.detached.unwrap_or(DEFAULT_DETACHED);
            out.float(scale);
            out.float(distance);
            out.float(mount_scale);
            out.float(mount_distance);
        }
        out.float(self.fov);
        out.float(self.hud_fov);
        out.double(self.sensitivity);
        out.bool(self.smooth);
        out.bool(self.grabbed);
        out.bool(self.fog.is_some());
        if let Some(f) = &self.fog {
            out.0.push(match f.fog_type {
                FogType::Atmospheric => 0,
                FogType::Water => 1,
                FogType::Lava => 2,
                FogType::PowderSnow => 3,
            });
            out.0.push(f.environment as u8);
            out.int(f.render_distance_chunks);
            out.float(f.darken);
            f.fog.color[..3].iter().for_each(|v| out.float(*v));
            [
                f.fog.environmental_start,
                f.fog.environmental_end,
                f.fog.render_distance_start,
                f.fog.render_distance_end,
                f.fog.sky_end,
                f.fog.cloud_end,
            ]
            .iter()
            .for_each(|v| out.float(*v));
        }
    }
}

/// `native` with the mods' fog colour and fog applied; values that are not numbers or have an end below its start keep native's.
fn fog_answered(native: FogData, effects: &ViewEffects) -> FogData {
    let mut fog = native;
    if let Some(rgb) = effects.fog_color.filter(|c| c.iter().all(|v| v.is_finite())) {
        fog.color = [rgb[0], rgb[1], rgb[2], native.color[3]];
    }
    if let Some(a) = effects.fog {
        let ten = a.color.iter().chain([
            &a.environmental_start,
            &a.environmental_end,
            &a.render_distance_start,
            &a.render_distance_end,
            &a.sky_end,
            &a.cloud_end,
        ]);
        if ten.into_iter().all(|v| v.is_finite())
            && a.environmental_end >= a.environmental_start
            && a.render_distance_end >= a.render_distance_start
        {
            fog = a;
        }
    }
    fog
}

/// This frame's fog inputs, recorded for the Java client, and `fog` as its last firing left it.
pub(in crate::game) fn hook_fog(input: FogIn) -> FogData {
    VIEW.with_borrow_mut(|v| {
        v.inputs.fog = Some(input);
        fog_answered(input.fog, &v.effects)
    })
}

/// No fog event is heard: the inputs carry no fog.
pub(in crate::game) fn forget_fog() {
    VIEW.with_borrow_mut(|v| v.inputs.fog = None);
}

/// The outputs the Java client answers: a mask of the values mods changed, then those in bit order.
pub(in crate::game) fn read_view_effects(reply: &mut In) -> Option<ViewEffects> {
    let mask = reply.int()?;
    let mut effects = ViewEffects::default();
    if mask & 1 != 0 {
        effects.angles = Some([reply.float()?, reply.float()?, reply.float()?]);
    }
    if mask & 2 != 0 {
        effects.distance = Some(reply.float()?);
    }
    if mask & 4 != 0 {
        effects.fov = Some(reply.float()?);
    }
    if mask & 8 != 0 {
        effects.hud_fov = Some(reply.float()?);
    }
    if mask & 16 != 0 {
        effects.turn = Some((reply.double()?, reply.bool()?));
    }
    if mask & 32 != 0 {
        effects.fog_color = Some([reply.float()?, reply.float()?, reply.float()?]);
    }
    if mask & 64 != 0 {
        effects.fog = Some(FogData {
            color: [reply.float()?, reply.float()?, reply.float()?, reply.float()?],
            environmental_start: reply.float()?,
            environmental_end: reply.float()?,
            render_distance_start: reply.float()?,
            render_distance_end: reply.float()?,
            sky_end: reply.float()?,
            cloud_end: reply.float()?,
        });
    }
    Some(effects)
}

#[derive(Default)]
struct View {
    /// This frame's values, filled as the seams run.
    inputs: ViewInputs,
    sent: Option<ViewInputs>,
    effects: ViewEffects,
    /// The Java client says the view is changing: each frame is fired with its own values until two alike.
    animating: bool,
    /// A tick's status was applied this frame: its firing is this frame's.
    recorded: bool,
}

thread_local! {
    static VIEW: RefCell<View> = RefCell::default();
}

fn finite(value: f32) -> bool {
    value.is_finite()
}

/// What the mods changed in the last firing.
pub(in crate::game) fn effects() -> ViewEffects {
    VIEW.with_borrow(|v| v.effects)
}

/// The yaw, pitch and roll the camera angle listeners left, when all three are numbers.
pub(in crate::game) fn angles() -> Option<[f32; 3]> {
    effects().angles.filter(|a| a.iter().all(|v| v.is_finite()))
}

/// The camera the window aligned this frame: the entity's eye and view rotation, before any detached back-off.
pub(in crate::game) fn record_camera(
    partial: f32,
    camera: [f64; 3],
    yaw: f32,
    pitch: f32,
    mode: u8,
) {
    VIEW.with_borrow_mut(|v| {
        let i = &mut v.inputs;
        (i.partial, i.camera, i.yaw, i.pitch, i.mode) = (partial, camera, yaw, pitch, mode);
        i.minecart = false;
        if mode == 0 {
            i.detached = None;
        }
    });
}

/// The scale and distance values a detached camera backed off by.
pub(in crate::game) fn record_detached(values: [f32; 4]) {
    VIEW.with_borrow_mut(|v| v.inputs.detached = Some(values));
}

/// The world field of view the frame computed.
pub(in crate::game) fn record_fov(fov: f32) {
    VIEW.with_borrow_mut(|v| v.inputs.fov = fov);
}

/// The first-person hand's field of view the frame computed, and what mods made of it.
pub(in crate::game) fn hud_fov(fov: f32) -> f32 {
    VIEW.with_borrow_mut(|v| {
        v.inputs.hud_fov = fov;
        v.effects.hud_fov.filter(|f| finite(*f)).unwrap_or(fov)
    })
}

/// What the frame's mouse turn reads: the options, and whether the pointer is grabbed.
pub(in crate::game) fn record_look(sensitivity: f64, smooth: bool, grabbed: bool) {
    VIEW.with_borrow_mut(|v| {
        v.inputs.sensitivity = sensitivity;
        v.inputs.smooth = smooth;
        v.inputs.grabbed = grabbed;
    });
}

/// The inputs to tell the Java client with a tick, when they differ from what it last heard.
pub(in crate::game) fn batch_input() -> Option<ViewInputs> {
    VIEW.with_borrow_mut(|v| {
        let changed = v.sent != Some(v.inputs);
        v.sent = Some(v.inputs);
        changed.then_some(v.inputs)
    })
}

/// A tick's status: its firing's answers, and whether the view keeps changing.
pub(in crate::game) fn java_view_status(effects: Option<ViewEffects>, animating: bool) {
    VIEW.with_borrow_mut(|v| {
        v.effects = effects.unwrap_or_default();
        v.animating = animating;
        v.recorded = true;
    });
}

/// A frame firing's reply: whether the view still changes, and the answers if they differ from the last.
pub(in crate::game) fn java_view_frame_apply(reply: &[u8]) {
    let mut reply = In(reply);
    let (Some(animating), Some(changed)) = (reply.bool(), reply.bool()) else {
        return;
    };
    let effects = changed.then(|| read_view_effects(&mut reply)).flatten();
    VIEW.with_borrow_mut(|v| {
        if let Some(effects) = effects {
            v.effects = effects;
        }
        v.animating = animating;
    });
}

/// The window's view state ends with its server.
pub(super) fn java_view_left() {
    VIEW.with_borrow_mut(|v| *v = View::default());
}

/// While the view changes, this frame's firing request; none on the frame that took a tick's status or while a tick runs.
pub(super) fn java_view_frame_request(busy: bool) -> Option<Vec<u8>> {
    VIEW.with_borrow_mut(|v| {
        let recorded = std::mem::take(&mut v.recorded);
        super::java_screens::hud::hud_frame_wanted(v.animating, recorded, busy).then(|| {
            let mut out = Out::default();
            v.inputs.write(&mut out);
            out.0
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inputs_go_over_the_wire_in_the_documented_order() {
        let inputs = ViewInputs {
            partial: 0.5,
            camera: [1.0, 2.0, 3.0],
            yaw: 30.0,
            pitch: -10.0,
            mode: 1,
            minecart: false,
            detached: Some([1.0, 4.0, 2.0, 6.0]),
            fov: 70.0,
            hud_fov: 70.0,
            sensitivity: 0.5,
            smooth: false,
            grabbed: true,
            fog: None,
        };
        let mut out = Out::default();
        inputs.write(&mut out);
        let mut expected = Vec::new();
        expected.extend(0.5f32.to_be_bytes());
        for v in [1.0f64, 2.0, 3.0] {
            expected.extend(v.to_be_bytes());
        }
        expected.extend(30.0f32.to_be_bytes());
        expected.extend((-10.0f32).to_be_bytes());
        expected.extend([1, 0]);
        for v in [1.0f32, 4.0, 2.0, 6.0, 70.0, 70.0] {
            expected.extend(v.to_be_bytes());
        }
        expected.extend(0.5f64.to_be_bytes());
        expected.extend([0, 1, 0]);
        assert_eq!(out.0, expected);
    }

    fn sample_fog() -> FogData {
        FogData {
            color: [0.1, 0.2, 0.3, 1.0],
            environmental_start: -8.0,
            environmental_end: 96.0,
            render_distance_start: 100.0,
            render_distance_end: 128.0,
            sky_end: 128.0,
            cloud_end: 256.0,
        }
    }

    #[test]
    fn fog_inputs_follow_the_documented_bytes() {
        let inputs = ViewInputs {
            fog: Some(FogIn {
                fog_type: FogType::Lava,
                environment: FogEnvironmentKind::Water,
                render_distance_chunks: 8,
                darken: 0.5,
                fog: sample_fog(),
            }),
            ..Default::default()
        };
        let mut with = Out::default();
        inputs.write(&mut with);
        let mut without = Out::default();
        ViewInputs::default().write(&mut without);
        let tail = &with.0[without.0.len() - 1..];
        let mut expected = vec![1, 2, 4];
        expected.extend(8i32.to_be_bytes());
        for v in [0.5f32, 0.1, 0.2, 0.3, -8.0, 96.0, 100.0, 128.0, 128.0, 256.0] {
            expected.extend(v.to_be_bytes());
        }
        assert_eq!(tail, expected);
    }

    #[test]
    fn fog_colour_alone_changes_only_the_rgb() {
        let effects = ViewEffects {
            fog_color: Some([1.0, 0.0, 0.0]),
            ..Default::default()
        };
        let mut native = sample_fog();
        native.color[3] = 0.75;
        let fog = fog_answered(native, &effects);
        assert_eq!(fog.color, [1.0, 0.0, 0.0, 0.75]);
        assert_eq!(fog.environmental_end, 96.0);
    }

    #[test]
    fn fog_answer_replaces_all_ten_values_unless_one_is_unusable() {
        let mut answer = sample_fog();
        answer.environmental_end = 8.0;
        answer.color = [1.0, 0.0, 0.0, 1.0];
        let effects = ViewEffects {
            fog: Some(answer),
            ..Default::default()
        };
        assert_eq!(fog_answered(sample_fog(), &effects), answer);
        answer.sky_end = f32::NAN;
        let bad = ViewEffects {
            fog: Some(answer),
            ..Default::default()
        };
        assert_eq!(fog_answered(sample_fog(), &bad), sample_fog());
        answer.sky_end = 1.0;
        answer.environmental_end = -100.0;
        let inverted = ViewEffects {
            fog: Some(answer),
            ..Default::default()
        };
        assert_eq!(fog_answered(sample_fog(), &inverted), sample_fog());
    }

    #[test]
    fn fog_bits_read_after_the_turn() {
        let mut out = Out::default();
        out.int(32 | 64);
        for v in [1.0f32, 0.0, 0.0] {
            out.float(v);
        }
        for v in [0.5f32, 0.5, 0.5, 1.0, 1.0, 8.0, 50.0, 60.0, 70.0, 80.0] {
            out.float(v);
        }
        let effects = read_view_effects(&mut In(&out.0)).unwrap();
        assert_eq!(effects.fog_color, Some([1.0, 0.0, 0.0]));
        let fog = effects.fog.unwrap();
        assert_eq!(fog.color, [0.5, 0.5, 0.5, 1.0]);
        assert_eq!((fog.environmental_start, fog.cloud_end), (1.0, 80.0));
    }

    #[test]
    fn first_person_inputs_leave_out_the_detached_values() {
        let mut first = Out::default();
        ViewInputs::default().write(&mut first);
        let mut third = Out::default();
        ViewInputs {
            mode: 1,
            ..Default::default()
        }
        .write(&mut third);
        assert_eq!(third.0.len() - first.0.len(), 16);
    }

    #[test]
    fn a_mask_reads_only_the_values_it_names() {
        let mut out = Out::default();
        out.int(0b10101);
        for v in [5.0f32, 6.0, 20.0] {
            out.float(v);
        }
        out.float(35.0);
        out.double(0.25);
        out.bool(true);
        let effects = read_view_effects(&mut In(&out.0)).unwrap();
        assert_eq!(
            effects,
            ViewEffects {
                angles: Some([5.0, 6.0, 20.0]),
                distance: None,
                fov: Some(35.0),
                hud_fov: None,
                turn: Some((0.25, true)),
                fog_color: None,
                fog: None,
            }
        );
        assert_eq!(read_view_effects(&mut In(&out.0[..out.0.len() - 1])), None);
    }

    #[test]
    fn a_frame_asks_only_while_the_view_animates_and_nothing_else_serves_it() {
        java_view_status(None, true);
        assert!(java_view_frame_request(false).is_none());
        assert!(java_view_frame_request(true).is_none());
        assert!(java_view_frame_request(false).is_some());
        java_view_frame_apply(&[0, 0]);
        assert!(java_view_frame_request(false).is_none());
        java_view_left();
    }

    #[test]
    fn a_frame_reply_keeps_the_last_answers_unless_they_changed() {
        java_view_status(
            Some(ViewEffects {
                fov: Some(35.0),
                ..Default::default()
            }),
            true,
        );
        java_view_frame_apply(&[1, 0]);
        assert_eq!(effects().fov, Some(35.0));
        let mut out = Out::default();
        out.bool(false);
        out.bool(true);
        out.int(0);
        java_view_frame_apply(&out.0);
        assert_eq!(effects(), ViewEffects::default());
        java_view_left();
    }

    #[test]
    fn an_unchanged_input_is_not_sent_twice() {
        java_view_left();
        record_look(0.5, false, true);
        assert!(batch_input().is_some());
        assert!(batch_input().is_none());
        record_look(0.5, false, false);
        assert!(batch_input().is_some());
        java_view_left();
    }
}
