package dev.horizonlab.javamods.boot;

import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.block.dispatch.BlockStateModel;
import net.minecraft.client.resources.model.BlockStateModelLoader;
import net.minecraft.client.resources.model.ClientItemInfoLoader;
import net.minecraft.resources.Identifier;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;

/** The block models mods build in code, baked by the client's resource reload; Rust draws the geometry they give for a block entity's data. */
public final class ClientModels {
    private ClientModels() {}

    private static final Set<String> VANILLA = Set.of("SingleVariant", "WeightedVariants", "MultiPartModel");
    private static volatile Set<BlockState> codeBuilt = Set.of();
    private static volatile Set<Identifier> foreign = Set.of();

    /** What the reload's model discovery starts from, as it starts: the block states whose model is a mod's own type, and the items only their mod's code draws. */
    public static void discovered(BlockStateModelLoader.LoadedModels states, ClientItemInfoLoader.LoadedClientInfos items) {
        var built = new HashSet<BlockState>();
        states.models().forEach((state, root) -> {
            if (codeBuilt(root)) built.add(state);
        });
        var drawn = new HashSet<Identifier>();
        items.contents().forEach((id, item) -> {
            if (!vanillaModel(item.model())) drawn.add(id);
        });
        codeBuilt = Set.copyOf(built);
        foreign = Set.copyOf(drawn);
    }

    /** The code-built block states' models, as the reload baked them. */
    static Map<BlockState, BlockStateModel> dynamicModels() {
        var set = Minecraft.getInstance().getModelManager().getBlockStateModelSet();
        var out = new HashMap<BlockState, BlockStateModel>();
        for (var state : codeBuilt) out.put(state, set.get(state));
        return out;
    }

    static Set<BlockState> codeBuiltStates() {
        return codeBuilt;
    }

    static Set<Identifier> foreignItems() {
        return foreign;
    }

