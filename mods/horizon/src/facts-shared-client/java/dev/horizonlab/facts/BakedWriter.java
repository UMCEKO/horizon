package dev.horizonlab.facts;

import com.google.gson.stream.JsonWriter;
import java.io.IOException;
import java.io.Writer;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Function;
import java.util.function.Predicate;
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

/** Every state of each block with a code-built model, baked once into the quads `carry` writes as `horizon:baked` models; shared by the client oracle and the game's Java boot. */
public final class BakedWriter {
    private BakedWriter() {}

    private static <T extends Comparable<T>> String value(BlockState state, Property<T> property) {
        return property.getName(state.getValue(property));
    }

    static String key(BlockState state) {
        var parts = new ArrayList<String>();
        for (Property<?> property : state.getBlock().getStateDefinition().getProperties()) parts.add(property.getName() + "=" + value(state, property));
        return String.join(",", parts);
    }

    /** Gson's own writer for a tree is lenient and escapes HTML; the file stays byte for byte what the tree gave. */
    static JsonWriter writer(Writer file) {
        var w = new JsonWriter(file);
        w.setLenient(true);
        w.setHtmlSafe(true);
        return w;
    }

    /** One quad; `vertices` is 4 x (x, y, z, u, v), and a null `cull` is left out. */
    static void writeQuad(JsonWriter w, float[] vertices, String sprite, String face, String cull, int tint, boolean shade, int emission, String layer) throws IOException {
        w.beginObject();
        w.name("v").beginArray();
        for (int i = 0; i < 4; i++) {
            w.beginArray();
            for (int j = 0; j < 5; j++) w.value(vertices[i * 5 + j]);
            w.endArray();
        }
        w.endArray();
        w.name("sprite").value(sprite);
        w.name("face").value(face);
        if (cull != null) w.name("cull").value(cull);
        w.name("tint").value(tint);
        w.name("shade").value(shade);
        w.name("emission").value(emission);
        w.name("layer").value(layer);
        w.endObject();
    }

    private static void quad(JsonWriter w, BakedQuad quad, Direction cull) throws IOException {
        var sprite = quad.materialInfo().sprite();
        float u0 = sprite.getU0(), u1 = sprite.getU1(), v0 = sprite.getV0(), v1 = sprite.getV1();
        var vertices = new float[20];
        for (int i = 0; i < 4; i++) {
            var p = quad.position(i);
            long uv = quad.packedUV(i);
            vertices[i * 5] = p.x();
            vertices[i * 5 + 1] = p.y();
            vertices[i * 5 + 2] = p.z();
            vertices[i * 5 + 3] = (UVPair.unpackU(uv) - u0) / (u1 - u0);
            vertices[i * 5 + 4] = (UVPair.unpackV(uv) - v0) / (v1 - v0);
        }
        writeQuad(w, vertices, sprite.contents().name().toString(), quad.direction().getSerializedName(), cull == null ? null : cull.getSerializedName(),
                quad.materialInfo().tintIndex(), quad.materialInfo().shadeDirectionOverride() == null, quad.materialInfo().lightEmission(),
                quad.materialInfo().layer().name().toLowerCase());
    }

    /** Writes `out` through a temporary file, one block state at a time; returns how many blocks, states and quads. */
    public static int[] write(Path out, Function<BlockState, BlockStateModel> models, Predicate<BlockState> codeBuilt) throws IOException {
        Path partial = out.resolveSibling(out.getFileName() + ".partial");
        int[] counts;
        try (var file = Files.newBufferedWriter(partial)) {
            counts = write(file, models, codeBuilt);
        }
        Files.move(partial, out, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
        return counts;
    }

    public static int[] write(Writer file, Function<BlockState, BlockStateModel> models, Predicate<BlockState> codeBuilt) throws IOException {
        var random = RandomSource.create(0);
        var w = writer(file);
        int blocks = 0, states = 0, quads = 0;
        w.beginObject();
        w.name("version").value(1);
        w.name("blocks").beginObject();
        for (Block block : BuiltInRegistries.BLOCK) {
            var possible = block.getStateDefinition().getPossibleStates();
            if (possible.stream().noneMatch(codeBuilt)) continue;
            w.name(BuiltInRegistries.BLOCK.getKey(block).toString()).beginObject();
            for (BlockState state : possible) {
                BlockStateModel model = models.apply(state);
                List<BlockStateModelPart> parts = new ArrayList<>();
                random.setSeed(42);
                model.collectParts(random, parts);
                w.name(key(state)).beginObject();
                w.name("particle").value(model.particleMaterial().sprite().contents().name().toString());
                w.name("quads").beginArray();
                for (var part : parts) {
                    for (Direction d : Direction.values()) for (var q : part.getQuads(d)) { quad(w, q, d); quads++; }
                    for (var q : part.getQuads(null)) { quad(w, q, null); quads++; }
                }
                w.endArray();
                w.endObject();
                states++;
            }
            w.endObject();
            blocks++;
        }
        w.endObject();
        w.endObject();
        w.flush();
        return new int[] {blocks, states, quads};
    }
}
