package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.Toasts;
import net.minecraft.client.gui.components.toasts.Toast;
import net.minecraft.client.gui.components.toasts.ToastManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The window shows toasts natively: a toast `addToast` queued reaches it, and stays in `queued` for `getToast` until the window finishes it. */
@Mixin(ToastManager.class)
abstract class ToastManagerMixin {
    @Inject(method = "addToast", at = @At("TAIL"))
    private void horizon$added(Toast toast, CallbackInfo info) {
        Toasts.added(toast);
    }

    @Inject(method = "clear", at = @At("TAIL"))
    private void horizon$cleared(CallbackInfo info) {
        Toasts.cleared();
    }
}
