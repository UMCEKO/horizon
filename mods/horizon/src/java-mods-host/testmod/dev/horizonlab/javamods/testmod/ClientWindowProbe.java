package dev.horizonlab.javamods.testmod;

import net.minecraft.client.Minecraft;
import net.minecraft.sounds.SoundSource;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.event.RenderFrameEvent;
import net.neoforged.neoforge.client.event.WindowResizeEvent;
import net.neoforged.neoforge.common.NeoForge;

/** Listeners on the window's own events: resizes, the two moments of a frame, and the volumes the player sets. */
final class ClientWindowProbe {
    private ClientWindowProbe() {}

    private static int resizes, scaledWidth = -1, pres, posts;
    private static boolean preOpen, paired = true, resized, framed, volumed;

    static void register() {
        NeoForge.EVENT_BUS.addListener((WindowResizeEvent event) -> {
            resizes++;
            scaledWidth = Minecraft.getInstance().getWindow().getGuiScaledWidth();
        });
        NeoForge.EVENT_BUS.addListener((RenderFrameEvent.Pre event) -> {
            if (preOpen) paired = false;
            preOpen = true;
            pres++;
        });
        NeoForge.EVENT_BUS.addListener((RenderFrameEvent.Post event) -> {
            if (!preOpen) paired = false;
            preOpen = false;
            posts++;
        });
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> tick());
    }

    private static void tick() {
        Minecraft minecraft = Minecraft.getInstance();
        if (minecraft.player == null) return;
        var window = minecraft.getWindow();
        if (!resized && resizes > 0) {
            resized = true;
            int expected = (int) Math.ceil(window.getWidth() / (double) window.getGuiScale());
            Checks.check("window_resize_fired", () -> scaledWidth == expected);
        }
        // A frame may sit between its two moments while this tick runs: the counts differ by one then.
        if (!framed && pres > 20 && Math.abs(pres - posts) <= 1) {
            framed = true;
            Checks.check("render_frame_paired", () -> paired && posts > 0);
        }
        if (!volumed && minecraft.options.getSoundSourceVolume(SoundSource.MASTER) == 0.25F) {
            volumed = true;
            Checks.check("master_volume_follows", () -> true);
        }
    }
}
