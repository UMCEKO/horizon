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

    private static final java.util.Map<net.minecraft.server.level.ServerLevel, it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet> RESHAPED = new java.util.IdentityHashMap<>();

    /** `ChunkHolder.blockChanged`: the position is sent at the level's next `broadcastChanges`. */
    public static synchronized void blockChanged(net.minecraft.server.level.ServerLevel level, net.minecraft.core.BlockPos pos) {
        CHANGED.computeIfAbsent(level, l -> new it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet()).add(pos.asLong());
        reshaped(level, pos);
    }

    /** The block's collision shape may have changed with its block entity: Horizon gets it again at the level's next tick. */
    static synchronized void reshaped(net.minecraft.server.level.ServerLevel level, net.minecraft.core.BlockPos pos) {
        RESHAPED.computeIfAbsent(level, l -> new it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet()).add(pos.asLong());
    }

    /** Horizon's collisions take a mod's block's `getCollisionShape(level, pos)` where its code may read the block entity. */
    static void sendShapes(net.minecraft.server.level.ServerLevel level) {
        it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet reshaped;
        synchronized (Clients.class) {
            reshaped = RESHAPED.remove(level);
        }
        if (reshaped == null) return;
        for (long at : reshaped) {
            var pos = net.minecraft.core.BlockPos.of(at);
            var state = level.getBlockState(pos);
            if (!state.hasBlockEntity() || !BlockEntities.ours(state)) continue;
            collisionShape(at, state, boxes(state.getCollisionShape(level, pos, net.minecraft.world.phys.shapes.CollisionContext.empty())));
        }
    }

    /** `VoxelShape.toAabbs`, six doubles a box. */
    static double[] boxes(net.minecraft.world.phys.shapes.VoxelShape shape) {
        var boxes = shape.toAabbs();
        var out = new double[boxes.size() * 6];
        int i = 0;
        for (var box : boxes) {
            out[i++] = box.minX;
            out[i++] = box.minY;
            out[i++] = box.minZ;
            out[i++] = box.maxX;
            out[i++] = box.maxY;
            out[i++] = box.maxZ;
        }
        return out;
    }

    /** Bridged: the collision shape Horizon's server gives this block while it keeps this state. */
    private static void collisionShape(long pos, net.minecraft.world.level.block.state.BlockState state, double[] boxes) {
        throw new IllegalStateException("not bridged");
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
            send(level, pos, new net.minecraft.network.protocol.game.ClientboundBlockUpdatePacket(pos, state));
            if (!state.hasBlockEntity()) continue;
            var entity = level.getBlockEntity(pos);
            var packet = entity == null ? null : entity.getUpdatePacket();
            if (packet != null) send(level, pos, packet);
        }
    }

    /** A block entity joined the level: its block and its update tag, as a chunk's arrival carries them (`ClientboundLevelChunkPacketData`). */
    static void joined(net.minecraft.server.level.ServerLevel level, net.minecraft.world.level.block.entity.BlockEntity entity) {
        reshaped(level, entity.getBlockPos());
        if (level.players().isEmpty()) return;
        send(level, entity.getBlockPos(), new net.minecraft.network.protocol.game.ClientboundBlockUpdatePacket(entity.getBlockPos(), entity.getBlockState()));
        send(level, entity.getBlockPos(), net.minecraft.network.protocol.game.ClientboundBlockEntityDataPacket.create(entity, net.minecraft.world.level.block.entity.BlockEntity::getUpdateTag));
    }

    /** A chunk reached this player: what `ClientboundLevelChunkPacketData` carries of its block entities. */
    static void chunkArrived(net.minecraft.server.level.ServerLevel level, net.minecraft.server.level.ServerPlayer player, net.minecraft.world.level.chunk.LevelChunk chunk) {
        for (var entity : BlockEntities.inChunk(level, chunk.getPos()).values()) {
            toClient(player.getUUID(), new net.minecraft.network.protocol.game.ClientboundBlockUpdatePacket(entity.getBlockPos(), entity.getBlockState()));
            toClient(player.getUUID(), net.minecraft.network.protocol.game.ClientboundBlockEntityDataPacket.create(entity, net.minecraft.world.level.block.entity.BlockEntity::getUpdateTag));
        }
    }

    /** `ChunkHolder.broadcast` to `ChunkMap.getPlayers(pos, false)`. */
    private static void send(net.minecraft.server.level.ServerLevel level, net.minecraft.core.BlockPos pos, Packet<?> packet) {
        for (var player : level.getChunkSource().chunkMap.getPlayers(net.minecraft.world.level.ChunkPos.containing(pos), false)) toClient(player.getUUID(), packet);
    }

    /** Counts packets sent to clients and geometry marked to record, so a window knows without the lock when it has work. */
    static int pending() {
        return pending + changes.get();
    }

    /** Counts geometry and renderers marked to record again, which a window reads with the packets to know when to ask. */
    static final java.util.concurrent.atomic.AtomicInteger changes = new java.util.concurrent.atomic.AtomicInteger();
}
