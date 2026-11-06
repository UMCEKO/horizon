package dev.horizonlab.javamods.testmod;

import com.mojang.blaze3d.platform.InputConstants;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.resources.Identifier;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.event.RegisterKeyMappingsEvent;
import net.neoforged.neoforge.client.settings.KeyConflictContext;
import net.neoforged.neoforge.client.settings.KeyModifier;
import net.neoforged.neoforge.common.NeoForge;

/** A key binding of the default context, polled as mods do: it must stay quiet while a screen of the window's is open. */
final class ClientKeys {
    private ClientKeys() {}

    private static final InputConstants.Key K = InputConstants.getKey("key.keyboard.k");
    private static final KeyMapping KEY = new KeyMapping("key.horizon_java_test.probe", KeyConflictContext.IN_GAME, KeyModifier.NONE,
            K, KeyMapping.Category.register(Identifier.parse("horizon_java_test:keys")));
    private static boolean wasDown;
    private static boolean hadScreen;

    static void register(IEventBus bus) {
        bus.addListener((RegisterKeyMappingsEvent event) -> event.register(KEY));
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> tick());
    }

    private static void tick() {
        Minecraft minecraft = Minecraft.getInstance();
        if (minecraft.level == null) return;
        boolean down = InputConstants.isKeyDown(KEY.getKey().getValue());
        boolean screenNow = minecraft.gui.screen() != null;
        if (down && wasDown && hadScreen && !screenNow && KEY.getKey().equals(K)) {
            // `Gui.setScreen(null)` grabs the mouse, whose `KeyMapping.setAll` presses what was held through the screen.
            Checks.check("key_held_through_a_native_screen_is_down", KEY::isDown);
        }
        hadScreen = screenNow;
        if (down && !wasDown && !KEY.getKey().equals(K)) {
            Checks.check("mod_key_follows_the_rebind", () -> KEY.isDown() && !KEY.isUnbound() && KEY.getKeyModifier() == KeyModifier.NONE);
        } else if (down && !wasDown) {
            boolean screen = minecraft.gui.screen() != null;
            if (screen) Checks.check("key_quiet_under_a_native_screen", () -> !KeyConflictContext.IN_GAME.isActive() && !KEY.isDown() && KeyConflictContext.GUI.isActive());
            else Checks.check("key_fires_without_a_screen", () -> KeyConflictContext.IN_GAME.isActive() && KEY.isDown());
        }
        wasDown = down;
    }
}
