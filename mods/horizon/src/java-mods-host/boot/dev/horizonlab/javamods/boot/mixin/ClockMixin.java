package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.Blaze3D;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The window's library isn't started in this process, so its clock is the JVM's. */
@Mixin(Blaze3D.class)
abstract class ClockMixin {
    @Inject(method = "getTime", at = @At("HEAD"), cancellable = true)
    private static void horizon$time(CallbackInfoReturnable<Double> result) {
        result.setReturnValue(System.nanoTime() / 1.0E9);
    }
}
