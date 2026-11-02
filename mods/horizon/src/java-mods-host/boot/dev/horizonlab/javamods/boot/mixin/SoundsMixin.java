package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientBridge;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.client.sounds.SoundEngine;
import net.minecraft.client.sounds.SoundManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Horizon's audio plays what mods' screens play. */
@Mixin(SoundManager.class)
abstract class SoundsMixin {
    @Inject(method = "play", at = @At("HEAD"), cancellable = true)
    private void horizon$play(SoundInstance sound, CallbackInfoReturnable<SoundEngine.PlayResult> result) {
        ClientBridge.sound(sound);
        result.setReturnValue(SoundEngine.PlayResult.STARTED);
    }
}
