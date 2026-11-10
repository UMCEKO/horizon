package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.Tracking;
import java.util.function.Predicate;
import net.minecraft.network.protocol.Packet;
import net.minecraft.server.level.ChunkHolder;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** No chunk holders or tracked entities in Java: Horizon's hub sends chunks and pairs entities, and `Tracking` knows what it did. */
@Mixin(ChunkMap.class)
abstract class ChunkMapMixin {
    @Inject(method = "getVisibleChunkIfPresent", at = @At("HEAD"), cancellable = true)
    private void horizon$holder(long key, CallbackInfoReturnable<ChunkHolder> result) {
        result.setReturnValue(null);
    }

    @Shadow
    @Final
    ServerLevel level;

    @Inject(method = "isChunkTracked", at = @At("HEAD"), cancellable = true)
    private void horizon$tracked(ServerPlayer player, int x, int z, CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(player.getChunkTrackingView().contains(x, z) && Tracking.sent(player, level, x, z));
    }

    @Inject(method = "sendToTrackingPlayers", at = @At("HEAD"), cancellable = true)
    private void horizon$tracking(Entity entity, Packet<?> packet, CallbackInfo info) {
        Tracking.sendToTracking(entity, packet, player -> true);
        info.cancel();
    }

    @Inject(method = "sendToTrackingPlayersFiltered", at = @At("HEAD"), cancellable = true)
    private void horizon$trackingFiltered(Entity entity, Packet<?> packet, Predicate<ServerPlayer> filter, CallbackInfo info) {
        Tracking.sendToTracking(entity, packet, filter);
        info.cancel();
    }

    @Inject(method = "sendToTrackingPlayersAndSelf", at = @At("HEAD"), cancellable = true)
    private void horizon$trackingAndSelf(Entity entity, Packet<?> packet, CallbackInfo info) {
        Tracking.sendToTrackingAndSelf(entity, packet);
        info.cancel();
    }

    @Inject(method = "isTrackedByAnyPlayer", at = @At("HEAD"), cancellable = true)
    private void horizon$trackedByAny(Entity entity, CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(!Tracking.untracked(entity));
    }
}
