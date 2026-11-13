package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.gui.components.DebugScreenOverlay;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.chunk.LevelChunk;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;
import org.spongepowered.asm.mixin.gen.Invoker;

@Mixin(DebugScreenOverlay.class)
public interface DebugOverlayAccess {
    @Accessor("lastPos")
    ChunkPos horizon$lastPos();

    @Accessor("lastPos")
    void horizon$lastPos(ChunkPos pos);

    @Invoker("getLevel")
    Level horizon$level();

    @Invoker("getClientChunk")
    LevelChunk horizon$clientChunk();

    @Invoker("getServerChunk")
    LevelChunk horizon$serverChunk();
}
