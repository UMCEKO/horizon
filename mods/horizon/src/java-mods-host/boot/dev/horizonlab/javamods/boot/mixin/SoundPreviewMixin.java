package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.sounds.SoundPreviewHandler;
import net.minecraft.client.sounds.SoundManager;
import net.minecraft.sounds.SoundSource;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The options screen's volume previews are native's: a volume the window tells must not play one here. */
@Mixin(SoundPreviewHandler.class)
abstract class SoundPreviewMixin {
    @Inject(method = "preview", at = @At("HEAD"), cancellable = true)
    private static void horizon$preview(SoundManager manager, SoundSource category, float volume, CallbackInfo info) {
        info.cancel();
    }
}
