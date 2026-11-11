package dev.horizonlab.facts;

import com.google.gson.stream.JsonWriter;
import java.io.IOException;
import java.lang.reflect.Method;
import java.lang.reflect.Proxy;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.function.Predicate;
import net.minecraft.client.Minecraft;
import net.minecraft.client.color.block.BlockTintSource;
import net.minecraft.client.color.block.BlockTintSources;
import net.minecraft.client.renderer.block.BlockAndTintGetter;
import net.minecraft.client.renderer.block.FluidModel;
import net.minecraft.client.renderer.block.FluidStateModelSet;
import net.minecraft.client.resources.model.sprite.Material;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.client.extensions.common.IClientFluidTypeExtensions;
import net.neoforged.neoforge.client.fluid.FluidTintSource;
import net.neoforged.neoforge.client.fluid.FluidTintSources;

/** What the client baked for each mod fluid (sprites, layer, tint) and what its client extension says, written as `fluids` and `fluid_overlay` of baked-models.json. */
public final class FluidModelWriter {
    private FluidModelWriter() {}

    /** Thrown by the getter a tint source is probed with when it reads the world. */
    private static final class Touched extends Error {
        Touched() {
            super(null, null, false, false);
        }
    }

    private static final Class<?> BLOCK_WATER = BlockTintSources.water().getClass();
    private static final Class<?> FLUID_WATER = FluidTintSources.water().getClass();

    private static Object zero(Class<?> type) {
        if (type == boolean.class) return false;
        if (type == float.class) return 0F;
        if (type == double.class) return 0D;
        if (type == long.class) return 0L;
        if (type == int.class) return 0;
        if (type == short.class) return (short) 0;
        if (type == byte.class) return (byte) 0;
        if (type == char.class) return (char) 0;
        return null;
    }

    /** A getter that throws `Touched` from every method, or answers air everywhere. */
    static BlockAndTintGetter getter(boolean throwing) {
        return (BlockAndTintGetter) Proxy.newProxyInstance(FluidModelWriter.class.getClassLoader(), new Class<?>[] {BlockAndTintGetter.class}, (proxy, method, args) -> {
            if (throwing) throw new Touched();
            if (method.getDeclaringClass() == Object.class) return method.getName().equals("hashCode") ? 0 : method.getName().equals("equals") ? proxy == args[0] : "air";
            if (method.getReturnType() == BlockState.class) return Blocks.AIR.defaultBlockState();
            if (method.getReturnType() == FluidState.class) return Fluids.EMPTY.defaultFluidState();
            if (method.getReturnType() == float.class && method.getName().equals("getShade")) return 1F;
            return zero(method.getReturnType());
        });
    }

