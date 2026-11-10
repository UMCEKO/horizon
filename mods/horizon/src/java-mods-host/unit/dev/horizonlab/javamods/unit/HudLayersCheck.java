package dev.horizonlab.javamods.unit;

import dev.horizonlab.javamods.boot.HudLayers;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.resources.Identifier;
import net.neoforged.neoforge.client.gui.GuiLayerManager.NamedLayer;

/** Vanilla's layers run, into the discarded target, before a mod's layer reads the heights they leave. */
public final class HudLayersCheck {
    private static int leftHeight = 39;

    private static GuiGraphicsExtractor target() throws Exception {
        return (GuiGraphicsExtractor) sun.reflect.ReflectionFactory.getReflectionFactory().newConstructorForSerialization(GuiGraphicsExtractor.class, Object.class.getDeclaredConstructor()).newInstance();
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) throws Exception {
        var real = target();
        var discarded = target();
        var seen = new ArrayList<GuiGraphicsExtractor>();
        boolean[] during = new boolean[2];
        var layers = new ArrayList<NamedLayer>(List.of(
                new NamedLayer(Identifier.parse("minecraft:player_health"), (g, d) -> { seen.add(g); leftHeight += 10; }),
                new NamedLayer(Identifier.parse("minecraft:armor_level"), (g, d) -> { seen.add(g); leftHeight += 10; }),
                new NamedLayer(Identifier.parse("minecraft:air_level"), (g, d) -> during[0] = HudLayers.discarded())));
        HudLayers.discarding(layers, () -> discarded);
        int[] modSaw = new int[1];
        layers.add(new NamedLayer(Identifier.parse("mod:energy"), (g, d) -> { modSaw[0] = leftHeight; during[1] = HudLayers.discarded(); }));
        for (var layer : layers) layer.layer().render(real, DeltaTracker.ONE);
        check(seen.size() == 2 && seen.get(0) == discarded && seen.get(1) == discarded, "vanilla layers draw into the discarded target");
        check(modSaw[0] == 59, "a mod layer sees the heights vanilla's layers raised");
        check(during[0], "a vanilla layer runs flagged, so its sounds are dropped");
        check(!during[1] && !HudLayers.discarded(), "a mod's layer, and anything after, isn't flagged");
        check(layers.get(0).name().equals(Identifier.parse("minecraft:player_health")), "names stay");
        System.out.println("HudLayersCheck ok");
    }
}
