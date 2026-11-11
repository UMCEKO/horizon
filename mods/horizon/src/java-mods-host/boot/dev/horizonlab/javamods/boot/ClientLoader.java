package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.platform.Window;
import java.io.File;
import java.lang.reflect.Field;
import java.net.Proxy;
import java.util.Optional;
import java.util.UUID;
import net.minecraft.client.ClientBootstrap;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Options;
import net.minecraft.client.User;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.GlyphSource;
import net.minecraft.client.gui.Hud;
import net.minecraft.client.gui.font.TextRenderable;
import net.minecraft.client.gui.font.glyphs.BakedGlyph;
import net.minecraft.client.gui.font.glyphs.EffectGlyph;
import net.minecraft.client.main.GameConfig;
import net.minecraft.network.chat.FontDescription;
import net.minecraft.network.chat.Style;
import net.minecraft.util.RandomSource;
import com.mojang.blaze3d.font.GlyphInfo;
import com.mojang.blaze3d.platform.DisplayData;
import java.nio.file.Path;
import java.util.concurrent.CompletableFuture;
import net.minecraft.client.color.block.BlockColors;
import net.minecraft.client.particle.ParticleEngine;
import net.minecraft.client.particle.ParticleResources;
import net.minecraft.client.renderer.GameRenderer;
import net.minecraft.client.renderer.PlayerSkinRenderCache;
import net.minecraft.client.renderer.block.BlockModelResolver;
import net.minecraft.client.renderer.blockentity.BlockEntityRenderDispatcher;
import net.minecraft.client.renderer.entity.EntityRenderDispatcher;
import net.minecraft.client.renderer.item.ItemModelResolver;
import net.minecraft.client.renderer.texture.TextureManager;
import net.minecraft.client.resources.ClientPackSource;
import net.minecraft.client.resources.model.EquipmentAssetManager;
import net.minecraft.client.resources.model.ModelManager;
import net.minecraft.client.resources.model.sprite.AtlasManager;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.resources.Identifier;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.FolderRepositorySource;
import net.minecraft.server.packs.repository.PackRepository;
import net.minecraft.server.packs.repository.PackSource;
import net.minecraft.server.packs.resources.ReloadableResourceManager;
import net.minecraft.util.Unit;
import net.minecraft.util.Util;
import net.minecraft.util.thread.ReentrantBlockableEventLoop;
import net.minecraft.world.level.storage.LevelStorageSource;
import net.neoforged.fml.loading.FMLPaths;
import net.neoforged.neoforge.client.ClientHooks;
import net.neoforged.neoforge.client.ClientLifecycleHooks;
import net.neoforged.neoforge.client.loading.ClientModLoader;

/** What `Minecraft`'s constructor does, in its order, without a window or a GPU, ending with its first resource reload: only loaded on the client side, as these classes exist only there. */
final class ClientLoader {
    private ClientLoader() {}

    private static void set(Object target, String field, Object value) throws ReflectiveOperationException {
        Field f = Minecraft.class.getDeclaredField(field);
        f.setAccessible(true);
        f.set(target, value);
    }

    private static volatile float[] advances = new float[0];
    private static final java.util.Map<Integer, BakedGlyph> GLYPHS = new java.util.concurrent.ConcurrentHashMap<>();

    /** Horizon's font's advances by character, so text measures in Java as Horizon draws it. */
    static void advances(float[] table) {
        advances = table;
        GLYPHS.clear();
    }

    private static BakedGlyph glyph(int codepoint) {
        return GLYPHS.computeIfAbsent(codepoint, c -> {
            float[] table = advances;
            // 6 is vanilla's missing glyph, as `FontManager` answers until the first resource load.
            float advance = c < table.length ? table[c] : 6;
            return new BakedGlyph() {
                @Override
                public GlyphInfo info() {
                    return GlyphInfo.simple(advance);
                }

                @Override
                public TextRenderable.Styled createGlyph(float x, float y, int color, int shadow, Style style, float bold, float offset) {
                    return null;
                }
            };
        });
    }

