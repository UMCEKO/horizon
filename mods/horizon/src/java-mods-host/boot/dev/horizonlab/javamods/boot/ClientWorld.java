package dev.horizonlab.javamods.boot;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.Heightmap;

/** The client level's chunks are the window's, which hold what the server sent this client: each read is bridged to them. */
public final class ClientWorld {
    private ClientWorld() {}

    public static BlockState state(BlockPos pos) {
        throw new IllegalStateException("not bridged");
    }

    /** `Level.getHeight`. */
    public static int height(Heightmap.Types type, int x, int z) {
        throw new IllegalStateException("not bridged");
    }

    public static int brightness(LightLayer layer, BlockPos pos) {
        throw new IllegalStateException("not bridged");
    }

    public static int rawBrightness(BlockPos pos, int darken) {
        throw new IllegalStateException("not bridged");
    }

    public static Holder<Biome> biome(int x, int y, int z) {
        throw new IllegalStateException("not bridged");
    }
}
