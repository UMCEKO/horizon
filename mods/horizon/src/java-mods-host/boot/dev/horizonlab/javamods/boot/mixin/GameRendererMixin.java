package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.Camera;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.GameRenderer;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Rendering is native: this view has a state and a camera and nothing that draws. */
@Mixin(GameRenderer.class)
abstract class GameRendererMixin {
    @Shadow
    @Final
    private Camera mainCamera;

    /** Mods aim from the main camera, so its part of `tick` (its entity and eye height) runs; the lightmap and boss darkening stay native. */
    @Inject(method = "tick", at = @At("HEAD"), cancellable = true)
    private void horizon$tick(CallbackInfo info) {
        if (mainCamera.entity() == null) mainCamera.setEntity(Minecraft.getInstance().player);
        mainCamera.tick();
        info.cancel();
    }

    /** The camera needs the level to align with its entity; the lighting `setLevel` updates is native. */
    @Inject(method = "setLevel", at = @At("HEAD"), cancellable = true)
    private void horizon$level(ClientLevel level, CallbackInfo info) {
        mainCamera.setLevel(level);
        info.cancel();
    }

    @Inject(method = "checkEntityPostEffect", at = @At("HEAD"), cancellable = true)
    private void horizon$effect(net.minecraft.world.entity.Entity camera, CallbackInfo info) {
        info.cancel();
    }
}
