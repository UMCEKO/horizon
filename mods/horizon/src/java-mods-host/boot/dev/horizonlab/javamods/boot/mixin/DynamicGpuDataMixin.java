package dev.horizonlab.javamods.boot.mixin;

import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import dev.horizonlab.javamods.boot.WorldFrame;
import net.minecraft.client.renderer.DynamicGpuData;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** A transform written while a world frame records is kept as itself for the draw that binds it. */
@Mixin(DynamicGpuData.class)
abstract class DynamicGpuDataMixin {
    @Inject(method = "writeTransform(Lnet/minecraft/client/renderer/DynamicGpuData$Transform;)Lcom/mojang/renderpearl/api/buffers/GpuBufferSlice;", at = @At("HEAD"), cancellable = true)
    private void horizon$transform(DynamicGpuData.Transform transform, CallbackInfoReturnable<GpuBufferSlice> info) {
        if (WorldFrame.recording) info.setReturnValue(WorldFrame.transform(transform));
    }

    @Inject(method = "writeTransforms", at = @At("HEAD"), cancellable = true)
    private void horizon$transforms(DynamicGpuData.Transform[] transforms, CallbackInfoReturnable<GpuBufferSlice[]> info) {
        if (!WorldFrame.recording) return;
        var slices = new GpuBufferSlice[transforms.length];
        for (int i = 0; i < slices.length; i++) slices[i] = WorldFrame.transform(transforms[i]);
        info.setReturnValue(slices);
    }
}
