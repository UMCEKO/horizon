package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientMusic;
import net.minecraft.client.gui.components.toasts.ToastManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The Now Playing toast is native's: its requests reach the window. */
@Mixin(ToastManager.class)
abstract class ToastNowPlayingMixin {
    @Inject(method = "showNowPlayingToast", at = @At("HEAD"), cancellable = true)
    private void horizon$show(CallbackInfo info) {
        if (!ClientMusic.owned) return;
        ClientMusic.toast(true);
        info.cancel();
    }

    @Inject(method = "hideNowPlayingToast", at = @At("HEAD"), cancellable = true)
    private void horizon$hide(CallbackInfo info) {
        if (!ClientMusic.owned) return;
        ClientMusic.toast(false);
        info.cancel();
    }
}
