package dev.horizonlab.javamods.boot.mixin;

import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import com.mojang.renderpearl.api.device.GpuDevice;
import java.util.function.Supplier;
import net.minecraft.client.renderer.ProjectionMatrixBuffer;
import org.joml.Matrix4f;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Mods hold projection buffers for their own scenes, and this client has no GPU to give them one: Rust draws. */
@Mixin(ProjectionMatrixBuffer.class)
abstract class ProjectionBufferMixin {
    @Redirect(method = "<init>", at = @At(value = "INVOKE", target = "Lcom/mojang/blaze3d/systems/RenderSystem;getDevice()Lcom/mojang/renderpearl/api/device/GpuDevice;"))
    private GpuDevice horizon$device() {
        return null;
    }

    @Redirect(method = "<init>", at = @At(value = "INVOKE", target = "Lcom/mojang/renderpearl/api/device/GpuDevice;createBuffer(Ljava/util/function/Supplier;IJ)Lcom/mojang/renderpearl/api/buffers/GpuBuffer;"))
    private GpuBuffer horizon$buffer(GpuDevice device, Supplier<String> label, int usage, long size) {
        return null;
    }

    @Redirect(method = "<init>", at = @At(value = "INVOKE", target = "Lcom/mojang/renderpearl/api/buffers/GpuBuffer;slice(JJ)Lcom/mojang/renderpearl/api/buffers/GpuBufferSlice;"))
    private GpuBufferSlice horizon$slice(GpuBuffer buffer, long offset, long length) {
        return null;
    }

    @Inject(method = "writeBuffer", at = @At("HEAD"), cancellable = true)
    private void horizon$write(Matrix4f matrix, CallbackInfoReturnable<GpuBufferSlice> info) {
        info.setReturnValue(null);
    }

    @Inject(method = "close", at = @At("HEAD"), cancellable = true)
    private void horizon$close(CallbackInfo info) {
        info.cancel();
    }
}
