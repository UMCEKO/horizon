package dev.horizonlab.javamods.boot;

import java.lang.reflect.Method;
import java.util.AbstractMap;
import java.util.HashMap;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.Executor;
import net.minecraft.client.Minecraft;
import net.minecraft.client.model.geom.EntityModelSet;
import net.minecraft.client.renderer.block.dispatch.BlockStateModel;
import net.minecraft.client.renderer.texture.SpriteLoader;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.client.resources.model.BlockStateModelLoader;
import net.minecraft.client.resources.model.ClientItemInfoLoader;
import net.minecraft.client.resources.model.ModelBakery;
import net.minecraft.client.resources.model.ModelManager;
import net.minecraft.client.resources.model.sprite.MaterialBaker;
import net.minecraft.data.AtlasIds;
import net.minecraft.resources.Identifier;
import net.minecraft.util.Util;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.core.registries.BuiltInRegistries;

/** The block models mods build in code, baked in this JVM against Horizon's sprite views; Rust draws what they give. */
public final class ClientModels {
    private ClientModels() {}

    private static final Set<String> VANILLA = Set.of("SingleVariant", "WeightedVariants", "MultiPartModel");
    private static Map<BlockState, BlockStateModel> baked;

    /** An atlas's stitch results as the baker reads them: a sprite for each file that exists. */
    private static SpriteLoader.Preparations atlas(Identifier atlas, Identifier sheet) {
        Map<Identifier, TextureAtlasSprite> regions = new AbstractMap<>() {
            @Override
            public TextureAtlasSprite get(Object key) {
                return key instanceof Identifier id ? Sprites.spriteIfPresent(atlas, id, sheet) : null;
            }

            @Override
            public Set<Entry<Identifier, TextureAtlasSprite>> entrySet() {
                return Set.of();
            }
        };
        return new SpriteLoader.Preparations(1, 1, 0, Sprites.sprite(atlas, net.minecraft.client.renderer.texture.MissingTextureAtlasSprite.getLocation(), sheet), regions, CompletableFuture.completedFuture(null));
    }

    /** The client's rendering pieces item and block entity renderers reach through `Minecraft`: a block model resolver and a game renderer's state. */
    private static void clientRendering() throws ReflectiveOperationException {
        var minecraft = Minecraft.getInstance();
        var resolver = Minecraft.class.getDeclaredField("blockModelResolver");
        resolver.setAccessible(true);
        if (resolver.get(minecraft) == null) resolver.set(minecraft, new net.minecraft.client.renderer.block.BlockModelResolver(minecraft.getModelManager()));
        var renderer = Minecraft.class.getDeclaredField("gameRenderer");
        renderer.setAccessible(true);
        if (renderer.get(minecraft) == null) {
            var view = GameBridge.construct(net.minecraft.client.renderer.GameRenderer.class, Object.class.getDeclaredConstructor());
            var state = net.minecraft.client.renderer.GameRenderer.class.getDeclaredField("gameRenderState");
            state.setAccessible(true);
            state.set(view, new net.minecraft.client.renderer.state.GameRenderState());
            renderer.set(minecraft, view);
        }
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

    private static final Map<Object, Integer> KEYS = new HashMap<>();
    private static final Map<Identifier, net.minecraft.client.renderer.item.ItemModel> ITEM_MODELS = new java.util.concurrent.ConcurrentHashMap<>();
    private static final Map<Identifier, net.minecraft.client.renderer.item.ClientItem.Properties> ITEM_PROPERTIES = new java.util.concurrent.ConcurrentHashMap<>();

    /** A mod item's model as baked here, for the items whose definitions use a type only the mod's code knows. */
    public static net.minecraft.client.renderer.item.ItemModel itemModel(Identifier id) {
        return ITEM_MODELS.get(id);
    }

    public static net.minecraft.client.renderer.item.ClientItem.Properties itemProperties(Identifier id) {
        return ITEM_PROPERTIES.getOrDefault(id, net.minecraft.client.renderer.item.ClientItem.Properties.DEFAULT);
    }

    /** The item ids whose models are baked here, as text. */
    static byte[] foreignItemIds() throws Exception {
        baked();
        var bytes = new java.io.ByteArrayOutputStream();
        var out = new java.io.DataOutputStream(bytes);
        out.writeInt(ITEM_MODELS.size());
        for (var id : ITEM_MODELS.keySet()) {
            byte[] text = id.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8);
            out.writeInt(text.length);
            out.write(text);
        }
        return bytes.toByteArray();
    }

