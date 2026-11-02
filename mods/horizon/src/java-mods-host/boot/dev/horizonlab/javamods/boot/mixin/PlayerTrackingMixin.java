package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.server.level.ChunkTrackingView;
import net.minecraft.server.level.ServerPlayer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Horizon streams chunks itself; in Java a player sees every chunk around it, so mods' updates for them are sent. */
@Mixin(ServerPlayer.class)
abstract class PlayerTrackingMixin {
    @Inject(method = "getChunkTrackingView", at = @At("HEAD"), cancellable = true)
    private void horizon$view(CallbackInfoReturnable<ChunkTrackingView> result) {
        result.setReturnValue(ChunkTrackingView.of(((ServerPlayer) (Object) this).chunkPosition(), 32));
    }
}
