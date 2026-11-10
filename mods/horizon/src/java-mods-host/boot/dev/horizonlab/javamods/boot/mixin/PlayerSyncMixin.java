package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.PlayerSync;
import java.util.Collection;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The acting player's health, absorption and effects reach Horizon's player as vanilla's own setters and effect callbacks change them. */
@Mixin(LivingEntity.class)
abstract class PlayerSyncMixin {
    @Inject(method = "setHealth", at = @At("RETURN"))
    private void horizon$health(float health, CallbackInfo ci) {
        PlayerSync.health(this);
    }

    @Inject(method = "setAbsorptionAmount", at = @At("RETURN"))
    private void horizon$absorption(float amount, CallbackInfo ci) {
        PlayerSync.absorption(this);
    }

    @Inject(method = "onEffectAdded", at = @At("RETURN"))
    private void horizon$effectAdded(MobEffectInstance effect, Entity source, CallbackInfo ci) {
        PlayerSync.effect(this, effect);
    }

    @Inject(method = "onEffectUpdated", at = @At("RETURN"))
    private void horizon$effectUpdated(MobEffectInstance effect, boolean refresh, Entity source, CallbackInfo ci) {
        PlayerSync.effect(this, effect);
    }

    @Inject(method = "onEffectsRemoved", at = @At("RETURN"))
    private void horizon$effectsRemoved(Collection<MobEffectInstance> effects, CallbackInfo ci) {
        PlayerSync.removed(this, effects);
    }
}
