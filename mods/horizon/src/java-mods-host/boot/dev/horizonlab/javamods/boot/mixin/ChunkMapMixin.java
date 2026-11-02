package dev.horizonlab.javamods.boot.mixin;

import java.util.List;
import net.minecraft.server.level.ChunkHolder;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.ChunkPos;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
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

    @Shadow
    @Final
    ServerLevel level;

    /** `isChunkTracked` without the chunk sender, which sends nothing here: Horizon streams the chunks. */
    @Inject(method = "isChunkTracked", at = @At("HEAD"), cancellable = true)
    private void horizon$tracked(ServerPlayer player, int x, int z, CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(player.getChunkTrackingView().contains(x, z));
    }
}
