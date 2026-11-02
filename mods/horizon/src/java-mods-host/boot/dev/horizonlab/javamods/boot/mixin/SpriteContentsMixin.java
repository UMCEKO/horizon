package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.platform.NativeImage;
import com.mojang.blaze3d.platform.Transparency;
import net.minecraft.client.renderer.texture.SpriteContents;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Horizon's sprites hold no pixels in Java: a region's transparency is the whole sprite's. */
@Mixin(SpriteContents.class)
abstract class SpriteContentsMixin {
    @Shadow @Final private NativeImage originalImage;
    @Shadow @Final private Transparency transparency;

    @Inject(method = "computeTransparency(FFFF)Lcom/mojang/blaze3d/platform/Transparency;", at = @At("HEAD"), cancellable = true)
    private void horizon$transparency(float u0, float v0, float u1, float v1, CallbackInfoReturnable<Transparency> result) {
        if (originalImage == null) result.setReturnValue(transparency);
    }
}