    /** Measures text for mods; nothing draws through it, as Horizon draws the text itself. */
    private static Font font() {
        GlyphSource source = new GlyphSource() {
            @Override
            public BakedGlyph getGlyph(int codepoint) {
                return glyph(codepoint);
            }

            @Override
            public BakedGlyph getRandomGlyph(RandomSource random, int width) {
                return glyph('a');
            }
        };
        return new Font(new Font.Provider() {
            @Override
            public GlyphSource glyphs(FontDescription description) {
                return source;
            }

            @Override
            public EffectGlyph effect() {
                return null;
            }
        });
    }

    private static GameConfig config(User user, File folder) {
        var args = net.neoforged.fml.loading.FMLLoader.getCurrent().getProgramArgs();
        return new GameConfig(
                new GameConfig.UserData(user, Proxy.NO_PROXY),
                new DisplayData(854, 480, java.util.OptionalInt.empty(), java.util.OptionalInt.empty(), false),
                new GameConfig.FolderData(folder, folder.toPath().resolve("resourcepacks").toFile(), new File(args.get("assetsDir")), args.get("assetIndex")),
                new GameConfig.GameData(false, "26.3", "release", false, false, false, false, false, null, false),
                new GameConfig.QuickPlayData(null, GameConfig.QuickPlayVariant.DISABLED));
    }

    /** The integrated server's view knows its game, and the game knows it, as a real single-player world has it. */
    static void attach(MinecraftServer server) throws ReflectiveOperationException {
        Minecraft minecraft = Minecraft.getInstance();
        Field game = IntegratedServer.class.getDeclaredField("minecraft");
        game.setAccessible(true);
        game.set(server, minecraft);
        Field scope = IntegratedServer.class.getDeclaredField("multiplayerScope");
        scope.setAccessible(true);
        scope.set(server, MinecraftServer.MultiplayerScope.OFF);
        set(minecraft, "singleplayerServer", server);
    }

    private static volatile String reloaded;

