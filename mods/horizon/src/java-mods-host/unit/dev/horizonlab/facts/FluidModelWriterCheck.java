package dev.horizonlab.facts;

import com.google.gson.JsonParser;
import java.io.StringWriter;
import java.util.Map;
import java.util.TreeMap;
import net.minecraft.client.color.block.BlockTintSource;
import net.minecraft.client.color.block.BlockTintSources;
import net.minecraft.client.renderer.BiomeColors;
import net.minecraft.client.renderer.block.BlockAndTintGetter;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.client.fluid.FluidTintSource;
import net.neoforged.neoforge.client.fluid.FluidTintSources;

/** A tint that never reads the world is written as the colour of each state; one from the biome as water; one that asks the world as code. */
public final class FluidModelWriterCheck {
    private static final FluidModelWriter.Probe PROBE = (source, getter) ->
        source instanceof FluidTintSource f ? f.colorInWorld(null, null, getter, BlockPos.ZERO) : source.colorInWorld(null, getter, BlockPos.ZERO);

    private static String tint(BlockTintSource source) throws Exception {
        var text = new StringWriter();
        var w = BakedWriter.writer(text);
        Map<String, FluidModelWriter.Probe> states = new TreeMap<>();
        states.put("amount=8,falling=false", PROBE);
        states.put("amount=7,falling=true", PROBE);
        w.beginObject();
        FluidModelWriter.tint(w, "mod:fluid", source, states);
        w.endObject();
        w.flush();
        return JsonParser.parseString(text.toString()).getAsJsonObject().get("tint").toString();
    }

    private static void expect(String what, String got, String want) {
        if (!got.equals(want)) throw new AssertionError(what + ": got " + got + ", wanted " + want);
    }

    public static void main(String[] args) throws Exception {
        long argb = 0xFF112233L;
        String constant = "{\"kind\":\"constant\",\"states\":{\"amount=7,falling=true\":" + argb + ",\"amount=8,falling=false\":" + argb + "}}";
        expect("fluid constant", tint(FluidTintSources.constant(0xFF112233)), constant);
        expect("block constant", tint(BlockTintSources.constant(0xFF112233)), constant);
        expect("block water", tint(BlockTintSources.water()), "{\"kind\":\"water\"}");
        expect("fluid water", tint(FluidTintSources.water()), "{\"kind\":\"water\"}");
        expect("wrapped water", tint(FluidTintSources.of(BlockTintSources.water())), "{\"kind\":\"water\"}");
        expect("none", tint(null), "{\"kind\":\"none\"}");
        BlockTintSource world = new BlockTintSource() {
            @Override
            public int color(BlockState state) {
                return -1;
            }

            @Override
            public int colorInWorld(BlockState state, BlockAndTintGetter level, BlockPos pos) {
                return level.getBlockTint(pos, BiomeColors.WATER_COLOR_RESOLVER);
            }
        };
        expect("world tint", tint(world), "{\"kind\":\"code\"}");
        System.out.println("FluidModelWriterCheck ok");
    }
}
