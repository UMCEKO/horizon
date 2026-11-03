package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.sounds.MusicManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Music is native: Horizon picks and plays it. */
@Mixin(MusicManager.class)
abstract class MusicManagerMixin {
    @Inject(method = "tick", at = @At("HEAD"), cancellable = true)
    private void horizon$tick(CallbackInfo info) {
        info.cancel();
    }
}