    private static boolean wrapsWater(BlockTintSource source) {
        if (!source.getClass().getName().startsWith(FluidTintSources.class.getName() + "$")) return false;
        try {
            var field = source.getClass().getDeclaredField("val$tintSource");
            field.setAccessible(true);
            var inner = field.get(source);
            return inner != null && inner.getClass() == BLOCK_WATER;
        } catch (NoSuchFieldException e) {
            return false;
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
    }

    private static void material(JsonWriter w, String name, Material.Baked material) throws IOException {
        w.name(name).value(material == null ? null : material.sprite().contents().name().toString());
    }

    /** What one fluid state's tint gives for the getter it is probed with. */
    interface Probe {
        int color(BlockTintSource source, BlockAndTintGetter getter);
    }

    static void tint(JsonWriter w, String id, BlockTintSource source, Map<String, Probe> states) throws IOException {
        w.name("tint").beginObject();
        if (source == null) {
            w.name("kind").value("none");
        } else if (source.getClass() == BLOCK_WATER || source.getClass() == FLUID_WATER || wrapsWater(source)) {
            w.name("kind").value("water");
        } else {
            var colors = new TreeMap<String, Integer>();
            var getter = getter(true);
            try {
                for (var state : states.entrySet()) colors.put(state.getKey(), state.getValue().color(source, getter));
                w.name("kind").value("constant");
                w.name("states").beginObject();
                for (var e : colors.entrySet()) w.name(e.getKey()).value(e.getValue() & 0xFFFFFFFFL);
                w.endObject();
            } catch (Touched e) {
                System.err.println("Horizon: fluid " + id + " tints from the world in code; Horizon draws it untinted (W7 D5)");
                w.name("kind").value("code");
            }
        }
        w.endObject();
    }

    private static Map<String, Probe> probes(Fluid fluid) {
        var probes = new TreeMap<String, Probe>();
        for (FluidState fs : fluid.getStateDefinition().getPossibleStates()) {
            var legacy = fs.createLegacyBlock();
            probes.put(FactsWriter.fluidStateKey(fs), (source, getter) -> source instanceof FluidTintSource f ? f.colorInWorld(fs, legacy, getter, BlockPos.ZERO) : source.colorInWorld(legacy, getter, BlockPos.ZERO));
        }
        return probes;
    }

    private static boolean overrides(Object extension, String name) {
        for (Method method : extension.getClass().getMethods()) {
            if (method.getName().equals(name) && method.getDeclaringClass() != IClientFluidTypeExtensions.class) return true;
        }
        return false;
    }

    /** Writes `"fluids": {...}` for each owned fluid that has a model of its own; returns how many. */
    public static int write(JsonWriter w, FluidStateModelSet models, Predicate<Fluid> owned) throws IOException {
        var missing = models.get(Fluids.EMPTY.defaultFluidState());
        int written = 0;
        w.name("fluids").beginObject();
        for (Fluid fluid : BuiltInRegistries.FLUID) {
            if (fluid == Fluids.EMPTY || !owned.test(fluid)) continue;
            var model = models.get(fluid.defaultFluidState());
            if (model == missing) continue;
            String id = BuiltInRegistries.FLUID.getKey(fluid).toString();
            var extension = IClientFluidTypeExtensions.of(fluid);
            var overlay = extension.getRenderOverlayTexture(Minecraft.getInstance());
            boolean overlayCode = overrides(extension, "extractOverlay");
            if (overlayCode) System.err.println("Horizon: fluid " + id + " draws its own screen overlay; Horizon draws the overlay texture (W7 out of scope)");
            w.name(id).beginObject();
            w.name("layer").value(model.layer().name());
            material(w, "still", model.stillMaterial());
            material(w, "flowing", model.flowingMaterial());
            material(w, "overlay", model.overlayMaterial());
            w.name("force_translucent").beginObject();
            w.name("still").value(model.stillMaterial().forceTranslucent());
            w.name("flowing").value(model.flowingMaterial().forceTranslucent());
            w.name("overlay").value(model.overlayMaterial() != null && model.overlayMaterial().forceTranslucent());
            w.endObject();
            tint(w, id, model.tintSource(), probes(fluid));
            w.name("overlay_texture").value(overlay == null ? null : overlay.toString());
            w.name("fog_code").value(overrides(extension, "modifyFogColor") || overrides(extension, "modifyFogRender"));
            w.name("overlay_code").value(overlayCode);
            w.endObject();
            written++;
        }
        w.endObject();
        return written;
    }

    private static boolean redeclares(Class<?> type) {
        for (Class<?> c = type; c != null && c != Object.class; c = c.getSuperclass()) {
            if (c.getName().startsWith("net.minecraft.") || c.getName().startsWith("net.neoforged.")) continue;
            for (Method method : c.getDeclaredMethods()) {
                if (method.getName().equals("shouldDisplayFluidOverlay") && method.getParameterCount() == 4) return true;
            }
        }
        return false;
    }

    /** Writes `"fluid_overlay": {block: [state keys]}` for the owned blocks whose own code wants the side overlay. */
    public static void writeOverlayStates(JsonWriter w, Predicate<Block> owned) throws IOException {
        var getter = getter(false);
        var water = Fluids.WATER.defaultFluidState();
        var found = new LinkedHashMap<String, List<String>>();
        for (Block block : BuiltInRegistries.BLOCK) {
            if (!owned.test(block) || !redeclares(block.getClass())) continue;
            var keys = new ArrayList<String>();
            for (BlockState state : block.getStateDefinition().getPossibleStates()) {
                if (state.shouldDisplayFluidOverlay(getter, BlockPos.ZERO, water)) keys.add(BakedWriter.key(state));
            }
            if (!keys.isEmpty()) found.put(BuiltInRegistries.BLOCK.getKey(block).toString(), keys);
        }
        w.name("fluid_overlay").beginObject();
        for (var e : found.entrySet()) {
            w.name(e.getKey()).beginArray();
            for (String key : e.getValue()) w.value(key);
            w.endArray();
        }
        w.endObject();
    }
}
