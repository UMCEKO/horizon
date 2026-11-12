package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.TreeSet;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.neoforged.bus.ListenerList;
import net.neoforged.bus.api.Event;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.ICancellableEvent;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.common.CommonHooks;
import net.neoforged.neoforge.common.NeoForge;

/** NeoForge events Rust posts where its hook fires (`game/java/java_listener.rs`): which ones a mod listens to, and the post ops. */
public final class EventPosts {
    private EventPosts() {}

    /** A client event's adapter: reads its fields after the name, posts it, and answers what Rust writes back. */
    interface ClientPost {
        byte[] post(DataInputStream in) throws IOException;
    }

    /** What a server event's listeners left: whether one cancelled it, and the fields its site reads back. */
    record Posted(boolean canceled, byte[] result) {
        static final Posted NONE = new Posted(false, new byte[0]);
    }

    /** A server event posted on the acting player's view: reads its fields after the view and posts it. */
    interface PlayerPost {
        Posted post(GameBridge.Call call) throws Exception;
    }

    /** A server event posted on a level with no acting player: reads its fields after the name and posts it. */
    interface LevelPost {
        Posted post(ServerLevel level, Crossing.In in) throws Exception;
    }

    /** The server events Rust posts through `post_event`, each with the acting player's view filled. */
    private static final Map<String, PlayerPost> PLAYER = Map.of("BreakBlockEvent", EventPosts::breakBlock, "AttackEntityEvent", EventPosts::attackEntity);

    /** The server events Rust posts through `post_level_event`. */
    private static final Map<String, LevelPost> LEVEL = Map.of();

    /** The client events Rust posts through `client_post_event`. */
    private static final Map<String, ClientPost> CLIENT = Map.of("PlaySoundEvent", NativeSounds::ask);

    /** The last census: each listened name with its listeners' owners. */
    private static volatile Map<String, List<String>> listened = Map.of();

    /** The packages NeoForge's events live in; a name is `Outer` or `Outer.Inner` within one of them. */
    private static final String[] PACKAGES = {
        "net.neoforged.neoforge.event", "net.neoforged.neoforge.event.entity", "net.neoforged.neoforge.event.entity.living",
        "net.neoforged.neoforge.event.entity.player", "net.neoforged.neoforge.event.entity.item", "net.neoforged.neoforge.event.level",
        "net.neoforged.neoforge.event.level.block", "net.neoforged.neoforge.event.tick", "net.neoforged.neoforge.event.server",
        "net.neoforged.neoforge.event.enchanting", "net.neoforged.neoforge.event.brewing", "net.neoforged.neoforge.event.village",
        "net.neoforged.neoforge.client.event", "net.neoforged.neoforge.client.event.sound", "net.neoforged.neoforge.client.event.lifecycle",
        "net.neoforged.neoforge.network.event", "net.neoforged.neoforge.client.network.event", "net.neoforged.neoforge.registries",
        "net.neoforged.neoforge.registries.datamaps", "net.neoforged.neoforge.server.permission.events",
    };

    /** A listened event: its listeners' owners, a bit per `EventPriority` ordinal with one, and whether one hears it cancelled. */
    record Heard(List<String> owners, int phases, boolean receivesCanceled) {}

