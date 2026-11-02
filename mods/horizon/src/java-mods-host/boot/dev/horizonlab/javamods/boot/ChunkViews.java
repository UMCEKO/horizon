package dev.horizonlab.javamods.boot;

import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.Map;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.LevelChunkSection;

/** `LevelChunk`s with no storage of their own, for chunks Horizon has loaded: what a mod does through one goes to the level, which Rust serves. */
public final class ChunkViews {
    private ChunkViews() {}

    private static final Map<ServerLevel, Long2ObjectOpenHashMap<LevelChunk>> VIEWS = new IdentityHashMap<>();

    public static boolean loaded(ServerLevel level, int x, int z) {
        return level.isLoaded(new BlockPos(x << 4, level.getMinY(), z << 4));
    }

    public static LevelChunk get(ServerLevel level, int x, int z) {
        if (!loaded(level, x, z)) return null;
        return VIEWS.computeIfAbsent(level, l -> new Long2ObjectOpenHashMap<>()).computeIfAbsent(ChunkPos.pack(x, z), key -> view(level, x, z));
    }

    private static LevelChunk view(ServerLevel level, int x, int z) {
        try {
            var chunk = GameBridge.construct(LevelChunk.class, Object.class.getDeclaredConstructor());
            ServerView.set(LevelChunk.class, chunk, "level", level);
            ServerView.set(LevelChunk.class, chunk, "unsavedListener", (LevelChunk.UnsavedListener) pos -> {});
            ServerView.set(ChunkAccess.class, chunk, "chunkPos", new ChunkPos(x, z));
            ServerView.set(ChunkAccess.class, chunk, "levelHeightAccessor", level);
            ServerView.set(ChunkAccess.class, chunk, "blockEntities", new HashMap<>());
            ServerView.set(ChunkAccess.class, chunk, "sections", new LevelChunkSection[0]);
            Inits.run(chunk, level, new ChunkPos(x, z));
            return chunk;
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }
}
