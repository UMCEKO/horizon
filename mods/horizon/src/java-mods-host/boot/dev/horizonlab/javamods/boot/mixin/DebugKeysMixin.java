package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.KeyboardHandler;
import net.minecraft.client.input.KeyEvent;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The debug keys act on the window, which handles them itself. */
@Mixin(KeyboardHandler.class)
abstract class DebugKeysMixin {
    @Inject(method = "handleDebugKeys", at = @At("HEAD"), cancellable = true)
    private void horizon$debug(KeyEvent event, CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(false);
    }
}
