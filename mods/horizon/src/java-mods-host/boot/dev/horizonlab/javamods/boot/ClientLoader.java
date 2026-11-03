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
import net.minecraft.client.resources.model.ModelManager;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.resources.ReloadableResourceManager;
import net.minecraft.util.thread.ReentrantBlockableEventLoop;
import net.neoforged.fml.loading.FMLPaths;
import net.neoforged.neoforge.client.ClientHooks;
import net.neoforged.neoforge.client.loading.ClientModLoader;
import net.neoforged.neoforge.client.resources.VanillaClientListeners;

/** What `Minecraft`'s constructor does for mods, without a window: only loaded on the client side, as these classes exist only there. */
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

    /** Measures text for mods; nothing draws through it, as Horizon draws the text itself. Until Horizon's font arrives every glyph is the default font's usual 6. */
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
        return new GameConfig(
                new GameConfig.UserData(user, Proxy.NO_PROXY),
                new DisplayData(854, 480, java.util.OptionalInt.empty(), java.util.OptionalInt.empty(), false),
                new GameConfig.FolderData(folder, folder.toPath().resolve("resourcepacks").toFile(), folder.toPath().resolve("assets").toFile(), null),
                new GameConfig.GameData(false, "26.3", "release", false, false, false, false, false, null, false),
                new GameConfig.QuickPlayData(null, GameConfig.QuickPlayVariant.DISABLED));
    }

    /** The integrated server's view knows its game, and the game knows it, as a real single-player world has it. */
    static void attach(MinecraftServer server) throws ReflectiveOperationException {
        Minecraft minecraft = Minecraft.getInstance();
        Field game = IntegratedServer.class.getDeclaredField("minecraft");
        game.setAccessible(true);
        game.set(server, minecraft);
        set(minecraft, "singleplayerServer", server);
    }

    /** The game's and mods' client resources, readable as after a reload, without running the reload: Horizon loads what it draws. */
    private static void packs(ReloadableResourceManager resources, File folder) throws ReflectiveOperationException {
        var repository = new net.minecraft.server.packs.repository.PackRepository(
                new net.minecraft.client.resources.ClientPackSource(folder.toPath().resolve("assets"), new net.minecraft.world.level.validation.DirectoryValidator(path -> true)));
        net.neoforged.neoforge.resource.ResourcePackLoader.populatePackRepository(repository, PackType.CLIENT_RESOURCES, false);
        repository.reload();
        repository.setSelected(repository.getAvailableIds());
        var manager = new net.minecraft.server.packs.resources.MultiPackResourceManager(PackType.CLIENT_RESOURCES, repository.openAllSelected());
        Field held = ReloadableResourceManager.class.getDeclaredField("resources");
        held.setAccessible(true);
        held.set(resources, manager);
    }

    /** The player's language, with English under it as the game stacks them, for the client and the server in this JVM alike. */
    static void language(String code) {
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
    }

    /** The reload listeners mods added run once over the packs, as a first resource load runs them; the game's own stay idle, as Horizon loads what it draws. */
    private static void reloadMods(ReloadableResourceManager resources, Object vanilla) throws ReflectiveOperationException {
        Field held = ReloadableResourceManager.class.getDeclaredField("listeners");
        held.setAccessible(true);
        var known = ((java.util.Map<?, ?>) vanilla).keySet();
        for (var listener : new java.util.ArrayList<>((java.util.List<net.minecraft.server.packs.resources.PreparableReloadListener>) held.get(resources))) {
            if (known.contains(listener.getClass())) continue;
            try {
                net.minecraft.server.packs.resources.SimpleReloadInstance.of(resources, java.util.List.of(listener), net.minecraft.util.Util.backgroundExecutor(), Runnable::run,
                        java.util.concurrent.CompletableFuture.completedFuture(net.minecraft.util.Unit.INSTANCE)).done().join();
            } catch (Throwable t) {
                System.err.println("Horizon: a mod's client reload listener " + listener.getClass().getName() + " failed: " + t);
            }
        }
    }

    static void load() throws ReflectiveOperationException {
        ClientModLoader.begin();
        ClientBootstrap.bootstrap();
        var loop = ReentrantBlockableEventLoop.class.getDeclaredConstructor(String.class, boolean.class);
        Minecraft minecraft = GameBridge.construct(Minecraft.class, loop, "Client", false);
        set(null, "instance", minecraft);
        var object = Object.class.getDeclaredConstructor();
        File folder = FMLPaths.GAMEDIR.get().toFile();
        set(minecraft, "gameDirectory", folder);
        set(minecraft, "resourcePackDirectory", folder.toPath().resolve("resourcepacks"));
        set(minecraft, "proxy", Proxy.NO_PROXY);
        set(minecraft, "launchedVersion", "26.3");
        User user = new User("Player", new UUID(0, 0), "", Optional.empty(), Optional.empty());
        set(minecraft, "user", user);
        Window window = GameBridge.construct(Window.class, object);
        Field handle = Window.class.getDeclaredField("handle");
        handle.setAccessible(true);
        // Horizon's window is the one window, so the input handlers' `handle == window.handle()` holds for what it hands them.
        handle.setLong(window, 1L);
        set(minecraft, "window", window);
        set(minecraft, "modelManager", GameBridge.construct(ModelManager.class, object));
        set(minecraft, "languageManager", new net.minecraft.client.resources.language.LanguageManager(minecraft, "en_us", language -> {}));
        set(minecraft, "deltaTracker", new net.minecraft.client.DeltaTracker.Timer(20.0F, 0L, target -> target));
        set(minecraft, "soundManager", GameBridge.construct(net.minecraft.client.sounds.SoundManager.class, object));
        set(minecraft, "textureManager", GameBridge.construct(net.minecraft.client.renderer.texture.TextureManager.class, object));
        set(minecraft, "atlasManager", GameBridge.construct(net.minecraft.client.resources.model.sprite.AtlasManager.class, object));
        var resources = new ReloadableResourceManager(PackType.CLIENT_RESOURCES);
        // NeoForge sorts mods' listeners among the game's by class; these never reload, as Horizon reloads its own resources.
        Field known = VanillaClientListeners.class.getDeclaredField("KNOWN_CLASSES");
        known.setAccessible(true);
        for (Object listener : ((java.util.Map<?, ?>) known.get(null)).keySet()) {
            resources.registerReloadListener((net.minecraft.server.packs.resources.PreparableReloadListener) GameBridge.construct((Class<?>) listener, object));
        }
        set(minecraft, "resourceManager", resources);
        packs(resources, folder);
        set(minecraft, "options", new Options(minecraft, folder));
        set(minecraft, "framerateLimitTracker", new com.mojang.blaze3d.platform.FramerateLimitTracker(minecraft.options, minecraft));
        set(minecraft, "mouseHandler", new net.minecraft.client.MouseHandler(minecraft));
        Gui gui = GameBridge.construct(Gui.class, object);
        Field hud = Gui.class.getDeclaredField("hud");
        hud.setAccessible(true);
        hud.set(gui, new Hud(minecraft));
        Field layers = Gui.class.getDeclaredField("screenLayers");
        layers.setAccessible(true);
        layers.set(gui, new java.util.Stack<>());
        set(minecraft, "gui", gui);
        set(minecraft, "font", font());
        Inits.run(minecraft, config(user, folder));
        Inits.run(gui, minecraft);
        ClientHooks.initClientHooks(minecraft, resources);
        ClientModLoader.finish();
        reloadMods(resources, known.get(null));
        ClientBridge.install();
    }
}
