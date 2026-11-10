package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.platform.InputConstants;
import dev.horizonlab.javamods.boot.ClientBridge;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Keys held in Horizon's window, as SDL's keyboard state would report them. */
@Mixin(InputConstants.class)
abstract class KeysMixin {
    @Inject(method = "isKeyDown(I)Z", at = @At("HEAD"), cancellable = true)
    private static void horizon$down(int key, CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(ClientBridge.keyDown(key));
    }
}
