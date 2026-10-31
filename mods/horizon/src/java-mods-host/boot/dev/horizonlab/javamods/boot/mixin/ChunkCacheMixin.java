package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.network.protocol.Packet;
import net.minecraft.server.level.ServerChunkCache;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Horizon's tracker tells clients about its entities, so these have no one to tell. */
@Mixin(ServerChunkCache.class)
abstract class ChunkCacheMixin {
    @Inject(method = "sendToTrackingPlayers", at = @At("HEAD"), cancellable = true)
    private void horizon$tracking(Entity entity, Packet<?> packet, CallbackInfo info) {
        info.cancel();
    }

    @Inject(method = "sendToTrackingPlayersAndSelf", at = @At("HEAD"), cancellable = true)
    private void horizon$trackingAndSelf(Entity entity, Packet<?> packet, CallbackInfo info) {
        info.cancel();
    }
}
