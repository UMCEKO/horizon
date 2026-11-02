package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.BlockRenders;
import dev.horizonlab.javamods.boot.ClientBridge;
import java.util.Map;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.client.sounds.SoundEngine;
import net.minecraft.client.sounds.SoundManager;
import net.minecraft.client.sounds.WeighedSoundEvents;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Horizon's audio plays what mods' screens play. */
@Mixin(SoundManager.class)
abstract class SoundsMixin {
    @Shadow
    @Final
    private Map<Identifier, WeighedSoundEvents> registry;

    @Inject(method = "play", at = @At("HEAD"), cancellable = true)
    private void horizon$play(SoundInstance sound, CallbackInfoReturnable<SoundEngine.PlayResult> result) {
        // Ticks run only to record a renderer's motion make no sound.
        if (!BlockRenders.sampling()) ClientBridge.sound(sound);
        result.setReturnValue(SoundEngine.PlayResult.STARTED);
    }

    /** Sounds are Horizon's, so this view loads none: an event lookup finds nothing rather than failing. */
    @Inject(method = "getSoundEvent", at = @At("HEAD"), cancellable = true)
    private void horizon$event(Identifier id, CallbackInfoReturnable<WeighedSoundEvents> result) {
        if (registry == null) result.setReturnValue(null);
    }
}
