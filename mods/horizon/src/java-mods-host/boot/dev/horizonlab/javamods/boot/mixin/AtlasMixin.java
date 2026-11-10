package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.systems.SamplerCache;
import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.device.DeviceInfo;
import com.mojang.renderpearl.api.device.DeviceLimits;
import com.mojang.renderpearl.api.device.GpuDevice;
import com.mojang.renderpearl.api.textures.FilterMode;
import com.mojang.renderpearl.api.textures.GpuSampler;
import dev.horizonlab.javamods.boot.Sprites;
import net.minecraft.client.renderer.texture.TextureAtlas;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** An atlas is stitched here as on a real client and only its GPU texture is left out: Rust draws from its own, and the sprites' layout stays for Java. */
@Mixin(TextureAtlas.class)
abstract class AtlasMixin {
    @Shadow @Final private Identifier location;
    @Shadow private int width;
    @Shadow private int height;
    @Shadow private int maxMipLevel;
    @Shadow private int mipLevelCount;

    @Redirect(method = "<init>", at = @At(value = "INVOKE", target = "Lcom/mojang/blaze3d/systems/RenderSystem;getDevice()Lcom/mojang/renderpearl/api/device/GpuDevice;"))
    private GpuDevice horizon$device() {
        return null;
    }

    @Redirect(method = "<init>", at = @At(value = "INVOKE", target = "Lcom/mojang/renderpearl/api/device/GpuDevice;getDeviceInfo()Lcom/mojang/renderpearl/api/device/DeviceInfo;"))
    private DeviceInfo horizon$info(GpuDevice device) {
        return null;
    }

    @Redirect(method = "<init>", at = @At(value = "INVOKE", target = "Lcom/mojang/renderpearl/api/device/DeviceInfo;limits()Lcom/mojang/renderpearl/api/device/DeviceLimits;"))
    private DeviceLimits horizon$limits(DeviceInfo info) {
        return null;
    }

    /** No texture is uploaded, so only the stitcher reads this: the largest size GPUs report. */
    @Redirect(method = "<init>", at = @At(value = "INVOKE", target = "Lcom/mojang/renderpearl/api/device/DeviceLimits;maxTextureSizeForFormat(Lcom/mojang/renderpearl/api/GpuFormat;)I"))
    private int horizon$maxSize(DeviceLimits limits, GpuFormat format) {
        return 32768;
    }

    @Inject(method = "createTexture", at = @At("HEAD"), cancellable = true)
    private void horizon$texture(int newWidth, int newHeight, int newMipLevel, CallbackInfo info) {
        width = newWidth;
        height = newHeight;
        maxMipLevel = newMipLevel;
        mipLevelCount = newMipLevel + 1;
        Sprites.stitched((TextureAtlas) (Object) this, location, newWidth, newHeight);
        info.cancel();
    }

    @Redirect(method = "upload", at = @At(value = "INVOKE", target = "Lcom/mojang/blaze3d/systems/SamplerCache;getClampToEdge(Lcom/mojang/renderpearl/api/textures/FilterMode;)Lcom/mojang/renderpearl/api/textures/GpuSampler;"))
    private GpuSampler horizon$sampler(SamplerCache cache, FilterMode filter) {
        return null;
    }

    /** Animation runs on the GPU's copy of the atlas; Rust animates its own. */
    @Redirect(method = "upload", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/renderer/texture/TextureAtlasSprite;isAnimated()Z"))
    private boolean horizon$animated(TextureAtlasSprite sprite) {
        return false;
    }

    @Inject(method = "uploadInitialContents", at = @At("HEAD"), cancellable = true)
    private void horizon$contents(CallbackInfo info) {
        info.cancel();
    }
}
