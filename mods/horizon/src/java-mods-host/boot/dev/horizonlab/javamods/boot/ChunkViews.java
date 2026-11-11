package dev.horizonlab.javamods.boot;

import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.Map;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.FullChunkStatus;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.LevelChunkSection;
import net.neoforged.neoforge.attachment.AttachmentHolder;

/** `LevelChunk`s with no storage of their own, for chunks Horizon has loaded: what a mod does through one goes to the level, which Rust serves. */
public final class ChunkViews {
    private ChunkViews() {}

    /** Each side's own, touched only by its thread: the server's levels, then the Java client's. */
    private static final Map<Level, Long2ObjectOpenHashMap<LevelChunk>> SERVER = new IdentityHashMap<>();
    private static final Map<Level, Long2ObjectOpenHashMap<LevelChunk>> CLIENT = new IdentityHashMap<>();

    private static Map<Level, Long2ObjectOpenHashMap<LevelChunk>> views(Level level) {
        return level instanceof ServerLevel ? SERVER : CLIENT;
    }

    /** The server's chunks are those Horizon's server has loaded; the client's are those its chunk cache was told of, as `ClientChunkCache` holds only what it received. */
    public static boolean loaded(Level level, int x, int z) {
        return level instanceof ServerLevel server ? server.isLoaded(new BlockPos(x << 4, level.getMinY(), z << 4)) : ClientBridge.hasChunk(x, z);
    }

    public static LevelChunk get(Level level, int x, int z) {
        if (!loaded(level, x, z)) return null;
        return views(level).computeIfAbsent(level, l -> new Long2ObjectOpenHashMap<>()).computeIfAbsent(ChunkPos.pack(x, z), key -> view(level, x, z));
    }

    /** `ServerChunkCache.getChunk` with `load`: the server thread waits until Horizon has it, as `managedBlock` does; null when Horizon couldn't load it. */
    public static LevelChunk getOrLoad(ServerLevel level, int x, int z) {
        return loaded(level, x, z) || load(x, z) ? get(level, x, z) : null;
    }

    /** Bridged: Horizon's streaming loads this chunk of the level in place before returning. */
    private static boolean load(int x, int z) {
        throw new IllegalStateException("not bridged");
    }

    /** The view a column already has, without making one. */
    static LevelChunk peek(Level level, int x, int z) {
        var views = views(level).get(level);
        return views == null ? null : views.get(ChunkPos.pack(x, z));
    }

    /** The column's view whether or not the level still has it, as a chunk being unloaded is still a `LevelChunk`. */
    static LevelChunk existing(Level level, int x, int z) {
        var view = peek(level, x, z);
        return view != null ? view : view(level, x, z);
    }

    /** The server's world closed; the client forgets its own levels as they change. */
    static void reset() {
        SERVER.clear();
    }

    static void forget(Level level) {
        views(level).remove(level);
    }

    /** The chunk unloaded: a later `getChunk` gets a new `LevelChunk`, as vanilla's would. */
    static void forget(Level level, int x, int z) {
        var views = views(level).get(level);
        if (views != null) views.remove(ChunkPos.pack(x, z));
    }

    private static LevelChunk view(Level level, int x, int z) {
        try {
            var chunk = GameBridge.construct(LevelChunk.class, Object.class.getDeclaredConstructor());
            ServerView.set(LevelChunk.class, chunk, "level", level);
            ServerView.set(LevelChunk.class, chunk, "unsavedListener", (LevelChunk.UnsavedListener) pos -> {});
            ServerView.set(ChunkAccess.class, chunk, "chunkPos", new ChunkPos(x, z));
            ServerView.set(ChunkAccess.class, chunk, "levelHeightAccessor", level);
            ServerView.set(ChunkAccess.class, chunk, "blockEntities", new HashMap<>());
            ServerView.set(LevelChunk.class, chunk, "tickersInLevel", new HashMap<>());
            ServerView.set(ChunkAccess.class, chunk, "sections", new LevelChunkSection[0]);
            ServerView.set(ChunkAccess.class, chunk, "attachmentHolder", new AttachmentHolder.AsField(chunk));
            Inits.run(chunk, level, new ChunkPos(x, z));
            if (level instanceof ServerLevel server) {
                long pos = ChunkPos.pack(x, z);
                chunk.setFullStatus(() -> server.shouldTickBlocksAt(pos) ? FullChunkStatus.BLOCK_TICKING : FullChunkStatus.FULL);
            }
            return chunk;
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }
}
