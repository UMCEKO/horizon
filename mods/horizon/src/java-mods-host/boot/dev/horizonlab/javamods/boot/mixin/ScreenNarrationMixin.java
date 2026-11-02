package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.gui.screens.Screen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Horizon narrates its own screens; Java has no narrator. */
@Mixin(Screen.class)
abstract class ScreenNarrationMixin {
    @Inject(method = "shouldRunNarration", at = @At("HEAD"), cancellable = true)
    private void horizon$narration(CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(false);
    }
}
