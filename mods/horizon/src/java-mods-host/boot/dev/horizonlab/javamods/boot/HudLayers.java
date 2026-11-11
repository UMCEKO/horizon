package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.IntSupplier;
import java.util.function.Supplier;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.resources.Identifier;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.client.event.RenderGuiLayerEvent;
import net.neoforged.neoforge.client.gui.GuiLayerManager.NamedLayer;
import net.neoforged.neoforge.client.gui.VanillaGuiLayers;
import org.joml.Matrix3x2f;

/** The HUD layers vanilla registered before the mods' event, cut into the segments native paints in vanilla's order. */
public final class HudLayers {
    private HudLayers() {}

    /** `VanillaGuiLayers` in registration order; the index is the wire id. */
    static final Identifier[] ORDER = {VanillaGuiLayers.CAMERA_OVERLAYS, VanillaGuiLayers.CROSSHAIR, VanillaGuiLayers.AFTER_CAMERA_DECORATIONS, VanillaGuiLayers.HOTBAR,
            VanillaGuiLayers.PLAYER_HEALTH, VanillaGuiLayers.ARMOR_LEVEL, VanillaGuiLayers.FOOD_LEVEL, VanillaGuiLayers.VEHICLE_HEALTH, VanillaGuiLayers.AIR_LEVEL,
            VanillaGuiLayers.CONTEXTUAL_INFO_BAR_BACKGROUND, VanillaGuiLayers.EXPERIENCE_LEVEL, VanillaGuiLayers.CONTEXTUAL_INFO_BAR, VanillaGuiLayers.SELECTED_ITEM_NAME,
            VanillaGuiLayers.SPECTATOR_TOOLTIP, VanillaGuiLayers.EFFECTS, VanillaGuiLayers.BOSS_OVERLAY, VanillaGuiLayers.SLEEP_OVERLAY, VanillaGuiLayers.DEMO_OVERLAY,
            VanillaGuiLayers.SCOREBOARD_SIDEBAR, VanillaGuiLayers.OVERLAY_MESSAGE, VanillaGuiLayers.TITLE, VanillaGuiLayers.CHAT, VanillaGuiLayers.TAB_LIST,
            VanillaGuiLayers.SUBTITLE_OVERLAY};
    static final int COUNT = ORDER.length, SEGMENTS = 2 * COUNT + 1;
    private static final Map<Identifier, Integer> IDS = new HashMap<>();
    /** The layers placed by the bar heights mods stack on: the bars, the item name and the action bar. */
    private static final int HEIGHT_DEPENDENT = bits(4, 5, 6, 7, 8, 12, 19);

    static {
        for (int i = 0; i < COUNT; i++) IDS.put(ORDER[i], i);
    }

    private static int bits(int... ids) {
        int mask = 0;
        for (int id : ids) mask |= 1 << id;
        return mask;
    }

    private static boolean discarded;

    /** Whether a vanilla layer is running: what it plays as sound is native's, as the window already plays it. */
    public static boolean discarded() {
        return discarded;
    }

    /** One HUD recording in segments: 0 before layer 0, `1 + 2 * id` a layer's own content when Java drew it, `2 + 2 * id` the gap after it. */
    static final class Recording {
        final Recorder recorder;
        final IntSupplier left, right;
        final byte[][] segments = new byte[SEGMENTS][];
        int gap, cancelled, drawn;
        Matrix3x2f poseAtPre;
        int expectedLeft, expectedRight;
        boolean started;

        Recording(Recorder recorder, IntSupplier left, IntSupplier right) {
            this.recorder = recorder;
            this.left = left;
            this.right = right;
            for (int i = 0; i < SEGMENTS; i++) segments[i] = new byte[] {0, 0, 0, 0};
        }

        void put(int index) throws IOException {
            segments[index] = Recorder.concat(segments[index], recorder.cut());
        }

        /** The open gap ends and the one after `id` opens. */
        void passed(int id) throws IOException {
            put(gap);
            gap = 2 + 2 * id;
        }
    }

    private static Recording recording;

