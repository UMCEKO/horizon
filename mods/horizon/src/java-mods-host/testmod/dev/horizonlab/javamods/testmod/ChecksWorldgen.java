package dev.horizonlab.javamods.testmod;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;

/** `worldgen_export`: the final registries after a biome modifier, and the mod-owned types inside a configured feature; each level's generation context. */
final class ChecksWorldgen {
    private ChecksWorldgen() {}

    static void run(UseOnContext context, BlockPos area) {
        Checks.check("world_start_order", WorldStart::inOrder);
        Checks.check("worldgen_context", () -> generationContext(context, area));
        Checks.check("worldgen_export", () -> {
            try {
                return exported();
            } catch (Exception e) {
                throw new IllegalStateException(e);
            }
        });
    }

    /** The overworld's generator is the stem's noise generator, with a random state and a structure manager that answers. */
    private static boolean generationContext(UseOnContext context, BlockPos area) {
        if (!(context.getLevel() instanceof ServerLevel level)) return false;
        ServerLevel overworld = level.getServer().getLevel(Level.OVERWORLD);
        var chunks = overworld.getChunkSource();
        overworld.structureManager().hasAnyStructureAt(area);
        return chunks.getGenerator() instanceof NoiseBasedChunkGenerator && chunks.randomState() != null && chunks.getGeneratorState() != null;
    }

    private static boolean exported() throws Exception {
        Path file = Files.createTempFile("worldgen-export", ".json");
        try {
            var export = Class.forName("dev.horizonlab.javamods.boot.WorldgenExport", true, ChecksWorldgen.class.getClassLoader());
            export.getMethod("write", String.class).invoke(null, file.toString());
            JsonObject root = JsonParser.parseString(Files.readString(file)).getAsJsonObject();
            JsonObject biomes = root.getAsJsonObject("entries").getAsJsonObject("worldgen/biome");
            var steps = biomes.getAsJsonObject("horizon_test:export_biome").getAsJsonArray("features");
            boolean diamond = steps.asList().stream().anyMatch(step -> step.getAsJsonArray().asList().stream().anyMatch(id -> id.getAsString().equals("minecraft:ore_diamond")));
            var owners = root.getAsJsonObject("owners").getAsJsonObject("worldgen/feature").getAsJsonArray("horizon_test:export_feature");
            boolean owned = owners.asList().stream().anyMatch(id -> id.getAsString().equals("minecraft:worldgen/feature_type:horizon_java_test:export"));
            return diamond && owned && !biomes.has("minecraft:plains");
        } finally {
            Files.deleteIfExists(file);
        }
    }
}