    /** Items whose definition names a type outside vanilla's, anywhere in its tree: only the mod's code can draw them. */
    private static Map<Identifier, net.minecraft.client.renderer.item.ClientItem> foreignItems(net.minecraft.server.packs.resources.ResourceManager manager, Map<Identifier, net.minecraft.client.renderer.item.ClientItem> all) {
        var pattern = java.util.regex.Pattern.compile("\"type\"\\s*:\\s*\"([a-z0-9_.-]+):");
        var out = new HashMap<Identifier, net.minecraft.client.renderer.item.ClientItem>();
        all.forEach((id, item) -> {
            if (id.getNamespace().equals("minecraft")) return;
            var resource = manager.getResource(id.withPath(path -> "items/" + path + ".json"));
            if (resource.isEmpty()) return;
            try (var reader = resource.get().openAsReader()) {
                var matcher = pattern.matcher(reader.lines().collect(java.util.stream.Collectors.joining()));
                while (matcher.find()) {
                    if (!matcher.group(1).equals("minecraft")) {
                        out.put(id, item);
                        break;
                    }
                }
            } catch (java.io.IOException e) {
                // An unreadable definition keeps Horizon's own reading.
            }
        });
        return out;
    }
    private static final Map<java.nio.ByteBuffer, Integer> CONTENT = new HashMap<>();
    private static final it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet DIRTY = new it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet();
    private static int nextKey;

