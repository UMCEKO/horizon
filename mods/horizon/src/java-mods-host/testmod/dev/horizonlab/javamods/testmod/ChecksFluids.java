package dev.horizonlab.javamods.testmod;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.core.BlockPos;
import net.minecraft.world.item.context.UseOnContext;

/** What the boot's facts file says of the test fluid and of vanilla water, against numbers set in Goo and read from vanilla's source. */
final class ChecksFluids {
    private ChecksFluids() {}

    private static JsonObject facts() {
        try {
            return JsonParser.parseString(Files.readString(Path.of(System.getProperty("horizon.facts")))).getAsJsonObject();
        } catch (IOException e) {
            throw new java.io.UncheckedIOException(e);
        }
    }

    private static JsonObject baked() {
        try {
            return JsonParser.parseString(Files.readString(Path.of(System.getProperty("horizon.facts").replaceFirst("\\.[^./]*$", ".baked.json")))).getAsJsonObject();
        } catch (IOException e) {
            throw new java.io.UncheckedIOException(e);
        }
    }

    static void run(UseOnContext context, BlockPos area) {
        Checks.check("goo_facts", () -> {
            JsonObject states = facts().getAsJsonObject("fluids").getAsJsonObject("horizon_java_test:flowing_goo").getAsJsonObject("states");
            if (states.size() != 16) return false;
            for (int n = 1; n <= 8; n++) {
                String still = states.getAsJsonObject("amount=" + n + ",falling=false").get("block").getAsString();
                String falling = states.getAsJsonObject("amount=" + n + ",falling=true").get("block").getAsString();
                if (!still.equals("horizon_java_test:goo[level=" + (8 - n) + "]")) return false;
                if (!falling.equals("horizon_java_test:goo[level=" + (16 - n) + "]")) return false;
            }
            return states.getAsJsonObject("amount=8,falling=true").get("block").getAsString().equals("horizon_java_test:goo[level=8]");
        });
        Checks.check("goo_model", () -> {
            JsonObject goo = baked().getAsJsonObject("fluids").getAsJsonObject("horizon_java_test:goo");
            var water = net.minecraft.client.Minecraft.getInstance().getModelManager().getFluidStateModelSet().get(net.minecraft.world.level.material.Fluids.WATER.defaultFluidState());
            JsonObject tint = goo.getAsJsonObject("tint");
            JsonObject colors = tint.getAsJsonObject("states");
            return goo.get("still").getAsString().equals("minecraft:block/water_still") && goo.get("flowing").getAsString().equals("minecraft:block/water_flow") && goo.get("overlay").isJsonNull()
                && goo.get("layer").getAsString().equals(water.layer().name())
                && tint.get("kind").getAsString().equals("constant") && !colors.isEmpty() && colors.entrySet().stream().allMatch(c -> c.getValue().getAsInt() == 0xFF80FF00)
                && goo.get("overlay_texture").getAsString().equals("minecraft:textures/block/slime_block.png") && goo.get("fog_code").getAsBoolean()
                && !baked().getAsJsonObject("fluids").has("minecraft:water");
        });
        Checks.check("goo_flowing_model", () -> {
            JsonObject colors = baked().getAsJsonObject("fluids").getAsJsonObject("horizon_java_test:flowing_goo").getAsJsonObject("tint").getAsJsonObject("states");
            if (colors.size() != 16) return false;
            for (var color : colors.entrySet()) if (color.getValue().getAsInt() != 0xFF80FF00) return false;
            return true;
        });
        Checks.check("goo_code", () -> {
            JsonObject code = facts().getAsJsonObject("fluids").getAsJsonObject("horizon_java_test:flowing_goo").getAsJsonObject("code");
            return code.get("getTickDelay").getAsString().equals("neoforge") && code.get("tick").getAsString().equals("vanilla");
        });
        Checks.check("goo_type", () -> {
            JsonObject type = facts().getAsJsonObject("fluids").getAsJsonObject("horizon_java_test:goo").getAsJsonObject("fluid_type");
            return type.get("motionScale").getAsDouble() == 0.002 && !type.get("canConvertToSource").getAsBoolean() && type.get("lightLevel").getAsInt() == 3
                && type.getAsJsonObject("sounds").get("bucket_fill").getAsString().equals("minecraft:item.bucket.fill_lava");
        });
        Checks.check("water_rows", () -> {
            JsonObject states = facts().getAsJsonObject("blocks").getAsJsonObject("minecraft:water").getAsJsonObject("states");
            JsonObject source = states.getAsJsonObject("level=0").getAsJsonObject("fluid");
            JsonObject flowing = states.getAsJsonObject("level=8").getAsJsonObject("fluid");
            return source.get("id").getAsString().equals("minecraft:water") && source.get("amount").getAsInt() == 8 && !source.get("falling").getAsBoolean() && source.get("source").getAsBoolean()
                && flowing.get("id").getAsString().equals("minecraft:flowing_water") && flowing.get("amount").getAsInt() == 8 && flowing.get("falling").getAsBoolean() && !flowing.get("source").getAsBoolean();
        });
    }
}
