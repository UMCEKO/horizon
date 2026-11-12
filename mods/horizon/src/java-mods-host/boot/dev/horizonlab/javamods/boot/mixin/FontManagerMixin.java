package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientResources;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;
import net.minecraft.client.gui.font.FontManager;
import net.minecraft.server.packs.resources.PreparableReloadListener;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Horizon's font draws every text and Java measures with its advances (`ClientLoader.fonts`), so the glyph providers this would load are never read. */
@Mixin(FontManager.class)
abstract class FontManagerMixin {
    @Inject(method = "reload", at = @At("HEAD"), cancellable = true)
    private void horizon$reload(PreparableReloadListener.SharedState state, Executor tasks, PreparableReloadListener.PreparationBarrier barrier, Executor main,
            CallbackInfoReturnable<CompletableFuture<Void>> result) {
        result.setReturnValue(ClientResources.skipped(barrier));
    }
}
