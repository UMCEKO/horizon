//! `LivingEntity.actuallyHurt` of a mob, apart from `hurtServer` so a Java mod's own call reaches it.
use super::*;

/// What `actuallyHurt` left for the rest of `hurtServer`.
pub(super) struct Taken {
    /// The damage after armor, effects, enchantments and innate resistance.
    pub(super) amount: f32,
    /// A wolf's armor took it instead.
    pub(super) absorbed: bool,
    dealt: f32,
    through: f32,
    armor_hit: Option<f32>,
}

impl Mob {
    /// `LivingEntity.actuallyHurt` of `amount` from `source`, the hit's sequence in `container`: true when it took health or absorption.
    pub fn actually_hurt(
        &mut self,
        level: &mut DamageLevel<'_>,
        container: &mut DamageContainer,
        source: Damage,
        amount: f32,
    ) -> bool {
        let taken = self.actually_hurt_health(level, container, source, amount, None);
        self.actually_hurt_after(level, container, source, &taken);
        taken.dealt != 0.0
    }

    /// `actuallyHurt` up to the health it takes; `hurtServer` writes its cooldown before the rest.
    pub(super) fn actually_hurt_health(
        &mut self,
        level: &mut DamageLevel<'_>,
        container: &mut DamageContainer,
        source: Damage,
        mut amount: f32,
        armor_effectiveness: Option<&mut dyn FnMut(f32, &mut LegacyRandom) -> f32>,
    ) -> Taken {
        // LivingEntity.getDamageAfterArmorAbsorb asks `hurtArmor` *before*
        // `CombatRules.getDamageAfterAbsorb`, so a worn piece wears by the
        // damage the armour, toughness and enchantment reductions have not
        // touched yet, and only a hit outside `#bypasses_armor` asks at all.
        let armor_hit = (!source.bypasses_armor() && amount > 0.0).then_some(amount);
        let entered = amount;
        let armor_ran = !source.bypasses_armor();
        if armor_ran {
            let (armor, armor_toughness) = armor::values(self);
            if armor > 0.0 {
                // CombatRules.getDamageAfterAbsorb
                let toughness = 2.0 + armor_toughness / 4.0;
                let effective = (armor - amount / toughness).clamp(armor * 0.2, 20.0);
                let mut fraction = effective / 25.0;
                if let Some(modify) = armor_effectiveness {
                    fraction = modify(fraction, level.random()).clamp(0.0, 1.0);
                }
                amount *= 1.0 - fraction;
            }
        }
        amount = container.reduce(
            Reduction::Armor,
            entered,
            entered - amount,
            amount,
            armor_ran,
        );
        let after_armor = amount;
        // LivingEntity.getDamageAfterMagicAbsorb: Resistance takes 20% per level off
        // (`#bypasses_resistance` and `#bypasses_effects` skip it) ...
        let mut resisted = false;
        if !source.kind.bypasses_effects()
            && !source.kind.bypasses_resistance()
            && let Some(&StoredEffect { amplifier: amp, .. }) = self
                .extra
                .effects
                .iter()
                .find(|e| e.id == "minecraft:resistance")
        {
            let remainder = 25i32.wrapping_sub(amp.wrapping_add(1).wrapping_mul(5));
            amount = (amount * remainder as f32 / 25.0).max(0.0);
            resisted = true;
            // actuallyHurt is void: zero health damage still lets hurtServer set hurt state.
        }
        amount = container.reduce(
            Reduction::MobEffects,
            after_armor,
            after_armor - amount,
            amount,
            resisted,
        );
        let after_effects = amount;
        // ... then, with damage left and outside `#bypasses_enchantments` (a
        // warden's sonic boom), CombatRules.getDamageAfterMagicAbsorb's
        // enchantment protection (#11308).
        let enchanted = amount > 0.0 && !source.kind.bypasses_enchantments();
        if enchanted {
            let epf = gear::protection(self, source.kind);
            if epf > 0.0 {
                amount *= 1.0 - epf / 25.0;
            }
        }
        amount = container.reduce(
            Reduction::Enchantments,
            after_effects,
            after_effects - amount,
            amount,
            enchanted,
        );
        let after_enchantments = amount;
        // Witch.getDamageAfterMagicAbsorb: none from its own doing, else
        // 85% less from `#witch_resistant_to`.
        let witch = self.species() == Species::Witch;
        if witch {
            if source.attacker == Some(Who::Mob(self.id)) {
                amount = 0.0;
            } else if matches!(
                source.kind,
                DamageKind::Magic
                    | DamageKind::IndirectMagic
                    | DamageKind::SonicBoom
                    | DamageKind::Thorns
            ) {
                amount *= 0.15;
            }
        }
        amount = container.reduce(
            Reduction::InnateResistance,
            after_enchantments,
            after_enchantments - amount,
            amount,
            witch,
        );
        // Wolf.actuallyHurt: wolf armor takes the damage instead of the wolf.
        let absorbed = self.wolf_armor_absorbs(&source);
        let mut dealt = amount;
        // Wolf.actuallyHurt keeps a hit its armor takes away from LivingEntity.actuallyHurt.
        if !absorbed {
            container.enter_pre(dealt);
            level.damage_pre(crate::health::Hurting::Mob(self, &source), container);
            dealt = container.capture_inflicted();
        }
        let mut through = 0.0;
        if absorbed {
            self.hurt_wolf_armor(dealt);
        } else {
            through = self.soak_hit(dealt, &source, container);
            container.record_reduction(Reduction::Absorption, dealt - through);
            self.health -= through;
        }
        Taken {
            amount,
            absorbed,
            dealt,
            through,
            armor_hit,
        }
    }

