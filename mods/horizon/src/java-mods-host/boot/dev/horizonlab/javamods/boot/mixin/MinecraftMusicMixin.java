package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientMusic;
import net.minecraft.client.Minecraft;
import net.minecraft.sounds.Music;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** While Java runs the music tick, the game's answers are native's. */
@Mixin(Minecraft.class)
abstract class MinecraftMusicMixin {
    @Inject(method = "getSituationalMusic", at = @At("HEAD"), cancellable = true)
    private void horizon$situational(CallbackInfoReturnable<Music> result) {
        if (ClientMusic.owned) result.setReturnValue(ClientMusic.music);
    }

    @Inject(method = "getMusicVolume", at = @At("HEAD"), cancellable = true)
    private void horizon$volume(CallbackInfoReturnable<Float> result) {
        if (ClientMusic.owned) result.setReturnValue(ClientMusic.volume);
    }
}