    /** Starts a HUD recording into `recorder`; `left` and `right` read the bar heights mods stack on. */
    static void begin(Recorder recorder, IntSupplier left, IntSupplier right) {
        recording = new Recording(recorder, left, right);
    }

    /** The recording as `int n` segments, `int cancelled`, `int drawn`; empty when none began. */
    static byte[] finish() throws IOException {
        var r = recording;
        recording = null;
        if (r == null) r = new Recording(new Recorder(), () -> 0, () -> 0);
        r.put(r.gap);
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(SEGMENTS);
        for (byte[] segment : r.segments) {
            out.writeInt(segment.length);
            out.write(segment);
        }
        out.writeInt(r.cancelled);
        out.writeInt(r.drawn);
        return bytes.toByteArray();
    }

    /** The listeners that see what mods do to a vanilla layer: a pose they moved in `Pre` (a wrap) and a cancel. */
    static void install(IEventBus bus) {
        EventPosts.own(RenderGuiLayerEvent.Pre.class);
        EventPosts.own(RenderGuiLayerEvent.Pre.class);
        bus.addListener(EventPriority.HIGHEST, RenderGuiLayerEvent.Pre.class, event -> {
            if (recording != null && IDS.containsKey(event.getName())) recording.poseAtPre = new Matrix3x2f(event.getGuiGraphics().pose());
        });
        bus.addListener(EventPriority.LOWEST, true, RenderGuiLayerEvent.Pre.class, event -> {
            var r = recording;
            Integer id = IDS.get(event.getName());
            if (r == null || id == null || !event.isCanceled()) return;
            r.cancelled |= 1 << id;
            try {
                r.passed(id);
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        });
    }

    /** Each vanilla layer still runs, so its layout bookkeeping (the bar heights mods stack on) happens. It draws into the recording only while a mod is detected to change it (a moved pose, or heights a mod raised before it), else into a target that keeps nothing: vanilla's pixels are native. */
    public static void discarding(List<NamedLayer> layers, Supplier<GuiGraphicsExtractor> sink) {
        layers.replaceAll(layer -> {
            Integer id = IDS.get(layer.name());
            return id == null ? layer : new NamedLayer(layer.name(), (graphics, delta) -> run(layer, id, graphics, delta, sink));
        });
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) {
            int last = -1;
            boolean ordered = true;
            for (var layer : layers) {
                Integer id = IDS.get(layer.name());
                if (id == null) continue;
                ordered &= id > last;
                last = id;
            }
            System.err.println(ordered ? "Horizon: HUD layers in vanilla order" : "Horizon: HUD layers NOT in vanilla order");
        }
    }

    private static boolean changed(Recording r, int id, GuiGraphicsExtractor graphics) {
        if (r.poseAtPre != null && !r.poseAtPre.equals(graphics.pose())) return true;
        return (HEIGHT_DEPENDENT & (1 << id)) != 0 && (r.left.getAsInt() != r.expectedLeft || r.right.getAsInt() != r.expectedRight);
    }

    private static void run(NamedLayer layer, int id, GuiGraphicsExtractor graphics, net.minecraft.client.DeltaTracker delta, Supplier<GuiGraphicsExtractor> sink) {
        var r = recording;
        boolean java = false;
        try {
            if (r != null) {
                if (!r.started) {
                    r.started = true;
                    r.expectedLeft = r.left.getAsInt();
                    r.expectedRight = r.right.getAsInt();
                }
                r.put(r.gap);
                java = changed(r, id, graphics);
            }
            int left = r == null ? 0 : r.left.getAsInt(), right = r == null ? 0 : r.right.getAsInt();
            discarded = true;
            try {
                layer.layer().render(java ? graphics : sink.get(), delta);
            } finally {
                discarded = false;
            }
            if (r != null) {
                r.expectedLeft += r.left.getAsInt() - left;
                r.expectedRight += r.right.getAsInt() - right;
                r.poseAtPre = null;
                if (java) {
                    r.drawn |= 1 << id;
                    r.put(1 + 2 * id);
                }
                r.gap = 2 + 2 * id;
            }
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }
}
