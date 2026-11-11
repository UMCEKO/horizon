package dev.horizonlab.javamods.testmod;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.core.BlockPos;
import net.minecraft.world.item.context.UseOnContext;

/** `worldgen_export`: the final registries after a biome modifier, and the mod-owned types inside a configured feature. */
final class ChecksWorldgen {
    private ChecksWorldgen() {}

    static void run(UseOnContext context, BlockPos area) {
        Checks.check("worldgen_export", () -> {
            try {
                return exported();
            } catch (Exception e) {
                throw new IllegalStateException(e);
            }
        });
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
