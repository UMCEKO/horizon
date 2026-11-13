package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.FovHook;
import dev.horizonlab.javamods.boot.Lockstep;
import net.minecraft.client.Camera;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** In a lockstep tick the field of view's easing is native's: this only posts the event, at vanilla's moment in the camera's tick. */
@Mixin(Camera.class)
abstract class CameraFovMixin {
    @Inject(method = "tickFov", at = @At("HEAD"), cancellable = true)
    private void horizon$fov(CallbackInfo info) {
        if (!Lockstep.active()) return;
        FovHook.tick();
        info.cancel();
    }
}
