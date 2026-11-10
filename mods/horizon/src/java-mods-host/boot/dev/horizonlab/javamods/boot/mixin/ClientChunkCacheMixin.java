package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ChunkViews;
import net.minecraft.client.multiplayer.ClientChunkCache;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The window's chunks are Horizon's; Java's are views that read through the level. */
@Mixin(ClientChunkCache.class)
abstract class ClientChunkCacheMixin {
    @Shadow @Final private ClientLevel level;

    @Inject(method = "getChunk(IILnet/minecraft/world/level/chunk/status/ChunkStatus;Z)Lnet/minecraft/world/level/chunk/LevelChunk;", at = @At("HEAD"), cancellable = true)
    private void horizon$chunk(int x, int z, ChunkStatus status, boolean load, CallbackInfoReturnable<LevelChunk> result) {
        result.setReturnValue(ChunkViews.get(level, x, z));
    }
}
