package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ChunkViews;
import net.minecraft.server.level.ChunkHolder;
import net.minecraft.server.level.FullChunkStatus;
import net.minecraft.world.level.chunk.LevelChunk;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** A holder `ChunkViews` made has no futures: it answers with Horizon's loaded chunk while its ticket level reaches the status. */
@Mixin(ChunkHolder.class)
abstract class ChunkHolderViewMixin {
    @Inject(method = "getTickingChunk", at = @At("HEAD"), cancellable = true)
    private void horizon$ticking(CallbackInfoReturnable<LevelChunk> result) {
        var holder = (ChunkHolder) (Object) this;
        if (ChunkViews.isHolderView(holder)) result.setReturnValue(ChunkViews.holderChunk(holder, FullChunkStatus.BLOCK_TICKING));
    }

    @Inject(method = "getChunkToSend", at = @At("HEAD"), cancellable = true)
    private void horizon$send(CallbackInfoReturnable<LevelChunk> result) {
        var holder = (ChunkHolder) (Object) this;
        if (ChunkViews.isHolderView(holder)) result.setReturnValue(ChunkViews.holderChunk(holder, FullChunkStatus.BLOCK_TICKING));
    }
}
