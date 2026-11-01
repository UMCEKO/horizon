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

    /** Measures text for mods that lay out at load; nothing draws through it, and every glyph is as wide as the default font's usual. */
    private static Font font() {
        BakedGlyph glyph = new BakedGlyph() {
            @Override
            public GlyphInfo info() {
                return GlyphInfo.simple(6);
            }

            @Override
            public TextRenderable.Styled createGlyph(float x, float y, int color, int shadow, Style style, float bold, float offset) {
                return null;
            }
        };
        GlyphSource source = new GlyphSource() {
            @Override
            public BakedGlyph getGlyph(int codepoint) {
                return glyph;
            }

            @Override
            public BakedGlyph getRandomGlyph(RandomSource random, int width) {
                return glyph;
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
        set(minecraft, "window", GameBridge.construct(Window.class, object));
        set(minecraft, "modelManager", GameBridge.construct(ModelManager.class, object));
        var resources = new ReloadableResourceManager(PackType.CLIENT_RESOURCES);
        // NeoForge sorts mods' listeners among the game's by class; these never reload, as Horizon reloads its own resources.
        Field known = VanillaClientListeners.class.getDeclaredField("KNOWN_CLASSES");
        known.setAccessible(true);
        for (Object listener : ((java.util.Map<?, ?>) known.get(null)).keySet()) {
            resources.registerReloadListener((net.minecraft.server.packs.resources.PreparableReloadListener) GameBridge.construct((Class<?>) listener, object));
        }
        set(minecraft, "resourceManager", resources);
        set(minecraft, "options", new Options(minecraft, folder));
        Gui gui = GameBridge.construct(Gui.class, object);
        Field hud = Gui.class.getDeclaredField("hud");
        hud.setAccessible(true);
        hud.set(gui, new Hud(minecraft));
        set(minecraft, "gui", gui);
        set(minecraft, "font", font());
        Inits.run(minecraft, config(user, folder));
        Inits.run(gui, minecraft);
        ClientHooks.initClientHooks(minecraft, resources);
        ClientModLoader.finish();
    }
}
