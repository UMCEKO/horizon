package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.platform.Window;
import dev.horizonlab.javamods.boot.ClientBridge;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The GUI scale is the window's: native chose it with vanilla's rule and the player's option. */
@Mixin(Window.class)
abstract class WindowScaleMixin {
    @Inject(method = "calculateScale", at = @At("HEAD"), cancellable = true)
    private void horizon$scale(int maxScale, boolean enforceUnicode, CallbackInfoReturnable<Integer> result) {
        result.setReturnValue(ClientBridge.guiScale());
    }
}