    /// The rest of `actuallyHurt`: the combat record, `LivingDamageEvent.Post`, a horse's armor and love.
    pub(super) fn actually_hurt_after(
        &mut self,
        level: &mut DamageLevel<'_>,
        container: &mut DamageContainer,
        source: Damage,
        taken: &Taken,
    ) {
        let Taken {
            absorbed,
            dealt,
            through,
            armor_hit,
            ..
        } = *taken;
        // LivingEntity.actuallyHurt: a hit that reaches the health is
        // recorded (`CombatTracker.recordDamage`), with the fall it ends;
        // Wolf.actuallyHurt keeps a hit its armor takes out of it.
        if !absorbed && dealt != 0.0 {
            self.record_damage(source, dealt);
        }
        if through != 0.0 {
            container.record_health_damage(through);
            level.damage_post(crate::health::Hurting::Mob(self, &source), container);
        }
        // LivingEntity.hurtArmor: `Horse` is the only mob that overrides it, and
        // it hands `doHurtEquipment` the body slot alone. A wolf's armour has
        // its own rule (`Wolf.actuallyHurt`, `hurt_wolf_armor`) and a player's
        // four pieces theirs, so the other "body slot" species — donkey, mule,
        // llama, camel, the skeleton and zombie horses — wear nothing.
        if let Some(damage) = armor_hit
            && self.species() == Species::Horse
            && self.extra.body_id().is_some()
        {
            self.outbox.push(MobEvent::ArmorHurt {
                mob: self.id,
                amount: damage,
                kind: source.kind,
            });
        }
        // Animal.actuallyHurt: damage taken ends love mode (`resetLove`).
        if drops::is_animal(self.species()) {
            self.extra.in_love = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;

    #[test]
    fn actually_hurt_takes_health_each_call_with_no_cooldown_or_hurt_time() {
        let mut fixture = crate::health::TestLevel::new(0);
        let mut level = fixture.level();
        let mut m = Mob::new(1, kind(), Vec3::ZERO, 0.0);
        let full = m.health;
        let generic = Damage::of(DamageKind::Generic);
        for hit in 1..=2 {
            let mut container = DamageContainer::new(2.0);
            assert!(m.actually_hurt(&mut level, &mut container, generic, 2.0));
            assert_eq!(m.health, full - 2.0 * hit as f32);
        }
        assert_eq!((m.invulnerable, m.hurt_time), (0, 0));
    }
}
