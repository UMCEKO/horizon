package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.mixin.ShellAccess;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Set;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.neoforged.bus.api.Event;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.neoforge.client.event.ScreenEvent;
import net.neoforged.neoforge.common.NeoForge;

/** A vanilla screen native owns, opened in the Java client as the real class so mods' screen events fire on it; its drawing and handler bodies never run, mods' widgets and events do. */
public final class Shells {
    private Shells() {}

    public static final int EVENT = 0, WIDGET = 1, NATIVE = 2, NONE = 3;
    static final int SEGMENTS = ShellLeaves.COUNT * 2;

    /** The open shell, kept while a mod's screen covers it: a mod returns to the same object. */
    static Screen current;
    /** The widgets vanilla's `init()` made, which native draws and handles. */
    static final Set<Object> vanilla = Collections.newSetFromMap(new IdentityHashMap<>());
    private static Set<Object> before = Collections.emptySet();
    /** leftPos, topPos, imageWidth, imageHeight, hovered menu slot (-1 none), as native lays the screen out now. */
    static int[] layout = {0, 0, 176, 166, -1};
    static boolean nativeWidget, nativeHandles;
    static int owner = EVENT;
    static byte[][] last = new byte[SEGMENTS][];
    static int lastRouted;
    /** Bits of the steps Java drew in the last recording; native skips its own for them. */
    static int routed;
    static boolean animating;
    private static final Set<String> HEARD = new java.util.HashSet<>();
    private static boolean cancelledLogged;

    public static boolean is(Object screen) {
        return screen != null && screen == current;
    }

    // --- Whether a mod listens

    private static final List<Class<? extends Event>> EVENTS = List.of(ScreenEvent.Opening.class, ScreenEvent.Init.Pre.class, ScreenEvent.Init.Post.class,
            ScreenEvent.Render.Pre.class, ScreenEvent.Render.Background.class, ScreenEvent.Render.Foreground.class, ScreenEvent.Render.Post.class,
            ScreenEvent.MouseButtonPressed.Pre.class, ScreenEvent.MouseButtonPressed.Post.class, ScreenEvent.MouseButtonReleased.Pre.class, ScreenEvent.MouseButtonReleased.Post.class,
            ScreenEvent.MouseScrolled.Pre.class, ScreenEvent.MouseScrolled.Post.class, ScreenEvent.MouseDragged.Pre.class, ScreenEvent.MouseDragged.Post.class,
            ScreenEvent.KeyPressed.Pre.class, ScreenEvent.KeyPressed.Post.class, ScreenEvent.KeyReleased.Pre.class, ScreenEvent.KeyReleased.Post.class,
            ScreenEvent.CharacterTyped.Pre.class, ScreenEvent.CharacterTyped.Post.class, ScreenEvent.Closing.class);

    /** Whether a class is a mod's: not NeoForge's, Minecraft's, Horizon's or the JDK's. */
    static boolean isMods(Class<?> owner) {
        String module = owner.getModule().getName();
        if (module != null) return !(module.equals("neoforge") || module.equals("minecraft") || module.equals(Shells.class.getModule().getName()) || module.startsWith("java.") || module.startsWith("jdk."));
        String name = owner.getName();
        return !(name.startsWith("java.") || name.startsWith("jdk.") || name.startsWith("net.neoforged.") || name.startsWith("net.minecraft.") || name.startsWith("dev.horizonlab.javamods."));
    }

    private static Class<?> ownerOf(net.neoforged.bus.api.EventListener listener) throws ReflectiveOperationException {
        if (listener instanceof net.neoforged.bus.IWrapperListener wrapper) listener = wrapper.getWithoutCheck();
        if (listener instanceof net.neoforged.bus.ConsumerEventHandler) {
            var consumer = net.neoforged.bus.ConsumerEventHandler.class.getDeclaredField("consumer");
            consumer.setAccessible(true);
            return consumer.get(listener).getClass();
        }
        return listener.getClass();
    }

