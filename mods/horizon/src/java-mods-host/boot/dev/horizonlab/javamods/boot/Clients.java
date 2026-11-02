package dev.horizonlab.javamods.boot;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import net.minecraft.network.protocol.Packet;

/** The packets between a player's server view and the Java client of the window they play in: the same JVM, so the network is two queues. */
public final class Clients {
    private Clients() {}

    private static final Map<UUID, ArrayDeque<Packet<?>>> TO_CLIENT = new HashMap<>();
    private static final Map<UUID, ArrayDeque<Packet<?>>> TO_SERVER = new HashMap<>();
    private static volatile int pending;
    private static final java.util.Set<UUID> JOINING = new java.util.HashSet<>();

    /** A window's player has a Java client from now on; the others' packets still go nowhere. */
    static synchronized void join(UUID player) {
        if (!TO_CLIENT.containsKey(player)) JOINING.add(player);
        TO_CLIENT.computeIfAbsent(player, id -> new ArrayDeque<>());
        TO_SERVER.computeIfAbsent(player, id -> new ArrayDeque<>());
    }

    /** True once for a client that just joined, which the server then sends what a login sends. */
    static synchronized boolean joining(UUID player) {
        return JOINING.remove(player);
    }

    static synchronized void leave(UUID player) {
        JOINING.remove(player);
        TO_CLIENT.remove(player);
        TO_SERVER.remove(player);
    }

    /** False when the player has no Java client to take it. */
    public static synchronized boolean toClient(UUID player, Packet<?> packet) {
        var queue = TO_CLIENT.get(player);
        if (queue == null) return false;
        queue.add(packet);
        pending++;
        return true;
    }

    static synchronized void toServer(UUID player, Packet<?> packet) {
        var queue = TO_SERVER.get(player);
        if (queue != null) queue.add(packet);
    }

    static synchronized List<Packet<?>> takeForClient(UUID player) {
        var queue = TO_CLIENT.get(player);
        if (queue == null || queue.isEmpty()) return List.of();
        var out = new ArrayList<Packet<?>>(queue);
        queue.clear();
        return out;
    }

    static synchronized List<Packet<?>> takeForServer(UUID player) {
        var queue = TO_SERVER.get(player);
        if (queue == null || queue.isEmpty()) return List.of();
        var out = new ArrayList<Packet<?>>(queue);
        queue.clear();
        return out;
    }

    /** Counts packets sent to clients, so a window can tell without the lock whether it has any to read. */
    static int pending() {
        return pending;
    }
}
