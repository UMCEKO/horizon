package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.util.concurrent.CompletableFuture;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.StreamTagVisitor;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.storage.ChunkScanAccess;

/** A level's `StructureCheck` storage: the structures native has for a column, loaded or saved, instead of the region files. */
final class NativeChunkScan implements ChunkScanAccess {
    private final String level;

    NativeChunkScan(String level) {
        this.level = level;
    }

    /** Bridged: the column's `DataVersion` and `structures` as a named compound; empty when native never had the column. */
    static byte[] structures(String level, long chunk) {
        throw new IllegalStateException("not bridged");
    }

    @Override
    public CompletableFuture<Void> scanChunk(ChunkPos pos, StreamTagVisitor visitor) {
        try {
            byte[] bytes = structures(level, pos.pack());
            if (bytes.length > 0) NbtIo.read(new DataInputStream(new ByteArrayInputStream(bytes))).acceptAsRoot(visitor);
            return CompletableFuture.completedFuture(null);
        } catch (Throwable t) {
            return CompletableFuture.failedFuture(t);
        }
    }
}
