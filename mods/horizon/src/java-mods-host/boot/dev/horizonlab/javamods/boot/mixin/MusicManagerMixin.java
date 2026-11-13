package dev.horizonlab.javamods.boot.mixin;

import com.llamalad7.mixinextras.injector.ModifyExpressionValue;
import dev.horizonlab.javamods.boot.ClientMusic;
import net.minecraft.client.sounds.MusicManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Music is native unless a mod changes the tick: then this one runs, and native answers what it asks the game. */
@Mixin(MusicManager.class)
abstract class MusicManagerMixin {
    @Inject(method = "tick", at = @At("HEAD"), cancellable = true)
    private void horizon$tick(CallbackInfo info) {
        if (!ClientMusic.owned) info.cancel();
    }

    /** Native's level loading screen is not a Java screen. */
    @ModifyExpressionValue(method = "tick", at = @At(value = "CONSTANT", args = "classValue=net/minecraft/client/gui/screens/LevelLoadingScreen"))
    private boolean horizon$loading(boolean original) {
        return original || ClientMusic.loading;
    }
}
