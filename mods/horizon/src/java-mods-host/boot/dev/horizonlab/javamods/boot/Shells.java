package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.mixin.CreativeShellAccess;
import dev.horizonlab.javamods.boot.mixin.RecipeBookComponentAccess;
import dev.horizonlab.javamods.boot.mixin.RecipeBookScreenAccess;
import dev.horizonlab.javamods.boot.mixin.ShellAccess;
import dev.horizonlab.javamods.boot.mixin.ShellInputAccess;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashMap;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.events.GuiEventListener;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.client.gui.screens.inventory.AbstractRecipeBookScreen;
import net.minecraft.client.gui.screens.inventory.CreativeModeInventoryScreen;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
import net.minecraft.client.input.CharacterEvent;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ServerboundContainerButtonClickPacket;
import net.minecraft.network.protocol.game.ServerboundContainerClickPacket;
import net.minecraft.network.protocol.game.ServerboundContainerClosePacket;
import net.minecraft.network.protocol.game.ServerboundContainerSlotStateChangedPacket;
import net.minecraft.network.protocol.game.ServerboundPlaceRecipePacket;
import net.minecraft.network.protocol.game.ServerboundRenameItemPacket;
import net.minecraft.network.protocol.game.ServerboundSelectBundleItemPacket;
import net.minecraft.network.protocol.game.ServerboundSelectTradePacket;
import net.minecraft.network.protocol.game.ServerboundSetBeaconPacket;
import net.minecraft.network.protocol.game.ServerboundSetCreativeModeSlotPacket;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.SimpleContainer;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.item.ItemStack;
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
    /** What the open shell stands for: 1 the survival inventory, 2 the creative screen, 3 the screen of a container menu the server opened. */
    static int kind;
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
    /** The creative screen as native shows it: the selected tab's id, what is hovered (0 none, 1 grid, 2 hotbar, 3 inventory tab, 4 destroy slot) and where. */
    private static String creativeTab = "";
    private static int creativeHover, creativeIndex;
    private static ItemStack creativeStack = ItemStack.EMPTY;
    private static int written = -1;
    private static final Slot DESTROY = new Slot(new SimpleContainer(1), 0, 173, 112);
    private static final Set<String> HEARD = new java.util.HashSet<>();
    private static boolean cancelledLogged, notCreative, unknownTab;
    /** A packet native forwarded is being handled: the screen it opens is vanilla's own for the menu. */
    public static boolean forwarding;
    /** A click native made is being replayed; its packet is native's already. */
    public static boolean echo;
    /** Whether native's recipe book is showing, as the shell's component says. */
    private static boolean bookVisible;

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
    public static boolean isMods(Class<?> owner) {
        String module = owner.getModule().getName();
        if (module != null) return !(module.equals("neoforge") || module.equals("minecraft") || module.equals(Shells.class.getModule().getName()) || module.startsWith("java.") || module.startsWith("jdk."));
        String name = owner.getName();
        return !(name.startsWith("java.") || name.startsWith("jdk.") || name.startsWith("net.neoforged.") || name.startsWith("net.minecraft.") || name.startsWith("dev.horizonlab.javamods."));
    }

    /** Whether a mod listens to a screen event on the bus, read as the moment vanilla would post `Opening`: JEI's listeners exist only once its runtime started. */
    static boolean wanted(net.neoforged.bus.api.IEventBus bus) {
        return Listeners.modListens(bus, EVENTS, HEARD, "screen events");
    }

    public static boolean wantedNow() {
        return wanted(NeoForge.EVENT_BUS);
    }

    // --- Opening and closing

    /** The vanilla screen of a forwarded menu is the shell, as `MenuScreens.create` built it: `Gui.setScreen` goes on to fire `Opening` and run `init`. */
    public static void adopt(Screen screen) {
        current = screen;
        kind = 3;
        opened();
        last = new byte[SEGMENTS][];
        lastRouted = 0;
        animating = false;
        written = -1;
    }

    /** The forwarded menu's screen is not a shell: the displayed shell, if any, is given up before the stand-in replaces it. */
    public static void retire(Minecraft minecraft) {
        if (!is(minecraft.gui.screen())) return;
        current = null;
        kind = 0;
    }

    /** A mod's own screen class opened for a vanilla menu type is a Java screen over native's menu: the displayed shell, if any, is given up to it. */
    public static void modScreen(Minecraft minecraft) {
        retire(minecraft);
    }

    /** `Input::ShellBook`: whether native's recipe book is showing. */
    static void book(boolean visible) {
        bookVisible = visible;
    }

    /** Native opened one of its vanilla screens: with a mod listening, the Java client opens the real class as a shell. */
    static void open(int which, Minecraft minecraft) {
        if ((which != 1 && which != 2) || !wanted(NeoForge.EVENT_BUS)) return;
        if (is(minecraft.gui.screen()) && kind == which) return;
        if (current != null && kind != which) {
            if (is(minecraft.gui.screen())) minecraft.gui.setScreen(null);
            release(minecraft);
            current = null;
        }
        Screen shell;
        try {
            if (which == 2) {
                // `init` selects the static tab: start from the default so the picker menu keeps its own slots.
                CreativeShellAccess.horizon$selectedTab(CreativeModeTabs.getDefaultTab());
                shell = new CreativeModeInventoryScreen(minecraft.player, minecraft.player.connection.enabledFeatures(), minecraft.options.operatorItemsTab().get());
            } else {
                shell = new InventoryScreen(minecraft.player);
            }
        } catch (Throwable t) {
            System.err.println("Horizon: the Java client's screen shell failed; native's screen stays alone");
            t.printStackTrace();
            return;
        }
        current = shell;
        kind = which;
        opened();
        last = new byte[SEGMENTS][];
        lastRouted = 0;
        animating = false;
        written = -1;
        minecraft.gui.setScreen(shell);
        if (which == 2 && minecraft.gui.screen() instanceof InventoryScreen) {
            if (!notCreative) System.err.println("Horizon: the Java client is not in creative; no creative shell");
            notCreative = true;
            minecraft.gui.setScreen(null);
        }
        if (minecraft.gui.screen() != shell) {
            release(minecraft);
            current = null;
        }
    }

    /** The creative screen's `init` listens to the inventory menu and its `removed`, which a shell never runs, stops that. */
    private static void release(Minecraft minecraft) {
        if (current instanceof CreativeModeInventoryScreen creative && minecraft.player != null) {
            var listener = ((CreativeShellAccess) creative).horizon$listener();
            if (listener != null) minecraft.player.inventoryMenu.removeSlotListener(listener);
        }
    }

    /** Native closed its screen. A mod's screen over the shell keeps it: the mod sets the same object back. */
    static void close(Minecraft minecraft) {
        if (!is(minecraft.gui.screen())) return;
        // A menu's shell: native already told the server, vanilla's `clientSideCloseContainer` brings the inventory menu back.
        if (kind == 3 && minecraft.player != null) minecraft.player.clientSideCloseContainer();
        else minecraft.gui.setScreen(null);
        release(minecraft);
        current = null;
        kind = 0;
    }

    // --- What mods do through a shell reaches native

    /** The menu native forwarded that the Java client opened: its packets are native's to perform. */
    private static AbstractContainerMenu forwardedMenu;
    /** Mods' packets for native's menus, in sending order, until a reply or a status carries them. */
    private static final List<byte[]> OUTBOX = new ArrayList<>();
    private static final List<Class<? extends Event>> DRAGS = List.of(ScreenEvent.MouseDragged.Pre.class, ScreenEvent.MouseDragged.Post.class);
    private static final Set<String> DRAGS_HEARD = new java.util.HashSet<>();
    /** A mod listens to drags, as read when the open shell opened: moves with a button down are inputs then. */
    static boolean drags;
    private static final Map<Class<?>, Boolean> STATE_READERS = new HashMap<>();
    /** The input state native last wrote: quick crafting, its type and button, skip next release, then the quick craft slots, sorted. */
    private static int[] wrote;

    private static void opened() {
        drags = Listeners.modListens(NeoForge.EVENT_BUS, DRAGS, DRAGS_HEARD, "drag events");
        wrote = null;
    }

    /** The menu of a packet native forwarded got its screen, a shell's or a mod's. */
    public static void forwarded(AbstractContainerMenu menu) {
        forwardedMenu = menu;
    }

    /** The inventory menu, or the forwarded menu while it is the player's: native owns them. */
    static boolean nativeMenu(int id) {
        if (id == 0) return true;
        var player = Minecraft.getInstance().player;
        return forwardedMenu != null && forwardedMenu.containerId == id && player != null && player.containerMenu == forwardedMenu;
    }

    /** `ClientBridge.send`: a packet acting on a native menu goes to the outbox instead of the Java server, which doesn't have the menu. */
    static boolean toNative(Packet<?> packet) {
        var player = Minecraft.getInstance().player;
        int open = player == null ? -1 : player.containerMenu.containerId;
        int id;
        if (packet instanceof ServerboundContainerClickPacket p) id = p.containerId();
        else if (packet instanceof ServerboundContainerClosePacket p) id = p.getContainerId();
        else if (packet instanceof ServerboundContainerButtonClickPacket p) id = p.containerId();
        else if (packet instanceof ServerboundContainerSlotStateChangedPacket p) id = p.containerId();
        else if (packet instanceof ServerboundPlaceRecipePacket p) id = p.containerId();
        else if (packet instanceof ServerboundSetCreativeModeSlotPacket) id = 0;
        else if (packet instanceof ServerboundSelectBundleItemPacket || packet instanceof ServerboundRenameItemPacket || packet instanceof ServerboundSelectTradePacket
                || packet instanceof ServerboundSetBeaconPacket) id = open;
        else return false;
        if (!nativeMenu(id)) return false;
        try {
            OUTBOX.add(Wire.toNative(packet));
        } catch (Throwable t) {
            System.err.println("Horizon: a mod's " + packet.getClass().getSimpleName() + " for native's menu didn't encode; dropped");
            t.printStackTrace();
        }
        return true;
    }

    /** `int n` and the outbox's packets, which it then forgets. */
    static void writeOutbox(DataOutputStream out) throws IOException {
        out.writeInt(OUTBOX.size());
        for (byte[] packet : OUTBOX) {
            out.writeInt(packet.length);
            out.write(packet);
        }
        OUTBOX.clear();
    }

    /** Whether a loaded mod reads or writes the open shell's input fields: one of its interfaces (an accessor or invoker mixin adds one) on the screen's vanilla classes. */
    static boolean stateWanted() {
        if (!(current instanceof AbstractContainerScreen<?>) || kind == 2 || !is(Minecraft.getInstance().gui.screen())) return false;
        return STATE_READERS.computeIfAbsent(current.getClass(), type -> {
            for (Class<?> c = type; c != null && c != Screen.class; c = c.getSuperclass()) {
                for (var face : c.getInterfaces()) if (isMods(face)) return true;
            }
            return false;
        });
    }

    /** `Input::ShellState`: native's quick craft and release state, written into the shell's own fields for mods to read. */
    static void state(DataInputStream in) throws IOException {
        boolean quick = in.readBoolean();
        int type = in.readInt(), button = in.readInt();
        int[] slots = new int[in.readInt()];
        for (int i = 0; i < slots.length; i++) slots[i] = in.readInt();
        boolean skip = in.readBoolean();
        Arrays.sort(slots);
        int[] state = new int[4 + slots.length];
        state[0] = quick ? 1 : 0;
        state[1] = type;
        state[2] = button;
        state[3] = skip ? 1 : 0;
        System.arraycopy(slots, 0, state, 4, slots.length);
        wrote = state;
        if (current instanceof AbstractContainerScreen<?> screen) writeState(screen, state);
    }

    private static void writeState(AbstractContainerScreen<?> screen, int[] state) {
        var access = (ShellInputAccess) screen;
        access.horizon$setQuickCrafting(state[0] != 0);
        access.horizon$setQuickCraftingType(state[1]);
        access.horizon$setQuickCraftingButton(state[2]);
        access.horizon$setSkipNextRelease(state[3] != 0);
        var set = access.horizon$quickCraftSlots();
        set.clear();
        var slots = screen.getMenu().slots;
        for (int i = 4; i < state.length; i++) if (state[i] >= 0 && state[i] < slots.size()) set.add(slots.get(state[i]));
    }

    private static int[] readState(AbstractContainerScreen<?> screen) {
        var access = (ShellInputAccess) screen;
        int[] slots = access.horizon$quickCraftSlots().stream().mapToInt(slot -> slot.index).sorted().toArray();
        int[] state = new int[4 + slots.length];
        state[0] = access.horizon$quickCrafting() ? 1 : 0;
        state[1] = access.horizon$quickCraftingType();
        state[2] = access.horizon$quickCraftingButton();
        state[3] = access.horizon$skipNextRelease() ? 1 : 0;
        System.arraycopy(slots, 0, state, 4, slots.length);
        return state;
    }

    /** `bool changed`, then the state block when a mod changed what native wrote. */
    private static void writeStateChange(DataOutputStream out) throws IOException {
        int[] now = wrote != null && current instanceof AbstractContainerScreen<?> screen && stateWanted() ? readState(screen) : null;
        boolean changed = now != null && !Arrays.equals(now, wrote);
        out.writeBoolean(changed);
        if (!changed) return;
        wrote = now;
        out.writeBoolean(now[0] != 0);
        out.writeInt(now[1]);
        out.writeInt(now[2]);
        out.writeInt(now.length - 4);
        for (int i = 4; i < now.length; i++) out.writeInt(now[i]);
        out.writeBoolean(now[3] != 0);
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
        if (screen instanceof CreativeModeInventoryScreen creative) {
            applyCreative(creative);
            return;
        }
        var access = (ShellAccess) container;
        if (screen instanceof AbstractRecipeBookScreen<?> book) ((RecipeBookComponentAccess) ((RecipeBookScreenAccess) book).horizon$component()).horizon$visible(bookVisible);
        int[] l = layout;
        access.horizon$leftPos(l[0]);
        access.horizon$topPos(l[1]);
        access.horizon$imageWidth(l[2]);
        access.horizon$imageHeight(l[3]);
        var slots = container.getMenu().slots;
        access.horizon$hovered(l[4] >= 0 && l[4] < slots.size() ? container.getMenu().getSlot(l[4]) : null);
    }

    /** The creative screen's selected tab and hovered slot, written where `selectTab` and `renderSlot` would leave them: a field write and one slot, no vanilla logic. */
    private static void applyCreative(CreativeModeInventoryScreen screen) {
        var access = (ShellAccess) screen;
        int[] l = layout;
        access.horizon$leftPos(l[0]);
        access.horizon$topPos(l[1]);
        access.horizon$imageWidth(l[2]);
        access.horizon$imageHeight(l[3]);
        if (!creativeTab.isEmpty()) {
            var id = Identifier.tryParse(creativeTab);
            var tab = id == null ? null : BuiltInRegistries.CREATIVE_MODE_TAB.getValue(id);
            if (tab != null) CreativeShellAccess.horizon$selectedTab(tab);
            else if (!unknownTab) {
                unknownTab = true;
                System.err.println("Horizon: the creative tab " + creativeTab + " is not in the Java client's registry");
            }
        }
        var slots = screen.getMenu().slots;
        if (written >= 0 && written < slots.size()) slots.get(written).container.setItem(written, ItemStack.EMPTY);
        written = -1;
        Slot hovered = null;
        switch (creativeHover) {
            case 1 -> {
                if (creativeIndex >= 0 && creativeIndex < 45 && creativeIndex < slots.size()) {
                    hovered = slots.get(creativeIndex);
                    hovered.container.setItem(creativeIndex, creativeStack);
                    written = creativeIndex;
                }
            }
            case 2 -> hovered = creativeIndex >= 0 && 45 + creativeIndex < slots.size() ? slots.get(45 + creativeIndex) : null;
            case 3 -> {
                var inventory = Minecraft.getInstance().player.inventoryMenu.slots;
                hovered = creativeIndex >= 0 && creativeIndex < inventory.size() ? inventory.get(creativeIndex) : null;
            }
            case 4 -> hovered = DESTROY;
            default -> {}
        }
        access.horizon$hovered(hovered);
    }

    /** `Input::ShellCreative`: native's selected tab and hovered slot. */
    static void creative(String tab, int hover, int index, byte[] stack) throws IOException {
        creativeTab = tab;
        creativeHover = hover;
        creativeIndex = index;
        creativeStack = stack.length == 0 ? ItemStack.EMPTY : GameBridge.decode(stack);
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

    /** `ShellReply`: who owns the input, the fresh recording, whether it animates, the outbox, a mod's change to the input state. */
    static byte[] reply(int answer, byte[][] segments, boolean changed) throws IOException {
        var bytes = new java.io.ByteArrayOutputStream();
        var out = new java.io.DataOutputStream(bytes);
        out.writeByte(answer);
        write(out, segments, changed, false);
        out.writeBoolean(animating);
        writeOutbox(out);
        writeStateChange(out);
        return bytes.toByteArray();
    }
}
