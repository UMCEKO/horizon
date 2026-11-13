package dev.horizonlab.javamods.boot;

import java.util.HashSet;
import java.util.List;
import java.util.Set;
import net.minecraft.client.DeltaTracker;
import net.neoforged.bus.api.Event;
import net.neoforged.neoforge.client.ClientHooks;
import net.neoforged.neoforge.client.event.RenderFrameEvent;
import net.neoforged.neoforge.common.NeoForge;

/** `RenderFrameEvent.Pre` and `.Post` for each window frame, only while a mod listens to them. */
final class FrameEvents {
    private FrameEvents() {}

    private static final List<Class<? extends Event>> EVENTS = List.of(RenderFrameEvent.Pre.class, RenderFrameEvent.Post.class);
    private static final Set<String> HEARD = new HashSet<>();

    /** Whether a loaded mod listens to a frame event; decided once when the mods have loaded. */
    static volatile boolean listened;

    static void decide() {
        listened = Listeners.modListens(NeoForge.EVENT_BUS, EVENTS, HEARD, "frame events");
        System.err.println("Horizon: RenderFrameEvent for " + (listened ? HEARD.size() + " mod listeners" : "none"));
    }

    /** `Minecraft.renderFrame`'s two moments: phase 0 is `Pre`, 1 is `Post`. */
    static void post(byte phase, DeltaTracker delta) {
        if (!listened) return;
        try {
            if (phase == 0) ClientHooks.fireRenderFramePre(delta);
            else ClientHooks.fireRenderFramePost(delta);
        } catch (Throwable t) {
            System.err.println("Horizon: a Java mod threw during RenderFrameEvent");
            t.printStackTrace();
        }
    }
}
