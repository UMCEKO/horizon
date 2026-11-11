// game-module-cfg: feature java-mods
// java-listener-family
//! `PlaySoundEvent` from `play_sound`: Java mods change or drop a sound native is about to play.

use super::java_listener::{Family, post_client};
use horizon_java_host::wire::{In, Out};
use mod_api::{HookId, LoopContext, PlaySoundEvent, SoundAttenuation, SoundPlay, SoundSource};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::sync::OnceLock;

const HOOKS: &[HookId] = &[HookId::PlaySound];
static SUBSCRIBED: OnceLock<Vec<Option<HookId>>> = OnceLock::new();

/// What the Java client's answer says of a sound: `NativeSounds.java`'s bytes.
const CANCEL: u8 = 0;
const AS_ASKED: u8 = 1;
const CHANGED: u8 = 2;

thread_local! {
    /// Set while the Java client's own sound starts: Java already posted its event.
    static EXTERNAL_NOW: Cell<bool> = const { Cell::new(false) };
    /// The Java client's sounds native holds, until it stops them.
    static EXTERNAL: RefCell<HashSet<u64>> = RefCell::default();
}

/// Plays a sound the Java client started, which its `SoundEngine.play` already posted; `play` answers its instance.
pub(super) fn external(play: impl FnOnce() -> u64) -> u64 {
    EXTERNAL_NOW.set(true);
    let id = play();
    EXTERNAL_NOW.set(false);
    EXTERNAL.with_borrow_mut(|held| held.insert(id));
    id
}

/// The Java client stopped one of its sounds: native's instance may be another sound's next.
pub(super) fn external_stopped(id: u64) {
    EXTERNAL.with_borrow_mut(|held| held.remove(&id));
}

fn is_external(id: u64) -> bool {
    EXTERNAL_NOW.get() || EXTERNAL.with_borrow(|held| held.contains(&id))
}

pub(super) fn family() -> Family {
    Family {
        hooks: HOOKS,
        listener: Box::new(Sounds),
    }
}

struct Sounds;

impl mod_api::Hooks for Sounds {
    fn subscribed(&self) -> &'static [Option<HookId>] {
        super::java_listener::subscribed(HOOKS, &SUBSCRIBED)
    }

    fn play_sound(&self, _: &dyn LoopContext, event: &mut PlaySoundEvent<'_>) {
        // Vanilla posts once per `SoundEngine.play`, not when a playing sound's volume or pitch refresh.
        if !event.starting
            || is_external(event.instance)
            || !super::java_listener::listens(HookId::PlaySound)
        {
            return;
        }
        let Some(sound) = event.sound.as_ref() else {
            return;
        };
        let answer = post_client(
            "PlaySoundEvent",
            |out| write_sound(out, sound),
            |reply| read_answer(reply, sound),
        );
        if let Some(answer) = answer {
            *event.sound = answer;
        }
    }
}

/// The sound as `NativeSounds.ask` reads it.
fn write_sound(out: &mut Out, sound: &SoundPlay) {
    out.text(&sound.identifier);
    out.int(sound.source.index() as i32);
    out.float(sound.volume);
    out.float(sound.pitch);
    out.bool(sound.relative);
    sound.position.iter().for_each(|v| out.double(*v));
    out.bool(sound.looping);
    out.int(sound.delay);
    out.bool(sound.attenuation == SoundAttenuation::Linear);
}

/// `Some(None)` when a listener dropped the sound, `Some(Some(_))` the sound it left, `None` to play it as asked.
fn read_answer(reply: &mut In, asked: &SoundPlay) -> Option<Option<SoundPlay>> {
    match reply.byte()? {
        CANCEL => Some(None),
        CHANGED => {
            let mut sound = asked.clone();
            sound.identifier = reply.text()?.to_owned();
            sound.source = *SoundSource::ALL.get(usize::try_from(reply.int()?).ok()?)?;
            sound.volume = reply.float()?;
            sound.pitch = reply.float()?;
            sound.relative = reply.bool()?;
            sound.position = [reply.double()?, reply.double()?, reply.double()?];
            sound.looping = reply.bool()?;
            sound.delay = reply.int()?;
            sound.attenuation = if reply.bool()? {
                SoundAttenuation::Linear
            } else {
                SoundAttenuation::None
            };
            Some(Some(sound))
        }
        AS_ASKED => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sound() -> SoundPlay {
        SoundPlay::at(
            "minecraft:block.note_block.bell",
            SoundSource::Records,
            [1.5, 64.0, -2.5],
        )
        .with_volume(0.5)
        .with_pitch(1.25)
        .looping(3)
    }

    #[test]
    fn a_cancel_drops_the_sound_and_as_asked_leaves_it() {
        assert_eq!(read_answer(&mut In(&[CANCEL]), &sound()), Some(None));
        assert_eq!(read_answer(&mut In(&[AS_ASKED]), &sound()), None);
    }

    #[test]
    fn a_change_comes_back_whole_in_the_order_java_writes_it() {
        let changed = sound();
        let asked = SoundPlay::ui("minecraft:ui.button.click");
        let mut out = Out(vec![CHANGED]);
        out.text(&changed.identifier);
        out.int(2);
        out.float(0.5);
        out.float(1.25);
        out.bool(false);
        [1.5, 64.0, -2.5].iter().for_each(|v| out.double(*v));
        out.bool(true);
        out.int(3);
        out.bool(true);
        assert_eq!(read_answer(&mut In(&out.0), &asked), Some(Some(changed)));
    }

    #[test]
    fn a_short_or_unknown_answer_plays_as_asked() {
        assert_eq!(read_answer(&mut In(&[CHANGED, 0, 0]), &sound()), None);
        assert_eq!(read_answer(&mut In(&[9]), &sound()), None);
        assert_eq!(read_answer(&mut In(&[]), &sound()), None);
    }

    #[test]
    fn the_request_is_what_native_sounds_reads() {
        let mut out = Out::default();
        write_sound(&mut out, &sound());
        let mut read = In(&out.0);
        assert_eq!(read.text(), Some("minecraft:block.note_block.bell"));
        assert_eq!(read.int(), Some(2));
        assert_eq!((read.float(), read.float()), (Some(0.5), Some(1.25)));
        assert_eq!(read.bool(), Some(false));
        assert_eq!((read.double(), read.double(), read.double()), (Some(1.5), Some(64.0), Some(-2.5)));
        assert_eq!((read.bool(), read.int(), read.bool()), (Some(true), Some(3), Some(true)));
        assert!(read.0.is_empty());
    }

    #[test]
    fn the_java_clients_own_sounds_are_not_posted_again() {
        let id = external(|| {
            assert!(is_external(77));
            77
        });
        assert!(is_external(id));
        assert!(!is_external(78));
        external_stopped(id);
        assert!(!is_external(id));
    }
}