    /** `bool loud`, `int n` x (name, `bool sited`), answered with `int m` x (name, `byte phases`, `bool receives canceled`) of the events a loaded mod listens to; `loud` logs each. */
    static byte[] census(byte[] input) throws IOException {
        CompiledEvents.freeze();
        var in = Crossing.In.of(input);
        boolean loud = in.readBoolean();
        int n = in.readInt();
        boolean debug = System.getenv("HORIZON_JAVA_DEBUG") != null;
        var found = new TreeMap<String, Heard>();
        for (int i = 0; i < n; i++) {
            String name = in.text();
            boolean sited = in.readBoolean();
            Class<?> event = resolve(name);
            if (event == null) continue;
            Heard heard = heard(NeoForge.EVENT_BUS, event);
            if (heard.owners().isEmpty()) {
                if (loud && debug) System.err.println("Horizon: server event " + event.getName() + " not listened");
                continue;
            }
            found.put(name, heard);
            if (!loud) continue;
            String by = String.join(", ", heard.owners());
            System.err.println("Horizon: server event " + event.getName() + " listened by " + by);
            if (!sited) System.err.println("Horizon: WARNING: server event " + event.getName() + " is listened (" + by + ") but Horizon has no site for it: mods will not hear it");
        }
        var owners = new TreeMap<String, List<String>>();
        found.forEach((name, heard) -> owners.put(name, heard.owners()));
        listened = Collections.unmodifiableMap(owners);
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(found.size());
        for (var entry : found.entrySet()) {
            byte[] utf = entry.getKey().getBytes(StandardCharsets.UTF_8);
            out.writeInt(utf.length);
            out.write(utf);
            out.writeByte(entry.getValue().phases());
            out.writeBoolean(entry.getValue().receivesCanceled());
        }
        return bytes.toByteArray();
    }

    /** `event`'s listeners on the bus beyond Horizon's own, by phase: whom they belong to, their phases, whether one hears a cancelled event. */
    static Heard heard(IEventBus bus, Class<?> event) {
        if (!Event.class.isAssignableFrom(event)) return new Heard(List.of(), 0, false);
        boolean cancellable = ICancellableEvent.class.isAssignableFrom(event);
        var owners = new TreeSet<String>();
        int phases = 0;
        boolean receives = false;
        try {
            for (EventPriority phase : EventPriority.values()) {
                for (var listener : CompiledEvents.phaseListeners(bus, event, phase)) {
                    String owner;
                    try {
                        owner = Listeners.ownerOf(listener).getName();
                    } catch (ReflectiveOperationException | RuntimeException e) {
                        owner = listener.getClass().getName();
                    }
                    if (horizons(owner)) continue;
                    owners.add(owner);
                    phases |= 1 << phase.ordinal();
                    receives |= cancellable && CompiledEvents.receivesCanceled(listener);
                }
            }
        } catch (Throwable e) {
            System.err.println("Horizon: the listeners of " + event.getName() + " can't be read by phase: " + e);
            List<String> all = owners(bus, event);
            return new Heard(all, all.isEmpty() ? 0 : 1 << EventPriority.NORMAL.ordinal(), false);
        }
        return new Heard(List.copyOf(owners), phases, receives);
    }

    /** The last census's listened names, each with its listeners' owners. */
    public static Map<String, List<String>> lastCensus() {
        return listened;
    }

    /** Whether the bus holds a listener of `event` (its own or a supertype's) beyond Horizon's own. */
    static boolean listened(IEventBus bus, Class<?> event) {
        return !owners(bus, event).isEmpty();
    }

    private static Class<?> resolve(String name) {
        String nested = name.replace('.', '$');
        for (String pkg : PACKAGES) {
            try {
                return Class.forName(pkg + "." + nested, false, NeoForge.class.getClassLoader());
            } catch (ClassNotFoundException | LinkageError ignored) {
                // Not in this package, or not on this side.
            }
        }
        System.err.println("Horizon: no NeoForge event class " + name + " on this side; its hooks do not post to Java");
        return null;
    }

    /** Whether a listener's owner is Horizon's own: its listeners never make an event listened, the test mod's do. */
    static boolean horizons(String owner) {
        return owner.startsWith("dev.horizonlab.javamods.") && !owner.startsWith("dev.horizonlab.javamods.testmod.");
    }

    /** The owners of `event`'s listeners on the bus (its own and its supertypes'), sorted and distinct, Horizon's left out. */
    static List<String> owners(IEventBus bus, Class<?> event) {
        if (!Event.class.isAssignableFrom(event)) return List.of();
        var owners = new TreeSet<String>();
        try {
            var list = net.neoforged.bus.EventBus.class.getDeclaredMethod("getListenerList", Class.class);
            list.setAccessible(true);
            for (var listener : ((ListenerList) list.invoke(bus, event)).getListeners()) {
                String owner;
                try {
                    owner = Listeners.ownerOf(listener).getName();
                } catch (ReflectiveOperationException | RuntimeException e) {
                    owner = listener.getClass().getName();
                }
                if (!horizons(owner)) owners.add(owner);
            }
        } catch (ReflectiveOperationException | RuntimeException e) {
            System.err.println("Horizon: the listeners of " + event.getName() + " can't be read: " + e);
            return List.of();
        }
        return List.copyOf(owners);
    }

