package dev.horizonlab.javamods.boot.mixin;

import java.util.List;
import net.minecraft.server.level.ChunkHolder;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.ChunkPos;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** No chunk holders in Java: Horizon's streaming tells clients about chunks. */
@Mixin(ChunkMap.class)
abstract class ChunkMapMixin {
    @Inject(method = "getVisibleChunkIfPresent", at = @At("HEAD"), cancellable = true)
    private void horizon$holder(long key, CallbackInfoReturnable<ChunkHolder> result) {
        result.setReturnValue(null);
    }

    @Inject(method = "getPlayers", at = @At("HEAD"), cancellable = true)
    private void horizon$players(ChunkPos pos, boolean boundaryOnly, CallbackInfoReturnable<List<ServerPlayer>> result) {
        result.setReturnValue(List.of());
    }
}