    /** A root whose contents aren't one of vanilla's model types (a mod's own type, registered by its code). */
    static boolean codeBuilt(BlockStateModel.UnbakedRoot root) {
        Object contents = root;
        if (root instanceof BlockStateModel.SimpleCachedUnbakedRoot) {
            try {
                var field = BlockStateModel.SimpleCachedUnbakedRoot.class.getDeclaredField("contents");
                field.setAccessible(true);
                contents = field.get(root);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }
        var type = contents.getClass();
        String outer = type.getEnclosingClass() != null ? type.getEnclosingClass().getSimpleName() : type.getSimpleName();
        return !type.getName().startsWith("net.minecraft.") || !VANILLA.contains(outer);
    }

    private static boolean vanillaId(Class<?> holder, Object value) {
        return BootFacts.vanillaMapped(holder, value);
    }

    /** Whether vanilla's mappers name every type, property and tint in the decoded tree: Horizon's own item models read only those. */
    private static boolean vanillaModel(net.minecraft.client.renderer.item.ItemModel.Unbaked model) {
        if (!vanillaId(net.minecraft.client.renderer.item.ItemModels.class, model.type())) return false;
        return switch (model) {
            case net.minecraft.client.renderer.item.CuboidItemModelWrapper.Unbaked m ->
                m.tints().stream().allMatch(tint -> vanillaId(net.minecraft.client.color.item.ItemTintSources.class, tint.type()));
            case net.minecraft.client.renderer.item.SpecialModelWrapper.Unbaked m ->
                vanillaId(net.minecraft.client.renderer.special.SpecialModelRenderers.class, m.specialModel().type());
            case net.minecraft.client.renderer.item.CompositeModel.Unbaked m -> m.models().stream().allMatch(ClientModels::vanillaModel);
            case net.minecraft.client.renderer.item.ConditionalItemModel.Unbaked m ->
                vanillaId(net.minecraft.client.renderer.item.properties.conditional.ConditionalItemModelProperties.class, m.property().type())
                        && vanillaModel(m.onTrue()) && vanillaModel(m.onFalse());
            case net.minecraft.client.renderer.item.SelectItemModel.Unbaked m ->
                vanillaId(net.minecraft.client.renderer.item.properties.select.SelectItemModelProperties.class, m.unbakedSwitch().property().type())
                        && m.unbakedSwitch().cases().stream().allMatch(c -> vanillaModel(c.model()))
                        && m.fallback().map(ClientModels::vanillaModel).orElse(true);
            case net.minecraft.client.renderer.item.RangeSelectItemModel.Unbaked m ->
                vanillaId(net.minecraft.client.renderer.item.properties.numeric.RangeSelectItemModelProperties.class, m.property().type())
                        && m.entries().stream().allMatch(e -> vanillaModel(e.model()))
                        && m.fallback().map(ClientModels::vanillaModel).orElse(true);
            default -> true;
        };
    }

    private static final Map<Object, Integer> KEYS = new HashMap<>();
    private static final Map<java.nio.ByteBuffer, Integer> CONTENT = new HashMap<>();
    private static final it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet DIRTY = new it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet();
    private static int nextKey;

    static boolean dynamic(BlockState state) {
        return codeBuilt.contains(state);
    }

    /** The block at `pos` changed in the window's level: its geometry is baked again on the next take. */
    static void dirty(BlockPos pos) {
        if (DIRTY.add(pos.asLong())) Clients.changes.incrementAndGet();
    }

    static int pending() {
        return DIRTY.size();
    }

    static void forget() {
        DIRTY.clear();
        KEYS.clear();
        CONTENT.clear();
        nextKey = 0;
    }

    /** Each changed position's geometry, from its block entity's model data; an equal geometry key bakes once and is sent once. */
    static byte[] take(net.minecraft.client.multiplayer.ClientLevel level) throws Exception {
        var models = Minecraft.getInstance().getModelManager().getBlockStateModelSet();
        var bytes = new java.io.ByteArrayOutputStream();
        var out = new java.io.DataOutputStream(bytes);
        var positions = new long[DIRTY.size()];
        int n = 0;
        for (long at : DIRTY) positions[n++] = at;
        DIRTY.clear();
        out.writeInt(n);
        var random = net.minecraft.util.RandomSource.create();
        for (long at : positions) {
            var pos = BlockPos.of(at);
            out.writeLong(at);
            var entity = ClientBridge.blockEntity(level, pos);
            BlockState state = entity == null ? null : entity.getBlockState();
            out.writeInt(state == null ? -1 : net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY.getId(state));
            BlockStateModel model = state == null || !codeBuilt.contains(state) ? null : models.get(state);
            if (model == null) {
                out.writeInt(-1);
                out.writeBoolean(false);
                continue;
            }
            level.getModelDataManager().requestRefresh(entity);
            long seed = state.getSeed(pos);
            random.setSeed(seed);
            var tints = tints(level, state, pos);
            Object made = model.createGeometryKey(level, pos, state, random);
            // Vanilla tints after the geometry, so a mod's key needn't tell two tints apart.
            Object geometry = made == null ? null : java.util.List.of(made, java.util.Arrays.asList(tints));
            Integer key = geometry == null ? null : KEYS.get(geometry);
            if (key != null) {
                out.writeInt(key);
                out.writeBoolean(false);
                continue;
            }
            var parts = new java.util.ArrayList<net.minecraft.client.renderer.block.dispatch.BlockStateModelPart>();
            random.setSeed(seed);
            model.collectParts(level, pos, state, random, parts);
            var quads = new java.util.ArrayList<Object[]>();
            for (var part : parts) {
                for (var d : net.minecraft.core.Direction.values()) for (var q : part.getQuads(d)) quads.add(new Object[] {q, d});
                for (var q : part.getQuads(null)) quads.add(new Object[] {q, null});
            }
            var encoded = new java.io.ByteArrayOutputStream();
            var data = new java.io.DataOutputStream(encoded);
            data.writeInt(quads.size());
            for (Object[] pair : quads) quad(data, (net.minecraft.client.resources.model.geometry.BakedQuad) pair[0], (net.minecraft.core.Direction) pair[1], tints);
            // Mods without a geometry key still repeat shapes: equal quads share one key, as a block state's model is shared.
            var content = java.nio.ByteBuffer.wrap(encoded.toByteArray());
            key = CONTENT.get(content);
            boolean fresh = key == null;
            if (fresh) {
                key = nextKey++;
                CONTENT.put(content, key);
            }
            if (geometry != null) KEYS.put(geometry, key);
            out.writeInt(key);
            out.writeBoolean(fresh);
            if (fresh) out.write(content.array());
        }
        return bytes.toByteArray();
    }

    /** `ModelBlockRenderer.computeTintColor`: each tint index's color, from the block's tint sources. */
    private static Integer[] tints(net.minecraft.client.multiplayer.ClientLevel level, BlockState state, BlockPos pos) {
        var sources = Minecraft.getInstance().getBlockColors().getTintSources(state);
        var colors = new Integer[sources.size()];
        for (int i = 0; i < colors.length; i++) colors[i] = sources.get(i).colorInWorld(state, level, pos);
        return colors;
    }

    private static void quad(java.io.DataOutputStream out, net.minecraft.client.resources.model.geometry.BakedQuad quad, net.minecraft.core.Direction cull, Integer[] tints) throws java.io.IOException {
        var info = quad.materialInfo();
        var sprite = info.sprite();
        float u0 = sprite.getU0(), u1 = sprite.getU1(), v0 = sprite.getV0(), v1 = sprite.getV1();
        for (int i = 0; i < 4; i++) {
            var p = quad.position(i);
            long uv = quad.packedUV(i);
            out.writeFloat(p.x());
            out.writeFloat(p.y());
            out.writeFloat(p.z());
            out.writeFloat((net.minecraft.client.model.geom.builders.UVPair.unpackU(uv) - u0) / (u1 - u0));
            out.writeFloat((net.minecraft.client.model.geom.builders.UVPair.unpackV(uv) - v0) / (v1 - v0));
        }
        byte[] id = sprite.contents().name().toString().getBytes(java.nio.charset.StandardCharsets.UTF_8);
        out.writeInt(id.length);
        out.write(id);
        out.writeByte(quad.direction().get3DDataValue());
        out.writeByte(cull == null ? 6 : cull.get3DDataValue());
        out.writeInt(info.tintIndex());
        out.writeInt(net.minecraft.util.ARGB.multiply(info.tintIndex() >= 0 && info.tintIndex() < tints.length ? tints[info.tintIndex()] : -1, quad.bakedColors().color(0)));
        out.writeBoolean(info.shadeDirectionOverride() == null);
        out.writeByte(info.lightEmission());
        out.writeByte(switch (info.layer()) {
            case SOLID -> 0;
            case CUTOUT -> 1;
            case TRANSLUCENT -> 3;
        });
    }
}
