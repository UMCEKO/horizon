package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Deque;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Set;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.toasts.Toast;
import net.minecraft.client.gui.components.toasts.ToastManager;

/** The toasts mods add to the Java client's `ToastManager`: the window shows them in its own column, so Java only runs their `update` and records their drawing. */
public final class Toasts {
    private Toasts() {}

    /** One toast `addToast` took, with what the window was last told of it. */
    static final class Entry {
        final int id;
        final Toast toast;
        Toast.Visibility wanted = Toast.Visibility.SHOW;
        byte[] last;
        boolean animating;

        Entry(int id, Toast toast) {
            this.id = id;
            this.toast = toast;
        }
    }

    /** What `apply` found for one toast: its wanted visibility, and its recording when it differs from the last. */
    record Update(int id, Toast.Visibility wanted, byte[] changed) {}

    private static final LinkedHashMap<Integer, Entry> ENTRIES = new LinkedHashMap<>();
    private static final List<Integer> FRESH = new ArrayList<>();
    private static final List<Update> UPDATES = new ArrayList<>();
    private static final Set<Class<?>> PLACED = new HashSet<>();
    private static final boolean DEBUG = System.getenv("HORIZON_JAVA_DEBUG") != null;
    private static int next;
    private static boolean cleared;
    private static boolean animating;

    /** `ToastManager.addToast` after the toast joined `queued`. */
    public static synchronized void added(Toast toast) {
        int id = ++next;
        ENTRIES.put(id, new Entry(id, toast));
        FRESH.add(id);
        if (PLACED.add(toast.getClass()) && placesItself(toast.getClass())) {
            System.err.println("Horizon: toast " + toast.getClass().getName() + " places at its own x/y; drawn at the default place");
        }
    }

    /** `ToastManager.clear`: nothing shown or waiting any more. */
    public static synchronized void cleared() {
        ENTRIES.clear();
        FRESH.clear();
        UPDATES.clear();
        cleared = true;
        animating = false;
    }

    public static synchronized int count() {
        return ENTRIES.size();
    }

    private static boolean placesItself(Class<?> type) {
        for (var method : type.getDeclaredMethods()) if (method.getName().equals("xPos") || method.getName().equals("yPos")) return true;
        return false;
    }

    /** One shown toast's turn: finished ones leave `queued` and are forgotten; the rest `update`, and with `record` their drawing is kept when it differs. */
    static synchronized Update apply(int id, boolean finished, long fullyVisibleFor, boolean record, Deque<Toast> queued, ToastManager manager, float realtimeDeltaTicks) {
        Entry entry = ENTRIES.get(id);
        if (entry == null) return null;
        Toast toast = entry.toast;
        if (finished) {
            ENTRIES.remove(id);
            queued.remove(toast);
            toast.onFinishedRendering();
            if (DEBUG) System.err.println("Horizon: toast finished " + toast.getClass().getName());
            return null;
        }
        try {
            toast.update(manager, fullyVisibleFor);
            entry.wanted = toast.getWantedVisibility();
            byte[] changed = null;
            if (record) {
                byte[] bytes = draw(toast, fullyVisibleFor);
                boolean identical = Arrays.equals(bytes, entry.last);
                if (!identical) entry.animating = true;
                else if (HudAnimation.ends(true, realtimeDeltaTicks)) entry.animating = false;
                if (!identical) changed = bytes;
                entry.last = bytes;
            }
            return new Update(id, entry.wanted, changed);
        } catch (Throwable t) {
            System.err.println("Horizon: a Java mod threw during toast");
            t.printStackTrace();
            ENTRIES.remove(id);
            queued.remove(toast);
            return new Update(id, Toast.Visibility.HIDE, null);
        }
    }

