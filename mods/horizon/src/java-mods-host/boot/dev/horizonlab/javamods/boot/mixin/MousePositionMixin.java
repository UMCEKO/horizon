package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.MouseHandler;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The cursor is the window's: its position arrives with its moves, not by asking the window's library. */
@Mixin(MouseHandler.class)
abstract class MousePositionMixin {
    @Inject(method = "resyncMousePosition", at = @At("HEAD"), cancellable = true)
    private void horizon$resync(CallbackInfo info) {
        info.cancel();
    }
}
