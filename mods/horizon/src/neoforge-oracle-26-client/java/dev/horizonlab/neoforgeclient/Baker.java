package dev.horizonlab.neoforgeclient;

import com.google.gson.Gson;
import com.google.gson.JsonArray;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import net.minecraft.client.Minecraft;
import net.minecraft.client.model.geom.builders.UVPair;
import net.minecraft.client.renderer.block.dispatch.BlockStateModel;
import net.minecraft.client.renderer.block.dispatch.BlockStateModelPart;
import net.minecraft.client.resources.model.geometry.BakedQuad;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;

/** Every block state whose model a mod's code builds, baked once into the quads `carry` writes as `horizon:baked` models. */
final class Baker {
    private Baker() {}

    private static final Set<String> VANILLA = Set.of("SingleVariant", "WeightedVariants", "MultiPartModel");

    static boolean codeBuilt(BlockStateModel model) {
        return !VANILLA.contains(model.getClass().getSimpleName());
    }

    private static <T extends Comparable<T>> String value(BlockState state, Property<T> property) {
        return property.getName(state.getValue(property));
    }

    static String key(BlockState state) {
        var parts = new ArrayList<String>();
        for (Property<?> property : state.getBlock().getStateDefinition().getProperties()) parts.add(property.getName() + "=" + value(state, property));
        return String.join(",", parts);
    }

    private static JsonObject quad(BakedQuad quad, Direction cull) {
        var sprite = quad.materialInfo().sprite();
        float u0 = sprite.getU0(), u1 = sprite.getU1(), v0 = sprite.getV0(), v1 = sprite.getV1();
        var o = new JsonObject();
        var vs = new JsonArray();
        for (int i = 0; i < 4; i++) {
            var p = quad.position(i);
            long uv = quad.packedUV(i);
            var v = new JsonArray();
            v.add(p.x());
            v.add(p.y());
            v.add(p.z());
            v.add((UVPair.unpackU(uv) - u0) / (u1 - u0));
            v.add((UVPair.unpackV(uv) - v0) / (v1 - v0));
            vs.add(v);
        }
        o.add("v", vs);
        o.addProperty("sprite", sprite.contents().name().toString());
        o.addProperty("face", quad.direction().getSerializedName());
        if (cull == null) o.add("cull", JsonNull.INSTANCE);
        else o.addProperty("cull", cull.getSerializedName());
        o.addProperty("tint", quad.materialInfo().tintIndex());
        o.addProperty("shade", quad.materialInfo().shadeDirectionOverride() == null);
        o.addProperty("emission", quad.materialInfo().lightEmission());
        o.addProperty("layer", quad.materialInfo().layer().name().toLowerCase());
        return o;
    }

    static void run(Path out) throws Exception {
        var models = Minecraft.getInstance().getModelManager().getBlockStateModelSet();
        var random = RandomSource.create(0);
        var blocks = new JsonObject();
        int states = 0, quads = 0;
        for (Block block : BuiltInRegistries.BLOCK) {
            var id = BuiltInRegistries.BLOCK.getKey(block);
            if (id.getNamespace().equals("minecraft")) continue;
            var possible = block.getStateDefinition().getPossibleStates();
            if (possible.stream().noneMatch(s -> codeBuilt(models.get(s)))) continue;
            var byState = new JsonObject();
            for (BlockState state : possible) {
                BlockStateModel model = models.get(state);
                List<BlockStateModelPart> parts = new ArrayList<>();
                random.setSeed(42);
                model.collectParts(random, parts);
                var list = new JsonArray();
                for (var part : parts) {
                    for (Direction d : Direction.values()) for (var q : part.getQuads(d)) list.add(quad(q, d));
                    for (var q : part.getQuads(null)) list.add(quad(q, null));
                }
                var entry = new JsonObject();
                entry.addProperty("particle", model.particleMaterial().sprite().contents().name().toString());
                entry.add("quads", list);
                byState.add(key(state), entry);
                states++;
                quads += list.size();
            }
            blocks.add(id.toString(), byState);
        }
        var root = new JsonObject();
        root.addProperty("version", 1);
        root.add("blocks", blocks);
        Files.writeString(out, new Gson().toJson(root));
        System.err.println("horizon oracle: baked " + blocks.size() + " blocks, " + states + " states, " + quads + " quads");
    }
}
