package dev.horizonlab.javamods.boot;

import java.util.List;
import java.util.function.Supplier;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.neoforged.neoforge.client.gui.GuiLayerManager.NamedLayer;

/** The HUD layers vanilla registered before the mods' event. */
public final class HudLayers {
    private HudLayers() {}

    private static boolean discarded;

    /** Whether a vanilla layer is running into the discarded target: what it plays is native's, as the window already plays it. */
    public static boolean discarded() {
        return discarded;
    }

    /** Each layer still runs, so its layout bookkeeping (the bar heights mods stack on) happens, but into a graphics target that keeps nothing: vanilla's pixels are native. */
    public static void discarding(List<NamedLayer> layers, Supplier<GuiGraphicsExtractor> sink) {
        layers.replaceAll(layer -> new NamedLayer(layer.name(), (graphics, delta) -> {
            discarded = true;
            try {
                layer.layer().render(sink.get(), delta);
            } finally {
                discarded = false;
            }
        }));
    }
}
