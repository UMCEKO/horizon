package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.renderer.state.gui.GuiRenderState;
import net.minecraft.resources.Identifier;
import net.neoforged.bus.api.BusBuilder;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.neoforge.client.event.RenderGuiLayerEvent;
import net.neoforged.neoforge.client.gui.GuiLayerManager.NamedLayer;
import org.joml.Matrix3x2fStack;

/** The HUD recording in vanilla's layer order: a mod's layer lands in the gap after the vanilla layer it follows, a moved pose or raised heights make Java draw that layer, a cancel is reported. */
public final class HudSegmentsCheck {
    private static GuiGraphicsExtractor graphics() throws Exception {
        var g = (GuiGraphicsExtractor) sun.reflect.ReflectionFactory.getReflectionFactory().newConstructorForSerialization(GuiGraphicsExtractor.class, Object.class.getDeclaredConstructor()).newInstance();
        var pose = GuiGraphicsExtractor.class.getDeclaredField("pose");
        pose.setAccessible(true);
        pose.set(g, new Matrix3x2fStack(8));
        return g;
    }

    private static void blur(Recorder recorder) throws Exception {
        var field = GuiRenderState.class.getDeclaredField("firstStratumAfterBlur");
        field.setAccessible(true);
        field.setInt(recorder, Integer.MAX_VALUE);
        recorder.blurBeforeThisStratum();
    }

    private static int entries(byte[] segment) throws Exception {
        return new DataInputStream(new ByteArrayInputStream(segment)).readInt();
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) throws Exception {
        var recorder = new Recorder();
        var real = graphics();
        var sink = graphics();
        int[] left = {39};
        var bus = BusBuilder.builder().build();
        HudLayers.install(bus);
        bus.addListener(EventPriority.NORMAL, RenderGuiLayerEvent.Pre.class, event -> {
            if (event.getName().equals(Identifier.parse("minecraft:player_health"))) event.getGuiGraphics().pose().pushMatrix().translate(0, -5);
            if (event.getName().equals(Identifier.parse("minecraft:armor_level"))) event.setCanceled(true);
            if (event.getName().equals(Identifier.parse("minecraft:food_level"))) left[0] += 10;
        });
        bus.start();
        var layers = new ArrayList<NamedLayer>();
        for (String name : List.of("crosshair", "hotbar", "player_health", "armor_level", "food_level", "air_level")) {
            layers.add(new NamedLayer(Identifier.parse("minecraft:" + name), (g, d) -> {
                if (g == real) {
                    try {
                        blur(recorder);
                    } catch (Exception e) {
                        throw new IllegalStateException(e);
                    }
                }
            }));
        }
        HudLayers.discarding(layers, () -> sink);
        layers.add(1, new NamedLayer(Identifier.parse("mod:energy"), (g, d) -> {
            try {
                blur(recorder);
            } catch (Exception e) {
                throw new IllegalStateException(e);
            }
        }));
        HudLayers.begin(recorder, () -> left[0], () -> 39);
        for (var layer : layers) {
            if (!bus.post(new RenderGuiLayerEvent.Pre(real, DeltaTracker.ONE, layer.name(), layer.layer())).isCanceled()) layer.layer().render(real, DeltaTracker.ONE);
        }
        var in = new DataInputStream(new ByteArrayInputStream(HudLayers.finish()));
        check(in.readInt() == HudLayers.SEGMENTS && HudLayers.SEGMENTS == 49, "49 segments");
        var segments = new byte[HudLayers.SEGMENTS][];
        for (int i = 0; i < segments.length; i++) {
            segments[i] = new byte[in.readInt()];
            in.readFully(segments[i]);
        }
        int cancelled = in.readInt(), drawn = in.readInt();
        check(entries(segments[2 + 2 * 1]) == 1, "the mod's layer sits in the gap after the crosshair");
        check(entries(segments[2 + 2 * 3]) == 0, "nothing in the gap after the hotbar");
        check((drawn & (1 << 4)) != 0 && entries(segments[1 + 2 * 4]) == 1, "a moved pose makes Java draw the layer and keeps its content");
        check((drawn & (1 << 1)) == 0 && (drawn & (1 << 3)) == 0, "unchanged layers stay native");
        check((cancelled & (1 << 5)) != 0 && (drawn & (1 << 5)) == 0, "a cancelled layer is reported, not drawn");
        check((drawn & (1 << 6)) != 0, "heights a mod raised before the layer make Java draw it");
        check((drawn & (1 << 8)) != 0, "the layers placed after a raised height are drawn too");
        System.out.println("HudSegmentsCheck ok");
    }
}
