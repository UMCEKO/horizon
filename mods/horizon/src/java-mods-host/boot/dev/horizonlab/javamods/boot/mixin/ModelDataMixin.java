package dev.horizonlab.javamods.boot.mixin;

import net.neoforged.neoforge.model.data.ModelDataManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Calls into this JVM take turns under one lock, so whichever thread makes them is the level's own. */
@Mixin(value = ModelDataManager.class, remap = false)
abstract class ModelDataMixin {
    @Inject(method = "isOtherThread", at = @At("HEAD"), cancellable = true)
    private void horizon$thread(CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(false);
    }
}
