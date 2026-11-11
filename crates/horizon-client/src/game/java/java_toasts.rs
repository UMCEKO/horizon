// game-module-cfg: feature java-mods
//! Toasts mods add to the Java client's `ToastManager`, shown in the window's own toast column: native owns the slots, the sounds and the slide, Java runs each toast's `update` and records its drawing.

use super::*;
use horizon_client::toasts::Slide;
use horizon_java_host::wire::{In, Out};
use std::cell::RefCell;
use std::collections::HashSet;
use std::time::Instant;

/// `ToastManager.SLOT_COUNT`.
const SLOT_COUNT: u32 = 5;

/// `Toast.SLOT_HEIGHT`.
const SLOT_HEIGHT: f32 = 32.0;

struct JavaToast {
    id: i32,
    width: f32,
    slots: u32,
    sound: Option<String>,
    slot: Option<u32>,
    slide: Slide,
    /// The visibility the toast's own `update` last wanted.
    show: bool,
    /// The toast's drawing at the origin, as Java recorded it.
    frame: Vec<u8>,
    finished: bool,
}

#[derive(Default)]
struct Toasts {
    list: Vec<JavaToast>,
    /// A toast still changes its drawing: each frame is recorded with its own timing until two alike.
    animating: bool,
    /// A tick's status was applied this frame: its recording is this frame's.
    recorded: bool,
}

thread_local! {
    static TOASTS: RefCell<Toasts> = RefCell::default();
}

/// One toast's turn as Java answers it: the visibility it wants, and its drawing when that changed.
struct Turn {
    id: i32,
    show: bool,
    frame: Option<Vec<u8>>,
}

/// `ToastManager.findFreeSlotsIndex`: the first run of `slots` consecutive free slots among the five.
fn free_run(used: u32, slots: u32) -> Option<u32> {
    if slots == 0 || SLOT_COUNT - (used & 0b11111).count_ones() < slots {
        return None;
    }
    let mut run = 0;
    for i in 0..SLOT_COUNT {
        if used & (1 << i) != 0 {
            run = 0;
        } else {
            run += 1;
            if run == slots {
                return Some(i + 1 - run);
            }
        }
    }
    None
}

/// The slot bits of the toasts being shown.
fn shown_slots(list: &[JavaToast]) -> u32 {
    list.iter()
        .filter(|t| !t.finished)
        .filter_map(|t| t.slot.map(|first| (first, t.slots)))
        .fold(0, |used, (first, count)| used | (((1u32 << count) - 1) << first))
}

/// `ToastInstance.update` at `now`: the visibility sound to play, if it changed.
fn advance(toast: &mut JavaToast, now: i64) -> Option<bool> {
    let show = toast.show;
    let (sound, finished) = toast.slide.update(now, |_| show);
    toast.finished |= finished;
    sound
}

fn read_turns(reply: &mut In) -> Option<Vec<Turn>> {
    let mut turns = Vec::new();
    for _ in 0..reply.int()? {
        let id = reply.int()?;
        let show = reply.byte()? == 0;
        let frame = reply.bool()?.then(|| reply.bytes()).flatten().map(<[u8]>::to_vec);
        turns.push(Turn { id, show, frame });
    }
    Some(turns)
}

fn apply_turns(turns: Vec<Turn>) {
    TOASTS.with_borrow_mut(|t| {
        for turn in turns {
            if let Some(toast) = t.list.iter_mut().find(|toast| toast.id == turn.id) {
                toast.show = turn.show;
                if let Some(frame) = turn.frame {
                    toast.frame = frame;
                }
            }
        }
    });
}

/// A status's toasts: the ones `addToast` took, whether `clear` ran, this tick's turns, and whether any still animates.
pub(super) fn java_toasts_status(reply: &mut In) -> Option<()> {
    let mut fresh = Vec::new();
    for _ in 0..reply.int()? {
        let id = reply.int()?;
        let width = reply.int()? as f32;
        let _height = reply.int()?;
        let slots = reply.int()?.max(1) as u32;
        let sound = reply.text()?;
        fresh.push(JavaToast {
            id,
            width,
            slots,
            sound: (!sound.is_empty()).then(|| sound.to_owned()),
            slot: None,
            slide: Slide::default(),
            show: true,
            frame: Vec::new(),
            finished: false,
        });
    }
    let cleared = reply.bool()?;
    let turns = read_turns(reply)?;
    let animating = reply.bool()?;
    TOASTS.with_borrow_mut(|t| {
        if cleared {
            t.list.clear();
        }
        t.list.extend(fresh);
        t.animating = animating;
        t.recorded = true;
    });
    apply_turns(turns);
    Some(())
}

/// A frame request's reply: the turns, and whether any toast still animates.
pub(super) fn java_toasts_frame_apply(reply: &[u8]) {
    let mut reply = In(reply);
    let Some(turns) = read_turns(&mut reply) else {
        return;
    };
    let Some(animating) = reply.bool() else {
        return;
    };
    apply_turns(turns);
    TOASTS.with_borrow_mut(|t| t.animating = animating);
}

/// The window's end of a world: the toasts end with the Java client's.
pub(super) fn java_toasts_left() {
    TOASTS.with_borrow_mut(|t| *t = Toasts::default());
}

/// The tick's input for the Java client: each shown toast's `(id, finished, fully visible for)`; the finished ones are forgotten once told.
pub(super) fn java_toasts_turns() -> Vec<(i32, bool, i64)> {
    TOASTS.with_borrow_mut(|t| {
        let turns = t
            .list
            .iter()
            .filter(|toast| toast.slot.is_some() || toast.finished)
            .map(|toast| (toast.id, toast.finished, toast.slide.fully_for))
            .collect();
        t.list.retain(|toast| !toast.finished);
        turns
    })
}

