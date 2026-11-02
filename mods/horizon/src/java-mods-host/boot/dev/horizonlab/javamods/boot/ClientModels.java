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
import net.minecraft.world.level.block.state.BlockState;

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
        states.models().forEach((state, root) -> {
            if (codeBuilt(root)) mine.put(state, root);
        });
        Method discover = null;
        for (Method m : ModelManager.class.getDeclaredMethods()) if (m.getName().equals("discoverModelDependencies")) discover = m;
        discover.setAccessible(true);
        Object resolved = discover.invoke(null, models, new BlockStateModelLoader.LoadedModels(mine), new ClientItemInfoLoader.LoadedClientInfos(Map.of()));
        var missing = resolved.getClass().getDeclaredMethod("missing");
        var all = resolved.getClass().getDeclaredMethod("models");
        missing.setAccessible(true);
        all.setAccessible(true);
        var bakery = new ModelBakery(EntityModelSet.vanilla(), Minecraft.getInstance().getAtlasManager(), null, mine, Map.of(),
                (Map<Identifier, net.minecraft.client.resources.model.ResolvedModel>) all.invoke(resolved),
                (net.minecraft.client.resources.model.ResolvedModel) missing.invoke(resolved));
        var materials = new MaterialBaker(atlas(AtlasIds.BLOCKS, net.minecraft.client.renderer.texture.TextureAtlas.LOCATION_BLOCKS), atlas(AtlasIds.ITEMS, net.minecraft.client.renderer.texture.TextureAtlas.LOCATION_ITEMS));
        baked = bakery.bakeModels(materials, executor).join().blockStateModels();
        System.err.println("Horizon: baked " + baked.size() + " block states whose models mods build in code");
        return baked;
    }
}
