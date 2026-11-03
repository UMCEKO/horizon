package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.Heightmap;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** A chunk view holds no heightmaps: its level's heights are its own, one below `Level.getHeight`. */
@Mixin(ChunkAccess.class)
abstract class ChunkHeightMixin {
    @Inject(method = "getHeight(Lnet/minecraft/world/level/levelgen/Heightmap$Types;II)I", at = @At("HEAD"), cancellable = true)
    private void horizon$height(Heightmap.Types type, int x, int z, CallbackInfoReturnable<Integer> result) {
        if (!((Object) this instanceof LevelChunk chunk) || chunk.getLevel() == null) return;
        var pos = chunk.getPos();
        result.setReturnValue(chunk.getLevel().getHeight(type, pos.getMinBlockX() + (x & 15), pos.getMinBlockZ() + (z & 15)) - 1);
    }
}
