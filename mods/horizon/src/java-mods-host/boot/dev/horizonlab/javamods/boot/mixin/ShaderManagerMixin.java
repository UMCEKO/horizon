package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientResources;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;
import net.minecraft.client.renderer.ShaderManager;
import net.minecraft.server.packs.resources.PreparableReloadListener;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Its whole reload compiles pipelines on the GPU device, which this client hasn't: Horizon's renderer has its own shaders. */
@Mixin(ShaderManager.class)
abstract class ShaderManagerMixin {
    @Inject(method = "reload", at = @At("HEAD"), cancellable = true)
    private void horizon$reload(PreparableReloadListener.SharedState state, Executor tasks, PreparableReloadListener.PreparationBarrier barrier, Executor main,
            CallbackInfoReturnable<CompletableFuture<Void>> result) {
        result.setReturnValue(ClientResources.skipped(barrier));
    }
}
