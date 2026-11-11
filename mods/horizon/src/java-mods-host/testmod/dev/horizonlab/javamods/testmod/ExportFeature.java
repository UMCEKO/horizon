package dev.horizonlab.javamods.testmod;

import com.mojang.serialization.MapCodec;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.WorldGenLevel;
import net.minecraft.world.level.chunk.ChunkGenerator;
import net.minecraft.world.level.levelgen.feature.Feature;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.registries.DeferredRegister;

/** A mod's own `Feature` type, for the worldgen export's owners. */
public record ExportFeature() implements Feature {
    public static final MapCodec<ExportFeature> CODEC = MapCodec.unit(ExportFeature::new);

    static void register(IEventBus bus) {
        var types = DeferredRegister.<MapCodec<? extends Feature>>create(Registries.FEATURE_TYPE, "horizon_java_test");
        types.register("export", () -> CODEC);
        types.register(bus);
    }

    @Override
    public MapCodec<ExportFeature> codec() {
        return CODEC;
    }

    @Override
    public boolean place(WorldGenLevel level, ChunkGenerator generator, RandomSource random, BlockPos origin) {
        return false;
    }
}
