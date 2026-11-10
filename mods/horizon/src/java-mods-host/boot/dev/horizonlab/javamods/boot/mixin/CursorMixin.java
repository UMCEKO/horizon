package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.platform.InputConstants;
import com.mojang.blaze3d.platform.Window;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The cursor is the window's: grabbing and releasing it is native. */
@Mixin(InputConstants.class)
abstract class CursorMixin {
    @Inject(method = "grabMouse", at = @At("HEAD"), cancellable = true)
    private static void horizon$grab(Window window, double x, double y, CallbackInfo info) {
        info.cancel();
    }

    @Inject(method = "releaseMouse", at = @At("HEAD"), cancellable = true)
    private static void horizon$release(Window window, double x, double y, CallbackInfo info) {
        info.cancel();
    }
}
