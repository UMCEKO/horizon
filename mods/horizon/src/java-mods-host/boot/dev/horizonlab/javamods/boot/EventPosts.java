package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Map;
import net.neoforged.bus.ListenerList;
import net.neoforged.bus.api.Event;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.common.NeoForge;

/** NeoForge events Rust posts where its hook fires (`game/java/java_listener.rs`): which ones a mod listens to, and one post op per side. */
final class EventPosts {
    private EventPosts() {}

    /** A client event's adapter: reads its fields after the name, posts it, and answers what Rust writes back. */
    interface ClientPost {
        byte[] post(DataInputStream in) throws IOException;
    }

    /** The server events Rust posts through `post_event`, by the `GameBridge.use` kind that posts each with the acting player's view. */
    private static final Map<String, Integer> SERVER = Map.of("BreakBlockEvent", 5, "AttackEntityEvent", 6);

    /** The client events Rust posts through `client_post_event`. */
    private static final Map<String, ClientPost> CLIENT = Map.of("PlaySoundEvent", NativeSounds::ask);

    /** Listeners Horizon itself put on `NeoForge.EVENT_BUS`, by event class: they do not make an event listened. */
    private static final Map<Class<?>, Integer> OWN = new HashMap<>();

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

    /** Counts one listener Horizon adds to `NeoForge.EVENT_BUS` for `event`. */
    static synchronized void own(Class<?> event) {
        OWN.merge(event, 1, Integer::sum);
    }

    /** `int n` names, answered with `int m` of them a loaded mod listens to. */
    static byte[] census(byte[] input) throws IOException {
        var in = Crossing.In.of(input);
        int n = in.readInt();
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        var listened = new java.util.ArrayList<String>();
        for (int i = 0; i < n; i++) {
            String name = in.text();
            Class<?> event = resolve(name);
            if (event != null && listened(NeoForge.EVENT_BUS, event)) listened.add(name);
        }
        out.writeInt(listened.size());
        for (String name : listened) {
            byte[] utf = name.getBytes(StandardCharsets.UTF_8);
            out.writeInt(utf.length);
            out.write(utf);
        }
        return bytes.toByteArray();
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

    /** Whether the bus holds a listener of `event` (its own or a supertype's) beyond Horizon's own. */
    static boolean listened(IEventBus bus, Class<?> event) {
        if (!Event.class.isAssignableFrom(event)) return false;
        try {
            var list = net.neoforged.bus.EventBus.class.getDeclaredMethod("getListenerList", Class.class);
            list.setAccessible(true);
            int own;
            synchronized (EventPosts.class) {
                own = OWN.getOrDefault(event, 0);
            }
            return ((ListenerList) list.invoke(bus, event)).getListeners().length > own;
        } catch (ReflectiveOperationException | RuntimeException e) {
            System.err.println("Horizon: the listeners of " + event.getName() + " can't be read: " + e);
            return false;
        }
    }

    /** `post_event`: the event's name, then the acting player's view and its fields, as `GameBridge.use` reads them; the reply's first field says it was cancelled. */
    static void server(Crossing.In in, Crossing.Out out) throws Exception {
        String name = in.text();
        Integer kind = SERVER.get(name);
        if (kind == null) throw new IllegalArgumentException("no server post for " + name);
        GameBridge.use(in, out, kind);
    }

    /** `client_post_event`: the event's name, then its fields, posted on the Java client's thread. */
    static byte[] client(DataInputStream in) throws IOException {
        String name = new String(in.readNBytes(in.readInt()), StandardCharsets.UTF_8);
        ClientPost post = CLIENT.get(name);
        if (post == null) throw new IllegalArgumentException("no client post for " + name);
        return post.post(in);
    }
}
