package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.renderpearl.api.device.GpuDevice;
import com.mojang.renderpearl.api.pipeline.CompiledRenderPipeline;
import com.mojang.renderpearl.api.pipeline.RenderPipeline;
import dev.horizonlab.javamods.boot.WorldFrame;
import net.minecraft.client.renderer.DynamicGpuData;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** While a world frame records, the device, its pipelines and its dynamic uniforms are the recording's; otherwise this client has no GPU. */
@Mixin(RenderSystem.class)
abstract class RenderSystemMixin {
    @Inject(method = "getDevice", at = @At("HEAD"), cancellable = true)
    private static void horizon$device(CallbackInfoReturnable<GpuDevice> info) {
        if (WorldFrame.recording) info.setReturnValue(WorldFrame.DEVICE);
    }

    @Inject(method = "tryGetDevice", at = @At("HEAD"), cancellable = true)
    private static void horizon$tryDevice(CallbackInfoReturnable<GpuDevice> info) {
        if (WorldFrame.recording) info.setReturnValue(WorldFrame.DEVICE);
    }

    @Inject(method = "getDynamicUniforms", at = @At("HEAD"), cancellable = true)
    private static void horizon$uniforms(CallbackInfoReturnable<DynamicGpuData> info) {
        if (WorldFrame.recording) info.setReturnValue(WorldFrame.uniforms());
    }

    @Inject(method = "getCompiledPipelineNullable", at = @At("HEAD"), cancellable = true)
    private static void horizon$pipeline(RenderPipeline pipeline, CallbackInfoReturnable<CompiledRenderPipeline> info) {
        if (WorldFrame.recording) info.setReturnValue(WorldFrame.pipeline(pipeline));
    }
}
