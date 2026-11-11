package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.platform.NativeImage;
import dev.horizonlab.javamods.boot.ResourceTextures;
import net.minecraft.client.renderer.texture.AbstractTexture;
import net.minecraft.client.renderer.texture.ReloadableTexture;
import net.minecraft.client.renderer.texture.TextureContents;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(ReloadableTexture.class)
abstract class ReloadableTextureMixin extends AbstractTexture {
    @Unique private boolean horizon$repeat;
    @Unique private boolean horizon$blur;

    @Inject(method = "apply", at = @At("HEAD"))
    private void horizon$sampler(TextureContents contents, CallbackInfo info) {
        horizon$repeat = !contents.clamp();
        horizon$blur = contents.blur();
    }

    @Inject(method = "doLoad", at = @At("HEAD"), cancellable = true)
    private void horizon$upload(NativeImage image, CallbackInfo info) {
        var captured = ResourceTextures.load(image, horizon$repeat, horizon$blur);
        close();
        texture = captured;
        textureView = captured;
        info.cancel();
    }
}