    /** `post_event`: the event's name, whether it arrives cancelled, then the acting player's view and its fields; the reply is the posted result, then the use reply's tail. */
    static void server(Crossing.In in, Crossing.Out out) throws Exception {
        String name = in.text();
        boolean arrivesCanceled = in.readBoolean();
        PlayerPost post = PLAYER.get(name);
        if (post == null) throw new IllegalArgumentException("no server post for " + name);
        var call = GameBridge.fill(in, false);
        // A view keeps no use between calls, as in `GameBridge.use`.
        Using.clear(call.player());
        write(out, posted(name, arrivesCanceled, () -> post.post(call)));
        GameBridge.writeUseTail(call.player(), out);
    }

    /** `post_level_event`: the level, the event's name, whether it arrives cancelled, and its fields; the reply is the posted result, then the effects. */
    static void level(Crossing.In in, Crossing.Out out) throws Exception {
        var level = GameBridge.levelOf(in);
        String name = in.text();
        boolean arrivesCanceled = in.readBoolean();
        LevelPost post = LEVEL.get(name);
        if (post == null) throw new IllegalArgumentException("no level post for " + name);
        write(out, posted(name, arrivesCanceled, () -> post.post(level, in)));
        GameBridge.writeEffects(out);
    }

    private interface Step {
        Posted run() throws Exception;
    }

    /** The event classes posts resolved by name. */
    private static final Map<String, Class<?>> CLASSES = new java.util.concurrent.ConcurrentHashMap<>();

    /** Runs a post through `GameBridge.safely`: a listener that throws leaves the event as it was; one a native mod cancelled first starts cancelled. */
    private static Posted posted(String name, boolean arrivesCanceled, Step step) {
        Posted[] posted = {Posted.NONE};
        Runnable run = () -> GameBridge.safely(name, () -> {
            try {
                posted[0] = step.run();
            } catch (Exception e) {
                throw new IllegalStateException(e);
            }
        });
        if (!arrivesCanceled) {
            run.run();
        } else {
            Class<?> type = CLASSES.computeIfAbsent(name, EventPosts::resolve);
            // Only compiled dispatch can start the site's own event cancelled; without it the cancel stands unheard.
            if (type == null || !CompiledEvents.carry(type, run)) return new Posted(true, Posted.NONE.result());
        }
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: server event " + name + " canceled=" + posted[0].canceled() + " result=" + posted[0].result().length + " bytes");
        return posted[0];
    }

    private static void write(Crossing.Out out, Posted posted) throws IOException {
        out.writeBoolean(posted.canceled());
        out.writeInt(posted.result().length);
        out.write(posted.result());
    }

    private static Posted breakBlock(GameBridge.Call call) {
        var pos = BlockPos.of(call.in().readLong());
        var player = call.player();
        var level = call.level();
        return new Posted(CommonHooks.fireBlockBreak(level, GameBridge.type(player), player, pos, level.getBlockState(pos)).isCanceled(), Posted.NONE.result());
    }

    private static Posted attackEntity(GameBridge.Call call) {
        var target = call.level().getEntity(call.in().readInt());
        return new Posted(target != null && !CommonHooks.onPlayerAttackTarget(call.player(), target), Posted.NONE.result());
    }

    /** `client_post_event`: the event's name, then its fields, posted on the Java client's thread. */
    static byte[] client(DataInputStream in) throws IOException {
        String name = new String(in.readNBytes(in.readInt()), StandardCharsets.UTF_8);
        ClientPost post = CLIENT.get(name);
        if (post == null) throw new IllegalArgumentException("no client post for " + name);
        return post.post(in);
    }
}