    /** The toast's drawing at the origin, as the HUD is recorded. */
    private static byte[] draw(Toast toast, long fullyVisibleFor) throws IOException {
        Minecraft minecraft = Minecraft.getInstance();
        var recorder = new Recorder();
        var graphics = new GuiGraphicsExtractor(minecraft, recorder, 0, 0);
        toast.extractRenderState(graphics, minecraft.font, fullyVisibleFor);
        graphics.extractDeferredElements(0, 0, 0.0F);
        return recorder.bytes();
    }

    private static final List<long[]> PENDING = new ArrayList<>();

    /** A tick's input kind 26: `int n` x (`int id`, `bool finished`, `long fullyVisibleFor`), applied after the tick. */
    static synchronized void input(DataInputStream in) throws IOException {
        for (int n = in.readInt(); n > 0; n--) PENDING.add(new long[] {in.readInt(), in.readBoolean() ? 1 : 0, in.readLong()});
    }

    /** The input of this tick, after `minecraft.tick()`: updates wait for the status. */
    static synchronized void applyPending(Minecraft minecraft, float realtimeDeltaTicks) {
        if (PENDING.isEmpty()) return;
        ToastManager manager = minecraft.gui.toastManager();
        Deque<Toast> queued = queued(manager);
        for (long[] one : PENDING) {
            Update update = apply((int) one[0], one[1] != 0, one[2], true, queued, manager, realtimeDeltaTicks);
            if (update != null) UPDATES.add(update);
        }
        PENDING.clear();
        animating = anyAnimating();
    }

    private static boolean anyAnimating() {
        for (Entry entry : ENTRIES.values()) if (entry.animating) return true;
        return false;
    }

    private static Deque<Toast> queued(ToastManager manager) {
        return ((dev.horizonlab.javamods.boot.mixin.ToastQueueAccess) manager).horizon$queued();
    }

    /** The status's end: the toasts added since the last, whether `clear` ran, this tick's updates, and whether any still animates. */
    static synchronized void writeStatus(DataOutputStream out) throws IOException {
        out.writeInt(FRESH.size());
        for (int id : FRESH) {
            Toast toast = ENTRIES.get(id).toast;
            out.writeInt(id);
            out.writeInt(toast.width());
            out.writeInt(toast.height());
            out.writeInt(toast.occcupiedSlotCount());
            var sound = toast.getSoundEvent();
            text(out, sound == null ? "" : sound.location().toString());
        }
        FRESH.clear();
        out.writeBoolean(cleared);
        cleared = false;
        writeUpdates(out, UPDATES);
        UPDATES.clear();
        out.writeBoolean(animating);
    }

    static void writeUpdates(DataOutputStream out, List<Update> updates) throws IOException {
        out.writeInt(updates.size());
        for (Update update : updates) {
            out.writeInt(update.id());
            out.writeByte(update.wanted() == Toast.Visibility.SHOW ? 0 : 1);
            out.writeBoolean(update.changed() != null);
            if (update.changed() != null) {
                out.writeInt(update.changed().length);
                out.write(update.changed());
            }
        }
    }

    /** `client_toast_frame`: `int n` x (`int id`, `long fullyVisibleFor`) in, the updates and whether any toast still animates out. */
    static synchronized byte[] frame(DataInputStream in, Minecraft minecraft, float realtimeDeltaTicks) throws IOException {
        ToastManager manager = minecraft.gui.toastManager();
        Deque<Toast> queued = queued(manager);
        var updates = new ArrayList<Update>();
        for (int n = in.readInt(); n > 0; n--) {
            Update update = apply(in.readInt(), false, in.readLong(), true, queued, manager, realtimeDeltaTicks);
            if (update != null) updates.add(update);
        }
        animating = anyAnimating();
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        writeUpdates(out, updates);
        out.writeBoolean(animating);
        return bytes.toByteArray();
    }

    private static void text(DataOutputStream out, String value) throws IOException {
        byte[] b = value.getBytes(StandardCharsets.UTF_8);
        out.writeInt(b.length);
        out.write(b);
    }
}
