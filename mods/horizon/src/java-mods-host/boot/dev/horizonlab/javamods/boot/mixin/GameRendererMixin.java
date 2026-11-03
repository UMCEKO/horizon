package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.GameRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Rendering is native: this view has a state and a camera and nothing that draws. */
@Mixin(GameRenderer.class)
abstract class GameRendererMixin {
    @Inject(method = "tick", at = @At("HEAD"), cancellable = true)
    private void horizon$tick(CallbackInfo info) {
        info.cancel();
    }

    @Inject(method = "setLevel", at = @At("HEAD"), cancellable = true)
    private void horizon$level(ClientLevel level, CallbackInfo info) {
        info.cancel();
    }

    @Inject(method = "checkEntityPostEffect", at = @At("HEAD"), cancellable = true)
    private void horizon$effect(net.minecraft.world.entity.Entity camera, CallbackInfo info) {
        info.cancel();
    }
}