impl Game<'_> {
    /// The slot bits of the shown mod toasts.
    pub(super) fn java_toast_slots(&self) -> u32 {
        TOASTS.with_borrow(|t| shown_slots(&t.list))
    }

    /// While a toast animates, this frame's recording request: each shown toast and how long it has been fully visible. None on the frame that took a tick's status or while a tick runs.
    pub(super) fn java_toasts_frame_request(&mut self, busy: bool) -> Option<Vec<u8>> {
        TOASTS.with_borrow_mut(|t| {
            let recorded = std::mem::take(&mut t.recorded);
            if !(t.animating && !recorded && !busy) {
                return None;
            }
            let shown: Vec<_> = t.list.iter().filter(|toast| toast.slot.is_some() && !toast.finished).collect();
            if shown.is_empty() {
                return None;
            }
            let mut out = Out::default();
            out.int(shown.len() as i32);
            for toast in shown {
                out.int(toast.id);
                out.long(toast.slide.fully_for);
            }
            Some(out.0)
        })
    }

    /// `ToastManager.update` and `extractRenderState` for the mods' toasts, after native's kinds took their slots.
    pub(super) fn java_draw_toasts(&mut self, batch: &mut HudBatch, hidden: bool) {
        if TOASTS.with_borrow(|t| t.list.is_empty()) {
            return;
        }
        let mut sounds = Vec::new();
        TOASTS.with_borrow_mut(|t| {
            let mut played = false;
            for toast in t.list.iter_mut().filter(|toast| toast.slot.is_some() && !toast.finished) {
                let origin = *toast.slide.origin.get_or_insert_with(Instant::now);
                let change = advance(toast, origin.elapsed().as_millis() as i64);
                // Only the first visibility change of an update plays its sound.
                if let Some(show) = change
                    && !std::mem::replace(&mut played, true)
                {
                    sounds.push(if show { "minecraft:ui.toast.in" } else { "minecraft:ui.toast.out" }.to_owned());
                }
            }
        });
        let mut used = self.toast_slots_used();
        TOASTS.with_borrow_mut(|t| {
            let mut played = HashSet::new();
            for toast in t.list.iter_mut().filter(|toast| toast.slot.is_none() && !toast.finished) {
                let Some(first) = free_run(used, toast.slots) else {
                    continue;
                };
                used |= ((1u32 << toast.slots) - 1) << first;
                toast.slot = Some(first);
                if let Some(sound) = &toast.sound
                    && played.insert(sound.clone())
                {
                    sounds.push(sound.clone());
                }
            }
        });
        for sound in sounds {
            self.audio.play(&sound, None, 1.0, 1.0);
        }
        if hidden {
            return;
        }
        let drawn: Vec<(Vec<u8>, [f32; 2])> = TOASTS.with_borrow(|t| {
            t.list
                .iter()
                .filter(|toast| !toast.finished && toast.frame.len() > 4)
                .filter_map(|toast| {
                    let x = batch.width - toast.width * toast.slide.portion;
                    Some((toast.frame.clone(), [x, toast.slot? as f32 * SLOT_HEIGHT]))
                })
                .collect()
        });
        for (frame, origin) in drawn {
            self.paint_java_frame(batch, &frame, origin);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toast(show: bool) -> JavaToast {
        JavaToast {
            id: 1,
            width: 160.0,
            slots: 1,
            sound: None,
            slot: Some(0),
            slide: Slide::default(),
            show,
            frame: Vec::new(),
            finished: false,
        }
    }

    #[test]
    fn free_run_finds_the_first_run_of_free_slots() {
        assert_eq!(free_run(0b00101, 2), Some(3));
        assert_eq!(free_run(0b00101, 3), None);
        assert_eq!(free_run(0, 5), Some(0));
        assert_eq!(free_run(0b10000, 4), Some(0));
        assert_eq!(free_run(0b11111, 1), None);
    }

    #[test]
    fn a_status_brings_a_fresh_toast_and_its_first_recording() {
        let mut out = Out::default();
        out.int(1);
        out.int(7);
        out.int(160);
        out.int(32);
        out.int(1);
        out.text("minecraft:ui.toast.in");
        out.bool(false);
        out.int(1);
        out.int(7);
        out.0.push(1);
        out.bool(true);
        out.bytes(&[0, 0, 0, 0]);
        out.bool(true);
        java_toasts_left();
        assert!(java_toasts_status(&mut In(&out.0)).is_some());
        TOASTS.with_borrow(|t| {
            assert_eq!(t.list.len(), 1);
            let toast = &t.list[0];
            assert_eq!((toast.id, toast.width, toast.slots), (7, 160.0, 1));
            assert_eq!(toast.sound.as_deref(), Some("minecraft:ui.toast.in"));
            assert!(!toast.show);
            assert_eq!(toast.frame, [0, 0, 0, 0]);
            assert!(t.animating);
        });
        java_toasts_left();
    }

    #[test]
    fn a_toast_that_stops_wanting_to_show_slides_out_and_frees_its_slot() {
        let mut shown = toast(true);
        assert_eq!(advance(&mut shown, 0), Some(true));
        assert_eq!(advance(&mut shown, 700), None);
        shown.show = false;
        assert_eq!(advance(&mut shown, 700), Some(false));
        assert!(!shown.finished);
        assert_eq!(shown_slots(std::slice::from_ref(&shown)), 0b1);
        advance(&mut shown, 1300);
        assert!(!shown.finished);
        advance(&mut shown, 1301);
        assert!(shown.finished);
        assert_eq!(shown_slots(std::slice::from_ref(&shown)), 0);
    }
}
