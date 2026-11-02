package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ChunkViews;
import net.minecraft.network.protocol.Packet;
import net.minecraft.server.level.ServerChunkCache;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Horizon's tracker tells clients about its entities, so these have no one to tell; its chunks are `ChunkViews`. */
@Mixin(ServerChunkCache.class)
abstract class ChunkCacheMixin {
    @Shadow @Final private ServerLevel level;

    @Inject(method = "getChunk(IILnet/minecraft/world/level/chunk/status/ChunkStatus;Z)Lnet/minecraft/world/level/chunk/ChunkAccess;", at = @At("HEAD"), cancellable = true)
    private void horizon$chunk(int x, int z, ChunkStatus status, boolean load, CallbackInfoReturnable<ChunkAccess> result) {
        result.setReturnValue(ChunkViews.get(level, x, z));
    }

    @Inject(method = "getChunkNow", at = @At("HEAD"), cancellable = true)
    private void horizon$chunkNow(int x, int z, CallbackInfoReturnable<LevelChunk> result) {
        result.setReturnValue(ChunkViews.get(level, x, z));
    }

    @Inject(method = "hasChunk", at = @At("HEAD"), cancellable = true)
    private void horizon$hasChunk(int x, int z, CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(ChunkViews.loaded(level, x, z));
    }

    /** Horizon decides which chunks tick; one it has loaded is ticking here, as there are no chunk holders to ask. */
    @Inject(method = "isPositionTicking", at = @At("HEAD"), cancellable = true)
    private void horizon$ticking(long chunk, CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(ChunkViews.loaded(level, net.minecraft.world.level.ChunkPos.getX(chunk), net.minecraft.world.level.ChunkPos.getZ(chunk)));
    }

    /** `ServerChunkCache.blockChanged`: no chunk holder here, so the position goes to the windows' clients' next broadcast. */
    @Inject(method = "blockChanged", at = @At("HEAD"))
    private void horizon$blockChanged(net.minecraft.core.BlockPos pos, CallbackInfo info) {
        dev.horizonlab.javamods.boot.Clients.blockChanged(level, pos);
    }

    @Inject(method = "sendToTrackingPlayers", at = @At("HEAD"), cancellable = true)
    private void horizon$tracking(Entity entity, Packet<?> packet, CallbackInfo info) {
        info.cancel();
    }

    @Inject(method = "sendToTrackingPlayersAndSelf", at = @At("HEAD"), cancellable = true)
    private void horizon$trackingAndSelf(Entity entity, Packet<?> packet, CallbackInfo info) {
        info.cancel();
    }
}
