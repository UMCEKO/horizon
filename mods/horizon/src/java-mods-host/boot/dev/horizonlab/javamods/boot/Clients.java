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
    /** Whether a block's geometry comes from its block entity's data, so its clients need that data; set where the client classes exist. */
    static volatile java.util.function.Predicate<net.minecraft.world.level.block.state.BlockState> dynamic = state -> false;
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

    private static final java.util.Map<net.minecraft.server.level.ServerLevel, it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet> CHANGED = new java.util.IdentityHashMap<>();

    /** `ChunkHolder.blockChanged`: the position is sent at the level's next `broadcastChanges`. */
    public static synchronized void blockChanged(net.minecraft.server.level.ServerLevel level, net.minecraft.core.BlockPos pos) {
        CHANGED.computeIfAbsent(level, l -> new it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet()).add(pos.asLong());
    }

    /** `ChunkHolder.broadcastChanges`, as the level's chunk source ticks: each changed block's state, then its block entity's update packet. */
    static void broadcastChanges(net.minecraft.server.level.ServerLevel level) {
        it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet changed;
        synchronized (Clients.class) {
            changed = CHANGED.remove(level);
        }
        if (changed == null || level.players().isEmpty()) return;
        for (long at : changed) {
            var pos = net.minecraft.core.BlockPos.of(at);
            var state = level.getBlockState(pos);
            send(level, new net.minecraft.network.protocol.game.ClientboundBlockUpdatePacket(pos, state));
            if (!state.hasBlockEntity()) continue;
            var entity = level.getBlockEntity(pos);
            var packet = entity == null ? null : entity.getUpdatePacket();
            if (packet != null) send(level, packet);
        }
    }

    /** A block entity joined the level: its block and its update tag, as a chunk's arrival carries them (`ClientboundLevelChunkPacketData`). */
    static void joined(net.minecraft.server.level.ServerLevel level, net.minecraft.world.level.block.entity.BlockEntity entity) {
        if (level.players().isEmpty()) return;
        send(level, new net.minecraft.network.protocol.game.ClientboundBlockUpdatePacket(entity.getBlockPos(), entity.getBlockState()));
        send(level, net.minecraft.network.protocol.game.ClientboundBlockEntityDataPacket.create(entity, net.minecraft.world.level.block.entity.BlockEntity::getUpdateTag));
    }

    private static void send(net.minecraft.server.level.ServerLevel level, Packet<?> packet) {
        for (var player : level.players()) toClient(player.getUUID(), packet);
    }

    /** Counts packets sent to clients and geometry marked to record, so a window knows without the lock when it has work. */
    static int pending() {
        return pending + changes.get();
    }

    /** Counts geometry and renderers marked to record again, which a window reads with the packets to know when to ask. */
    static final java.util.concurrent.atomic.AtomicInteger changes = new java.util.concurrent.atomic.AtomicInteger();
}
