package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.platform.NativeImage;
import com.mojang.blaze3d.platform.Transparency;
import dev.horizonlab.javamods.boot.SpriteMasks;
import net.minecraft.client.renderer.texture.SpriteContents;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/** A sprite whose pixels were freed after the stitch answers vanilla's transparency questions from its mask. */
@Mixin(SpriteContents.class)
abstract class SpriteMaskMixin {
    @Redirect(method = "isTransparent", at = @At(value = "INVOKE", target = "Lcom/mojang/blaze3d/platform/NativeImage;getPixel(II)I"))
    private int horizon$pixel(NativeImage image, int x, int y) {
        var mask = SpriteMasks.of((SpriteContents) (Object) this);
        return mask == null ? image.getPixel(x, y) : mask.alpha(x, y) << 24;
    }

    @Redirect(method = "computeTransparency", at = @At(value = "INVOKE", target = "Lcom/mojang/blaze3d/platform/NativeImage;computeTransparency(IIII)Lcom/mojang/blaze3d/platform/Transparency;"))
    private Transparency horizon$transparency(NativeImage image, int x0, int y0, int x1, int y1) {
        var mask = SpriteMasks.of((SpriteContents) (Object) this);
        return mask == null ? image.computeTransparency(x0, y0, x1, y1) : mask.transparency(x0, y0, x1, y1);
    }
}