    /** Whether a mod listens to a screen event on the bus, read as the moment vanilla would post `Opening`: JEI's listeners exist only once its runtime started. */
    static boolean wanted(net.neoforged.bus.api.IEventBus bus) {
        try {
            var list = net.neoforged.bus.EventBus.class.getDeclaredMethod("getListenerList", Class.class);
            list.setAccessible(true);
            boolean found = false;
            for (var event : EVENTS) {
                for (var listener : ((net.neoforged.bus.ListenerList) list.invoke(bus, event)).getListeners()) {
                    Class<?> owner = ownerOf(listener);
                    if (!isMods(owner)) continue;
                    found = true;
                    if (HEARD.add(owner.getName())) System.err.println("Horizon: screen events heard by " + owner.getName());
                }
            }
            return found;
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    // --- Opening and closing

    /** Native opened one of its vanilla screens: with a mod listening, the Java client opens the real class as a shell. */
    static void open(int kind, Minecraft minecraft) {
        if (kind != 1 || !wanted(NeoForge.EVENT_BUS) || is(minecraft.gui.screen())) return;
        Screen shell;
        try {
            shell = new InventoryScreen(minecraft.player);
        } catch (Throwable t) {
            System.err.println("Horizon: the Java client's inventory shell failed; native's screen stays alone");
            t.printStackTrace();
            return;
        }
        current = shell;
        last = new byte[SEGMENTS][];
        lastRouted = 0;
        animating = false;
        minecraft.gui.setScreen(shell);
        if (minecraft.gui.screen() != shell) current = null;
    }

    /** Native closed its screen. A mod's screen over the shell keeps it: the mod sets the same object back. */
    static void close(Minecraft minecraft) {
        if (!is(minecraft.gui.screen())) return;
        minecraft.gui.setScreen(null);
        current = null;
    }

    // --- Init bookkeeping

    private static Set<Object> identity(List<? extends GuiEventListener> children) {
        Set<Object> set = Collections.newSetFromMap(new IdentityHashMap<>());
        set.addAll(children);
        return set;
    }

    static void install() {
        var bus = NeoForge.EVENT_BUS;
        bus.addListener(EventPriority.LOWEST, true, ScreenEvent.Init.Pre.class, event -> {
            if (is(event.getScreen())) before = identity(event.getScreen().children());
        });
        bus.addListener(EventPriority.HIGHEST, ScreenEvent.Init.Post.class, event -> {
            Screen screen = event.getScreen();
            if (!is(screen)) return;
            vanilla.clear();
            for (var child : screen.children()) if (!before.contains(child)) vanilla.add(child);
            vanilla.addAll(screen.renderables);
            apply(screen);
        });
    }

    /** The layout native sent, written where vanilla's `init` and `extractContents` would have: mods read it with the getters. */
    static void apply(Screen screen) {
        if (!(screen instanceof AbstractContainerScreen<?> container)) return;
        var access = (ShellAccess) container;
        int[] l = layout;
        access.horizon$leftPos(l[0]);
        access.horizon$topPos(l[1]);
        access.horizon$imageWidth(l[2]);
        access.horizon$imageHeight(l[3]);
        var slots = container.getMenu().slots;
        access.horizon$hovered(l[4] >= 0 && l[4] < slots.size() ? container.getMenu().getSlot(l[4]) : null);
    }

    // --- Recording

    private static byte[][] empty() {
        byte[][] none = new byte[SEGMENTS][];
        for (int i = 0; i < SEGMENTS; i++) none[i] = new byte[] {0, 0, 0, 0};
        return none;
    }

    /** Whether a step's vanilla method runs in Java: only a loaded mod's detected patch of it does. */
    static boolean routes(Screen shell, int leaf) {
        return false;
    }

    /** The recording of one frame: a gap after each step holds what mods drew there, a step's own part what Java drew of it. */
    private static final class Steps {
        final Recorder recorder = new Recorder();
        final byte[][] segments = empty();
        int gap;

        void put(int index) throws IOException {
            byte[] frame = recorder.cut();
            segments[index] = Recorder.concat(segments[index], frame);
        }

        /** Closes the open gap and opens the one after `leaf`. */
        void step(int leaf) throws IOException {
            put(gap);
            gap = leaf * 2;
        }
    }

    /** `extractRenderStateWithTooltipAndSubtitles` as NeoForge's `extractScreen` runs it, cut at native's draw steps; null when a mod's overlay failed. */
    static byte[][] record(Minecraft minecraft, int mx, int my, float partial) {
        try {
            var shell = current;
            var steps = new Steps();
            var graphics = new GuiGraphicsExtractor(minecraft, steps.recorder, mx, my);
            apply(shell);
            var bus = NeoForge.EVENT_BUS;
            routed = 0;
            if (bus.post(new ScreenEvent.Render.Pre(shell, graphics, mx, my, partial)).isCanceled()) {
                if (!cancelledLogged) System.err.println("Horizon: a mod cancelled Render.Pre of a native screen; native still draws it");
                cancelledLogged = true;
                return empty();
            }
            steps.step(ShellLeaves.BACKGROUND);
            graphics.nextStratum();
            bus.post(new ScreenEvent.Render.Background(shell, graphics, mx, my, partial));
            steps.step(ShellLeaves.EFFECTS);
            graphics.nextStratum();
            steps.step(ShellLeaves.WIDGETS);
            for (var renderable : new ArrayList<>(shell.renderables)) if (!vanilla.contains(renderable)) renderable.extractRenderState(graphics, mx, my, partial);
            steps.put(ShellLeaves.WIDGETS * 2 + 1);
            steps.step(ShellLeaves.LABELS);
            steps.step(ShellLeaves.HIGHLIGHT_BACK);
            steps.step(ShellLeaves.SLOTS);
            steps.step(ShellLeaves.HIGHLIGHT_FRONT);
            bus.post(new ScreenEvent.Render.Foreground(shell, graphics, mx, my, partial));
            steps.step(ShellLeaves.EXTRA);
            steps.step(ShellLeaves.RECIPE_BOOK);
            steps.step(ShellLeaves.CARRIED);
            steps.step(ShellLeaves.TOOLTIP);
            steps.step(ShellLeaves.BOOK_TOOLTIP);
            steps.step(ShellLeaves.DEFERRED);
            graphics.extractDeferredElements(mx, my, partial);
            bus.post(new ScreenEvent.Render.Post(shell, graphics, mx, my, partial));
            steps.put(steps.gap);
            return steps.segments;
        } catch (Throwable t) {
            System.err.println("Horizon: a mod's screen overlay failed");
            t.printStackTrace();
            return null;
        }
    }

    /** The HUD's rule: a recording differing from the last starts the animation, two alike end it only if real time passed. Answers whether it changed. */
    static boolean noteRecorded(byte[][] segments, int routedBits, float realtimeDeltaTicks) {
        boolean identical = routedBits == lastRouted && java.util.Arrays.deepEquals(segments, last);
        lastRouted = routedBits;
        if (!identical) animating = true;
        else if (HudAnimation.ends(true, realtimeDeltaTicks)) animating = false;
        last = segments;
        return !identical;
    }

    /** The layout and the answers native computed for the next input, from its request. */
    static void layout(int[] values) {
        layout = values;
    }

    // --- Who takes an input

    private static List<GuiEventListener> mods(Screen screen) {
        var found = new ArrayList<GuiEventListener>();
        for (var child : screen.children()) if (!vanilla.contains(child)) found.add(child);
        return found;
    }

    private static boolean isMod(GuiEventListener widget) {
        return widget != null && !vanilla.contains(widget);
    }

    private static boolean native_() {
        owner = nativeHandles ? NATIVE : NONE;
        return nativeHandles;
    }

    private static boolean widget() {
        owner = WIDGET;
        return true;
    }

    public static boolean clicked(Screen screen, MouseButtonEvent event, boolean doubleClick) {
        if (nativeWidget) {
            owner = NATIVE;
            return true;
        }
        for (var widget : mods(screen)) {
            if (!widget.mouseClicked(event, doubleClick)) continue;
            if (widget.shouldTakeFocusAfterInteraction()) {
                screen.setFocused(widget);
                if (event.button() == 1) screen.setDragging(true);
            }
            return widget();
        }
        return native_();
    }

    public static boolean released(Screen screen, MouseButtonEvent event) {
        boolean dragging = screen.isDragging();
        screen.setDragging(false);
        var focused = screen.getFocused();
        if (dragging && event.button() == 1 && isMod(focused) && focused.mouseReleased(event)) return widget();
        return native_();
    }

    public static boolean dragged(Screen screen, MouseButtonEvent event, double dx, double dy) {
        var focused = screen.getFocused();
        if (screen.isDragging() && event.button() == 1 && isMod(focused) && focused.mouseDragged(event, dx, dy)) return widget();
        return native_();
    }

    public static boolean scrolled(Screen screen, double x, double y, double sx, double sy) {
        for (var widget : mods(screen)) {
            if (widget.isMouseOver(x, y) && widget.mouseScrolled(x, y, sx, sy)) return widget();
        }
        return native_();
    }

    public static boolean key(Screen screen, KeyEvent event) {
        if (event.isEscape()) return native_();
        var focused = screen.getFocused();
        if (isMod(focused) && focused.keyPressed(event)) return widget();
        return native_();
    }

    public static boolean typed(Screen screen, CharacterEvent event) {
        var focused = screen.getFocused();
        if (isMod(focused) && focused.charTyped(event)) return widget();
        return native_();
    }

    /** `bool recorded`, when it is the segments (a status counts them first, a `ShellReply` does not), then `int routed`. */
    static void write(java.io.DataOutputStream out, byte[][] segments, boolean changed, boolean counted) throws IOException {
        out.writeBoolean(changed);
        if (changed) {
            if (counted) out.writeInt(SEGMENTS);
            for (byte[] segment : segments) {
                out.writeInt(segment.length);
                out.write(segment);
            }
        }
        out.writeInt(routed);
    }

    /** `ShellReply`: who owns the input, the fresh recording, whether it animates. */
    static byte[] reply(int answer, byte[][] segments, boolean changed) throws IOException {
        var bytes = new java.io.ByteArrayOutputStream();
        var out = new java.io.DataOutputStream(bytes);
        out.writeByte(answer);
        write(out, segments, changed, false);
        out.writeBoolean(animating);
        return bytes.toByteArray();
    }
}
