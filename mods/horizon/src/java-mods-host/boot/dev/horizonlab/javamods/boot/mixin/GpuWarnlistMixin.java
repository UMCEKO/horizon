package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.renderer.GpuWarnlistManager;
import net.minecraft.server.packs.resources.ResourceManager;
import net.minecraft.util.profiling.ProfilerFiller;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Coerce;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The list is matched against the GPU device's driver, and this client has no device: no warnings. */
@Mixin(GpuWarnlistManager.class)
abstract class GpuWarnlistMixin {
    @Inject(method = "apply(Lnet/minecraft/client/renderer/GpuWarnlistManager$Preparations;Lnet/minecraft/server/packs/resources/ResourceManager;Lnet/minecraft/util/profiling/ProfilerFiller;)V",
            at = @At("HEAD"), cancellable = true)
    private void horizon$apply(@Coerce Object preparations, ResourceManager manager, ProfilerFiller profiler, CallbackInfo info) {
        info.cancel();
    }
}
