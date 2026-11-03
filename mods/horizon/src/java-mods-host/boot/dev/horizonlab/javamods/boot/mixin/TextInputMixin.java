package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.platform.TextInputManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Text input is the window's: it composes text and sends the characters. */
@Mixin(TextInputManager.class)
abstract class TextInputMixin {
    @Inject(method = {"startTextInput", "stopTextInput()V", "applyTextInputArea"}, at = @At("HEAD"), cancellable = true)
    private void horizon$native(CallbackInfo info) {
        info.cancel();
    }
}
