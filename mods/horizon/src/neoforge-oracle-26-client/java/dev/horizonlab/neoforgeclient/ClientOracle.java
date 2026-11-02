package dev.horizonlab.neoforgeclient;

import java.nio.file.Path;
import net.minecraft.client.Minecraft;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.client.event.RenderFrameEvent;
import net.neoforged.neoforge.common.NeoForge;

/** Waits for the first frame after the resources load, bakes, and quits. */
@Mod("horizon_oracle_client")
public final class ClientOracle {
    private static boolean done;

    public ClientOracle(IEventBus bus) {
        NeoForge.EVENT_BUS.addListener(RenderFrameEvent.Pre.class, e -> {
            if (done || Minecraft.getInstance().gui.overlay() != null) return;
            done = true;
            int status = 0;
            try {
                Baker.run(Path.of(System.getProperty("horizon.oracle.baked")));
            } catch (Throwable t) {
                t.printStackTrace();
                status = 1;
            }
            Runtime.getRuntime().halt(status);
        });
    }
}
