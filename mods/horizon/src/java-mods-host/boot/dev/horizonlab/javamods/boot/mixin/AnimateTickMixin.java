package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.Animate;
import net.minecraft.client.multiplayer.ClientLevel;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Native picks the positions; Java runs only the `animateTick` code mods own or patch, or vanilla's whole loop when a mod patches the loop itself. */
@Mixin(ClientLevel.class)
abstract class AnimateTickMixin {
    @Inject(method = "animateTick", at = @At("HEAD"), cancellable = true)
    private void horizon$animate(int x, int y, int z, CallbackInfo info) {
        if (Animate.wholeLoop()) {
            Animate.loop(true);
            return;
        }
        Animate.run((ClientLevel) (Object) this);
        info.cancel();
    }

    @Inject(method = "animateTick", at = @At("RETURN"))
    private void horizon$animated(int x, int y, int z, CallbackInfo info) {
        Animate.loop(false);
    }
}