    /** The player's language, with English under it as the game stacks them, for the client and the server in this JVM alike; the resource reload's own load stays when it was for this code. */
    static void language(String code) {
        var manager = Minecraft.getInstance().getLanguageManager();
        if (code.equals(reloaded) && code.equals(manager.getSelected()) && (code.equals("en_us") || manager.getLanguage(code) != null)) return;
        try {
            var current = net.minecraft.client.resources.language.LanguageManager.class.getDeclaredField("currentCode");
            current.setAccessible(true);
            current.set(Minecraft.getInstance().getLanguageManager(), code);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
        var stack = code.equals("en_us") ? java.util.List.of("en_us") : java.util.List.of("en_us", code);
        var resources = Minecraft.getInstance().getResourceManager();
        net.minecraft.locale.Language.inject(net.minecraft.client.resources.language.ClientLanguage.loadFrom(resources, stack, false));
        reloaded = null;
    }

    private static void set(Class<?> owner, Object target, String field, Object value) throws ReflectiveOperationException {
        Field f = owner.getDeclaredField(field);
        f.setAccessible(true);
        f.set(target, value);
    }

    /** `Minecraft`'s constructor from its pack repository to its first reload, with what needs a window or a GPU left out: each listener is the real one, made and registered in its order, so NeoForge's events fire where they do on a real client. */
    static void load() throws Exception {
        ClientModLoader.begin();
        var loop = ReentrantBlockableEventLoop.class.getDeclaredConstructor(String.class, boolean.class);
        Minecraft minecraft = GameBridge.construct(Minecraft.class, loop, "Client", false);
        set(null, "instance", minecraft);
        var object = Object.class.getDeclaredConstructor();
        File folder = FMLPaths.GAMEDIR.get().toFile();
        Path gameDirectory = folder.toPath();
        User user = new User("Player", new UUID(0, 0), "", Optional.empty(), Optional.empty());
        GameConfig config = config(user, folder);
        set(minecraft, "gameDirectory", folder);
        set(minecraft, "resourcePackDirectory", gameDirectory.resolve("resourcepacks"));
        set(minecraft, "launchedVersion", "26.3");
        var validator = LevelStorageSource.parseValidator(gameDirectory.resolve("allowed_symlinks.txt"));
        set(minecraft, "directoryValidator", validator);
        var clientPacks = new ClientPackSource(config.location.getExternalAssetSource(), validator);
        // Without the server's downloaded packs: Horizon applies a server's resource packs itself.
        var repository = new PackRepository(clientPacks, new FolderRepositorySource(gameDirectory.resolve("resourcepacks"), PackType.CLIENT_RESOURCES, PackSource.DEFAULT, validator));
        set(minecraft, "resourcePackRepository", repository);
        set(minecraft, "vanillaPackResources", clientPacks.getVanillaPack());
        set(minecraft, "proxy", Proxy.NO_PROXY);
        set(minecraft, "user", user);
        set(minecraft, "fixerUpper", net.minecraft.util.datafix.DataFixers.getDataFixer());
        set(minecraft, "options", new Options(minecraft, folder));
        set(minecraft, "debugEntries", new net.minecraft.client.gui.components.debug.DebugScreenEntryList(folder, net.minecraft.util.datafix.DataFixers.getDataFixer()));
        Window window = GameBridge.construct(Window.class, object);
        Field handle = Window.class.getDeclaredField("handle");
        handle.setAccessible(true);
        // Horizon's window is the one window, so the input handlers' `handle == window.handle()` holds for what it hands them.
        handle.setLong(window, 1L);
        Field videoMode = Window.class.getDeclaredField("preferredFullscreenVideoMode");
        videoMode.setAccessible(true);
        videoMode.set(window, Optional.empty());
        set(minecraft, "window", window);
        set(minecraft, "textInputManager", new com.mojang.blaze3d.platform.TextInputManager(window));
        set(minecraft, "metricsRecorder", net.minecraft.util.profiling.metrics.profiling.InactiveMetricsRecorder.INSTANCE);
        set(minecraft, "narrator", new net.minecraft.client.GameNarrator(minecraft));
        set(minecraft, "mouseHandler", new net.minecraft.client.MouseHandler(minecraft));
        set(minecraft, "keyboardHandler", new net.minecraft.client.KeyboardHandler(minecraft));
        var resources = new ReloadableResourceManager(PackType.CLIENT_RESOURCES);
        set(minecraft, "resourceManager", resources);
        ClientModLoader.setupModResourcePacks(repository);
        ClientBootstrap.bootstrap();
        repository.reload();
        minecraft.options.loadSelectedResourcePacks(repository);
        var languages = new net.minecraft.client.resources.language.LanguageManager(minecraft, minecraft.options.languageCode, language -> {
            reloaded = minecraft.options.languageCode;
            if (minecraft.player != null) minecraft.player.connection.updateSearchTrees();
        });
        set(minecraft, "languageManager", languages);
        resources.registerReloadListener(languages);
        var textures = new TextureManager(resources);
        set(minecraft, "textureManager", textures);
        resources.registerReloadListener(textures);
        // Its fields hold GPU buffers from the start and its reload is all GPU work, which `ShaderManagerMixin` leaves out.
        var shaders = GameBridge.construct(net.minecraft.client.renderer.ShaderManager.class, object);
        set(minecraft, "shaderManager", shaders);
        resources.registerReloadListener(shaders);
        set(minecraft, "musicManager", new net.minecraft.client.sounds.MusicManager(minecraft));
        var sounds = new net.minecraft.client.sounds.SoundManager(minecraft.options);
        set(minecraft, "soundManager", sounds);
        resources.registerReloadListener(sounds);
        var atlases = new AtlasManager(textures, minecraft.options.mipmapLevels().get());
        set(minecraft, "atlasManager", atlases);
        resources.registerReloadListener(atlases);
        // No skin downloads: Horizon fetches the skins it draws.
        var skins = new PlayerSkinRenderCache(textures, null, profiles(minecraft));
        set(minecraft, "playerSkinRenderCache", skins);
        // Its constructor bakes the missing glyphs into a GPU texture; Horizon's font draws all text, so `FontManagerMixin` leaves its reload out too.
        var fonts = GameBridge.construct(net.minecraft.client.gui.font.FontManager.class, object);
        set(minecraft, "fontManager", fonts);
        Font font = font();
        set(minecraft, "font", font);
        set(minecraft, "fontFilterFishy", font);
        resources.registerReloadListener(fonts);
        resources.registerReloadListener(new net.minecraft.client.resources.GrassColorReloadListener());
        resources.registerReloadListener(new net.minecraft.client.resources.FoliageColorReloadListener());
        resources.registerReloadListener(new net.minecraft.client.resources.DryFoliageColorReloadListener());
        var colors = BlockColors.createDefault();
        set(minecraft, "blockColors", colors);
        net.neoforged.neoforge.client.model.UnbakedModelParser.init();
        var models = new ModelManager(colors, atlases, skins);
        set(minecraft, "modelManager", models);
        resources.registerReloadListener(models);
        var equipment = new EquipmentAssetManager();
        resources.registerReloadListener(equipment);
        var blockModels = new BlockModelResolver(models);
        set(minecraft, "blockModelResolver", blockModels);
        var itemModels = new ItemModelResolver(models);
        set(minecraft, "itemModelResolver", itemModels);
        var paletted = new net.minecraft.client.resources.palette.PalettedTextureManager(resources, textures);
        set(minecraft, "palettedTextureManager", paletted);
        resources.registerReloadListener(paletted);
        var mapTextures = new net.minecraft.client.resources.MapTextureManager(textures);
        set(minecraft, "mapTextureManager", mapTextures);
        var maps = new net.minecraft.client.renderer.MapRenderer(atlases, mapTextures);
        set(minecraft, "mapRenderer", maps);
        var entities = new EntityRenderDispatcher(textures, blockModels, itemModels, maps, atlases, font, minecraft.options, models.entityModels(), equipment, skins, paletted);
        set(minecraft, "entityRenderDispatcher", entities);
        resources.registerReloadListener(entities);
        var blockEntities = new BlockEntityRenderDispatcher(font, models.entityModels(), blockModels, itemModels, entities, atlases, skins);
        set(minecraft, "blockEntityRenderDispatcher", blockEntities);
        resources.registerReloadListener(blockEntities);
        var particles = new ParticleResources();
        resources.registerReloadListener(particles);
        var engine = new ParticleEngine(null, particles);
        set(minecraft, "particleEngine", engine);
        particles.onReload(engine::clearParticles);
        ClientHooks.onRegisterParticleProviders(particles);
        // The game renderer's own constructor makes render targets and buffers on the GPU: a view keeps its state, camera and feature renderers.
        var renderer = GameBridge.construct(GameRenderer.class, object);
        var state = new net.minecraft.client.renderer.state.GameRenderState();
        set(GameRenderer.class, renderer, "gameRenderState", state);
        set(GameRenderer.class, renderer, "minecraft", minecraft);
        set(GameRenderer.class, renderer, "mainCamera", new net.minecraft.client.Camera());
        var buffers = GameBridge.construct(net.minecraft.client.renderer.RenderBuffers.class, object);
        set(GameRenderer.class, renderer, "featureRenderDispatcher", new net.minecraft.client.renderer.feature.FeatureRenderDispatcher(buffers, models, atlases, font, state));
        set(minecraft, "gameRenderer", renderer);
        resources.registerReloadListener(renderer);
        var extractor = new net.minecraft.client.renderer.extract.LevelExtractor(minecraft, state.levelRenderState, null);
        set(minecraft, "levelExtractor", extractor);
        resources.registerReloadListener(extractor);
        // `LevelRenderer`'s cloud renderer, whose constructor maps a GPU buffer: its reload reads the cloud texture all the same.
        resources.registerReloadListener(GameBridge.construct(net.minecraft.client.renderer.CloudRenderer.class, object));
        var warnlist = new net.minecraft.client.renderer.GpuWarnlistManager();
        set(minecraft, "gpuWarnlistManager", warnlist);
        resources.registerReloadListener(warnlist);
        var countries = Minecraft.class.getDeclaredMethod("countryEqualsISO3", Object.class);
        countries.setAccessible(true);
        var compliance = new net.minecraft.client.PeriodicNotificationManager(Identifier.withDefaultNamespace("regional_compliancies.json"), country -> {
            try {
                return (boolean) countries.invoke(null, country);
            } catch (ReflectiveOperationException e) {
                return false;
            }
        });
        set(minecraft, "regionalCompliancies", compliance);
        resources.registerReloadListener(compliance);
        Gui gui = new Gui(minecraft, new Hud(minecraft), state.guiRenderState);
        set(minecraft, "gui", gui);
        gui.registerReloadListeners(resources);
        set(minecraft, "deltaTracker", new net.minecraft.client.DeltaTracker.Timer(20.0F, 0L, target -> target));
        set(minecraft, "quickPlayLog", net.minecraft.client.quickplay.QuickPlayLog.of(null));
        set(minecraft, "framerateLimitTracker", new com.mojang.blaze3d.platform.FramerateLimitTracker(minecraft.options, minecraft));
        Inits.run(minecraft, config);
        Inits.run(gui, minecraft);
        ClientHooks.initClientHooks(minecraft, resources);
        ClientModLoader.finish();
        reload(minecraft, resources, repository);
        ClientBridge.install();
    }

    /** `createReload` over the selected packs with `Minecraft` as the reload's main executor, as its constructor starts it; `run` starting meanwhile says the client has started, and the end is `onResourceLoadFinished`'s. */
    private static void reload(Minecraft minecraft, ReloadableResourceManager resources, PackRepository repository) throws Exception {
        // The reload's main-thread steps run on the thread that builds the client, as on a real one; the render thread takes the client over after.
        set(minecraft, "gameThread", Thread.currentThread());
        var initial = Minecraft.class.getDeclaredField("RESOURCE_RELOAD_INITIAL_TASK");
        initial.setAccessible(true);
        @SuppressWarnings("unchecked")
        var task = (CompletableFuture<Unit>) initial.get(null);
        var reload = resources.createReload(Util.backgroundExecutor().forName("resourceLoad"), minecraft, task, repository.openAllSelected());
        ClientLifecycleHooks.handleClientStarted(minecraft);
        minecraft.managedBlock(reload::isDone);
        reload.checkExceptions();
        set(minecraft, "gameLoadFinished", true);
        // `onGameLoadFinished`'s first screens are left out: Horizon shows its own title screen.
        ClientHooks.fireResourceLoadFinishedEvent(true);
        ClientResources.take();
        Environments.decide();
    }

    /** Vanilla's client resolver: the connection's player info first; with no session service, a stranger resolves to nothing. */
    static net.minecraft.server.players.ProfileResolver profiles(Minecraft minecraft) {
        return new net.minecraft.client.player.LocalPlayerResolver(minecraft, new net.minecraft.server.players.ProfileResolver() {
            @Override
            public Optional<com.mojang.authlib.GameProfile> fetchByName(String name) {
                return Optional.empty();
            }

            @Override
            public Optional<com.mojang.authlib.GameProfile> fetchById(java.util.UUID id) {
                return Optional.empty();
            }
        });
    }
}
