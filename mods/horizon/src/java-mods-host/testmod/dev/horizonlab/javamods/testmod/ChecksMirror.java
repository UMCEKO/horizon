package dev.horizonlab.javamods.testmod;

import dev.horizonlab.javamods.boot.ChunkMirror;
import java.io.DataInputStream;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.Arrays;
import java.util.zip.GZIPInputStream;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.GenerationChunkHolder;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.WorldGenRegion;
import net.minecraft.util.StaticCache2D;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.PalettedContainerFactory;
import net.minecraft.world.level.chunk.ProtoChunk;
import net.minecraft.world.level.chunk.status.ChunkPyramid;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import org.jspecify.annotations.Nullable;

/** `ChunkMirror` on the chunk Rust's `a_decorated_chunk_crosses_the_wire_whole` wrote (`mirror/chunk.bin.gz`). */
final class ChecksMirror {
    private ChecksMirror() {}

    private static final LevelHeightAccessor OVERWORLD = LevelHeightAccessor.create(-64, 384);

    static void run(UseOnContext context, BlockPos area) {
        ServerLevel level = (ServerLevel) context.getLevel();
        byte[][] fixture = fixture();
        var factory = PalettedContainerFactory.create(level.registryAccess());
        var session = new ChunkMirror.Session(ChunkMirror.Header.read(fixture[0]), level.registryAccess(), null);
        ProtoChunk chunk = ChunkMirror.read(fixture[1], OVERWORLD, session, factory, true);
        Checks.check("mirror reads native's chunk", () -> chunk.getPersistedStatus() == ChunkStatus.FEATURES
            && chunk.getBlockState(new BlockPos(0, -64, 0)).is(Blocks.BEDROCK)
            && chunk.getBlockEntityNbts().get(new BlockPos(3, 70, 5)).getStringOr("id", "").equals("minecraft:chest")
            && chunk.getTicksForSerialization(0L).blocks().stream().anyMatch(t -> t.pos().equals(new BlockPos(4, 60, 9)) && t.type() == Blocks.STONE && t.delay() == 3)
            && session.log().size() == 0);
        Checks.check("mirror writes a chunk as it reads it", () -> {
            byte[] once = ChunkMirror.writeChunk(chunk, session);
            ProtoChunk again = ChunkMirror.read(once, OVERWORLD, session, factory, false);
            for (int y = -64; y < 320; y += 7) {
                BlockPos at = new BlockPos(y & 15, y, (y >> 4) & 15);
                if (again.getBlockState(at) != chunk.getBlockState(at) || again.getNoiseBiome(0, y >> 2, 0) != chunk.getNoiseBiome(0, y >> 2, 0)) return false;
            }
            return Arrays.equals(once, ChunkMirror.writeChunk(again, session));
        });
        Checks.check("mirror logs a region's writes at the chunk", () -> {
            ChunkAccess[] chunks = new ChunkAccess[9];
            for (int i = 0; i < 9; i++) {
                var pos = new ChunkPos(i % 3 - 1, i / 3 - 1);
                chunks[i] = i == 4 ? chunk : ChunkMirror.empty(pos, OVERWORLD, session, factory);
            }
            var holders = StaticCache2D.create(0, 0, 1, (x, z) -> holder(new ChunkPos(x, z), chunks[(z + 1) * 3 + x + 1]));
            var region = new WorldGenRegion(level, holders, ChunkPyramid.GENERATION_PYRAMID.getStepTo(ChunkStatus.FEATURES), chunk);
            BlockPos center = new BlockPos(1, 300, 1);
            BlockPos east = new BlockPos(17, 300, 1);
            region.setBlock(center, Blocks.STONE.defaultBlockState(), 3);
            region.getChunk(east).setBlockState(east, Blocks.GLASS.defaultBlockState(), 2);
            // Setting a block to what it already is logs nothing.
            region.setBlock(center, Blocks.STONE.defaultBlockState(), 3);
            var expected = new java.io.ByteArrayOutputStream();
            var out = new java.io.DataOutputStream(expected);
            try {
                out.writeInt(2);
                for (Object[] write : new Object[][] {{center, Blocks.STONE, 3}, {east, Blocks.GLASS, 2}}) {
                    out.writeByte(0);
                    out.writeLong(((BlockPos) write[0]).asLong());
                    out.writeInt(Block.BLOCK_STATE_REGISTRY.getId(((Block) write[1]).defaultBlockState()));
                    out.writeInt((int) write[2]);
                }
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
            return Arrays.equals(session.log().take(), expected.toByteArray()) && session.log().size() == 0;
        });
    }

    private static GenerationChunkHolder holder(ChunkPos pos, ChunkAccess chunk) {
        return new GenerationChunkHolder(pos) {
            @Override
            public @Nullable ChunkAccess getChunkIfPresentUnchecked(ChunkStatus status) {
                return chunk;
            }

            @Override
            public int getTicketLevel() {
                return 0;
            }

            @Override
            public int getQueueLevel() {
                return 0;
            }

            @Override
            protected void addSaveDependency(java.util.concurrent.CompletableFuture<?> future) {}
        };
    }

    /** The session header's bytes, then the chunk's. */
    private static byte[][] fixture() {
        try (var in = new DataInputStream(new GZIPInputStream(ChecksMirror.class.getResourceAsStream("/mirror/chunk.bin.gz")))) {
            byte[] header = in.readNBytes(in.readInt());
            byte[] chunk = in.readNBytes(in.readInt());
            return new byte[][] {header, chunk};
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }
}
