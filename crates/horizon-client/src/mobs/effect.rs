//! `MobEffectInstance` for a mob: the effect, its details and the chain of weaker effects hidden behind it.

#[derive(Clone, Debug, PartialEq)]
pub struct MobEffect {
    pub id: &'static str,
    pub ticks: i32,
    pub amplifier: i32,
    pub ambient: bool,
    pub visible: bool,
    pub show_icon: bool,
    pub hidden: Option<Box<MobEffect>>,
}

impl MobEffect {
    /// `new MobEffectInstance(effect, duration, amplifier)`: not ambient, visible, icon shown.
    pub fn new(id: &'static str, ticks: i32, amplifier: i32) -> Self {
        Self {
            id,
            ticks,
            amplifier: amplifier.clamp(0, 255),
            ambient: false,
            visible: true,
            show_icon: true,
            hidden: None,
        }
    }

    pub fn infinite(&self) -> bool {
        self.ticks == -1
    }

    /// `ClientboundUpdateMobEffectPacket`'s flags without the blend bit.
    pub fn packet_flags(&self) -> i32 {
        i32::from(self.ambient) | i32::from(self.visible) << 1 | i32::from(self.show_icon) << 2
    }

    /// `MobEffectInstance(copy)`: the details, no hidden chain.
    fn copied(&self) -> Self {
        Self {
            hidden: None,
            ..self.clone()
        }
    }

    fn shorter_than(&self, other: &Self) -> bool {
        !self.infinite() && (self.ticks < other.ticks || other.infinite())
    }

    /// `MobEffectInstance.update`.
    pub fn update(&mut self, take_over: &Self) -> bool {
        let mut changed = false;
        if take_over.amplifier > self.amplifier {
            if take_over.shorter_than(self) {
                let mut hidden = self.copied();
                hidden.hidden = self.hidden.take();
                self.hidden = Some(Box::new(hidden));
            }
            self.amplifier = take_over.amplifier;
            self.ticks = take_over.ticks;
            changed = true;
        } else if self.shorter_than(take_over) {
            if take_over.amplifier == self.amplifier {
                self.ticks = take_over.ticks;
                changed = true;
            } else if let Some(hidden) = self.hidden.as_mut() {
                hidden.update(take_over);
            } else {
                self.hidden = Some(Box::new(take_over.copied()));
            }
        }
        if (!take_over.ambient && self.ambient) || changed {
            self.ambient = take_over.ambient;
            changed = true;
        }
        if take_over.visible != self.visible {
            self.visible = take_over.visible;
            changed = true;
        }
        if take_over.show_icon != self.show_icon {
            self.show_icon = take_over.show_icon;
            changed = true;
        }
        changed
    }

    /// `tickDownDuration` then `downgradeToHiddenEffect`; true when the hidden effect took over.
    pub fn tick_down(&mut self) -> bool {
        self.tick_down_duration();
        self.downgrade_to_hidden()
    }

    fn tick_down_duration(&mut self) {
        if let Some(hidden) = self.hidden.as_mut() {
            hidden.tick_down_duration();
        }
        if !self.infinite() && self.ticks != 0 {
            self.ticks -= 1;
        }
    }

    fn downgrade_to_hidden(&mut self) -> bool {
        if self.ticks != 0 {
            return false;
        }
        let Some(hidden) = self.hidden.take() else {
            return false;
        };
        let hidden = *hidden;
        self.ticks = hidden.ticks;
        self.amplifier = hidden.amplifier;
        self.ambient = hidden.ambient;
        self.visible = hidden.visible;
        self.show_icon = hidden.show_icon;
        self.hidden = hidden.hidden;
        true
    }

    /// `MobEffectInstance.hasRemainingDuration`.
    pub fn has_remaining(&self) -> bool {
        self.infinite() || self.ticks > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stronger_shorter_effect_hides_the_weaker_longer_one() {
        let mut e = MobEffect::new("minecraft:speed", 600, 0);
        assert!(e.update(&MobEffect::new("minecraft:speed", 100, 1)));
        assert_eq!((e.ticks, e.amplifier), (100, 1));
        let hidden = e.hidden.as_ref().unwrap();
        assert_eq!((hidden.ticks, hidden.amplifier), (600, 0));
    }

    #[test]
    fn the_hidden_effect_takes_over_when_the_front_one_ends() {
        let mut e = MobEffect::new("minecraft:speed", 600, 0);
        e.update(&MobEffect::new("minecraft:speed", 2, 1));
        assert!(!e.tick_down());
        assert!(e.tick_down());
        assert_eq!((e.ticks, e.amplifier), (598, 0));
        assert!(e.hidden.is_none());
    }

    #[test]
    fn a_weaker_longer_effect_waits_behind_and_the_flags_follow_the_newest() {
        let mut e = MobEffect::new("minecraft:speed", 50, 1);
        let mut weak = MobEffect::new("minecraft:speed", 400, 0);
        weak.visible = false;
        weak.show_icon = false;
        assert!(e.update(&weak));
        assert_eq!(e.hidden.as_ref().unwrap().ticks, 400);
        assert_eq!(e.packet_flags(), 0);
    }

    #[test]
    fn a_non_ambient_effect_clears_the_ambient_flag() {
        let mut e = MobEffect::new("minecraft:speed", 50, 0);
        e.ambient = true;
        assert_eq!(e.packet_flags(), 7);
        assert!(e.update(&MobEffect::new("minecraft:speed", 10, 0)));
        assert!(!e.ambient);
    }
}