    static boolean dynamic(BlockState state) {
        try {
            return baked().containsKey(state);
        } catch (Exception e) {
            throw new IllegalStateException(e);
        }
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

    /** Java state ids whose geometry may come from a block entity's data. */
    static byte[] states() throws Exception {
        var bytes = new java.io.ByteArrayOutputStream();
        var out = new java.io.DataOutputStream(bytes);
        var states = baked().keySet();
        out.writeInt(states.size());
        for (BlockState state : states) out.writeInt(net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY.getId(state));
        return bytes.toByteArray();
    }

    /** Each changed position's geometry, from its block entity's model data; an equal geometry key bakes once and is sent once. */
    static byte[] take(net.minecraft.client.multiplayer.ClientLevel level) throws Exception {
        var models = baked();
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
            BlockStateModel model = state == null ? null : models.get(state);
            if (model == null) {
                out.writeInt(-1);
                out.writeBoolean(false);
                continue;
            }
            level.getModelDataManager().requestRefresh(entity);
            long seed = state.getSeed(pos);
            random.setSeed(seed);
            Object geometry = model.createGeometryKey(level, pos, state, random);
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
            for (Object[] pair : quads) quad(data, (net.minecraft.client.resources.model.geometry.BakedQuad) pair[0], (net.minecraft.core.Direction) pair[1]);
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

    private static void quad(java.io.DataOutputStream out, net.minecraft.client.resources.model.geometry.BakedQuad quad, net.minecraft.core.Direction cull) throws java.io.IOException {
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
        out.writeBoolean(info.shadeDirectionOverride() == null);
        out.writeByte(info.lightEmission());
        out.writeByte(switch (info.layer()) {
            case SOLID -> 0;
            case CUTOUT -> 1;
            case TRANSLUCENT -> 3;
        });
    }

    /** Bakes once, on first use, only the block states whose unbaked model isn't one of vanilla's own types. */
    @SuppressWarnings("unchecked")
    static synchronized Map<BlockState, BlockStateModel> baked() throws Exception {
        if (baked != null) return baked;
        // What the client's start does for mods' model loaders and block model types, which the window-less view skips.
        net.neoforged.neoforge.client.model.UnbakedModelParser.init();
        net.neoforged.neoforge.client.model.block.BlockStateModelHooks.init();
        var manager = Minecraft.getInstance().getResourceManager();
        Executor executor = Util.backgroundExecutor();
        Method loadModels = ModelManager.class.getDeclaredMethod("loadBlockModels", net.minecraft.server.packs.resources.ResourceManager.class, Executor.class);
        loadModels.setAccessible(true);
        var models = ((CompletableFuture<Map<Identifier, net.minecraft.client.resources.model.UnbakedModel>>) loadModels.invoke(null, manager, executor)).join();
        var states = BlockStateModelLoader.loadBlockStates(manager, executor).join();
        var mine = new HashMap<BlockState, BlockStateModel.UnbakedRoot>();
        var codeBuilt = new java.util.HashSet<BlockState>();
        // Every mod block's model, as mods' item and block entity renderers draw them; only the code-built ones also come from block entity data.
        states.models().forEach((state, root) -> {
            boolean built = codeBuilt(root);
            if (built) codeBuilt.add(state);
            if (built || !BuiltInRegistries.BLOCK.getKey(state.getBlock()).getNamespace().equals("minecraft")) mine.put(state, root);
        });
        // Models mods register to fetch by key at draw time (a cable's arms), as the client's reload loads them.
        var standalone = net.neoforged.neoforge.client.model.standalone.StandaloneModelLoader.load(executor).join();
        Method discover = null;
        for (Method m : ModelManager.class.getDeclaredMethods()) if (m.getName().equals("discoverModelDependencies") && m.getParameterCount() == 4) discover = m;
        discover.setAccessible(true);
        var clientItems = foreignItems(manager, ClientItemInfoLoader.scheduleLoad(manager, executor).join().contents());
        Object resolved = discover.invoke(null, models, new BlockStateModelLoader.LoadedModels(mine), new ClientItemInfoLoader.LoadedClientInfos(clientItems), standalone);
        var missing = resolved.getClass().getDeclaredMethod("missing");
        var all = resolved.getClass().getDeclaredMethod("models");
        missing.setAccessible(true);
        all.setAccessible(true);
        var bakery = new ModelBakery(EntityModelSet.vanilla(), Minecraft.getInstance().getAtlasManager(), null, mine, clientItems,
                (Map<Identifier, net.minecraft.client.resources.model.ResolvedModel>) all.invoke(resolved),
                (net.minecraft.client.resources.model.ResolvedModel) missing.invoke(resolved), standalone,
                net.neoforged.neoforge.client.entity.animation.json.AnimationLoader.PendingAnimations.EMPTY);
        var blocks = atlas(AtlasIds.BLOCKS, net.minecraft.client.renderer.texture.TextureAtlas.LOCATION_BLOCKS);
        var items = atlas(AtlasIds.ITEMS, net.minecraft.client.renderer.texture.TextureAtlas.LOCATION_ITEMS);
        var materials = new MaterialBaker(blocks, items);
        var result = bakery.bakeModels(materials, executor).join();
        materials.logMissingTextures();
        // The client's resource reload posts these after stitching and baking; mods take their sprites and models from them.
        net.neoforged.fml.ModLoader.postEvent(new net.neoforged.neoforge.client.event.TextureAtlasStitchedEvent(Sprites.atlas(AtlasIds.BLOCKS)));
        net.neoforged.fml.ModLoader.postEvent(new net.neoforged.neoforge.client.event.TextureAtlasStitchedEvent(Sprites.atlas(AtlasIds.ITEMS)));
        net.neoforged.fml.ModLoader.postEvent(new net.neoforged.neoforge.client.event.ModelEvent.ModifyBakingResult(result, materials, blocks, items, bakery));
        var standaloneField = ModelManager.class.getDeclaredField("bakedStandaloneModels");
        standaloneField.setAccessible(true);
        standaloneField.set(Minecraft.getInstance().getModelManager(), result.standaloneModels());
        var fluids = ModelManager.class.getDeclaredField("fluidStateModelSet");
        fluids.setAccessible(true);
        fluids.set(Minecraft.getInstance().getModelManager(), new net.minecraft.client.renderer.block.FluidStateModelSet(
                net.minecraft.client.renderer.block.FluidStateModelSet.bake(materials), result.missingModels().fluid()));
        net.neoforged.fml.ModLoader.postEvent(new net.neoforged.neoforge.client.event.ModelEvent.BakingCompleted(Minecraft.getInstance().getModelManager(), result, bakery));
        ITEM_MODELS.putAll(result.itemStackModels());
        ITEM_PROPERTIES.putAll(result.itemProperties());
        var stateModels = ModelManager.class.getDeclaredField("blockStateModelSet");
        stateModels.setAccessible(true);
        var stateModelSet = new net.minecraft.client.renderer.block.BlockStateModelSet(result.blockStateModels(), result.missingModels().block());
        stateModels.set(Minecraft.getInstance().getModelManager(), stateModelSet);
        var blockModels = ModelManager.class.getDeclaredField("blockModelSet");
        blockModels.setAccessible(true);
        blockModels.set(Minecraft.getInstance().getModelManager(), new net.minecraft.client.renderer.block.BlockModelSet(stateModelSet, Map.of(), net.minecraft.client.color.block.BlockColors.createDefault()));
        clientRendering();
        baked = new HashMap<>();
        result.blockStateModels().forEach((state, model) -> {
            if (codeBuilt.contains(state)) baked.put(state, model);
        });
        System.err.println("Horizon: baked " + baked.size() + " block states whose models mods build in code");
        return baked;
    }
}
