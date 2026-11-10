package dev.horizonlab.javamods.boot;

import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import java.io.DataInputStream;
import java.io.IOException;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.function.Predicate;
import net.minecraft.network.protocol.Packet;
import net.minecraft.server.level.ChunkTrackingView;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.ChunkPos;
import net.neoforged.neoforge.event.EventHooks;

/** `ChunkMap`'s knowledge of its players, told by Horizon's hub as it changes: each one's view, the chunks sent to them, and who each entity is `seenBy`. */
public final class Tracking {
    private Tracking() {}

    private static final class Seen {
        int entity;
        String dimension = "";
        ChunkTrackingView view = ChunkTrackingView.EMPTY;
        final LongOpenHashSet sent = new LongOpenHashSet();
    }

    private static final Map<UUID, Seen> PLAYERS = new HashMap<>();
    private static final Int2ObjectOpenHashMap<Set<UUID>> SEEN_BY = new Int2ObjectOpenHashMap<>();

    static void reset() {
        PLAYERS.clear();
        SEEN_BY.clear();
    }

    private static Seen seen(UUID player) {
        return PLAYERS.computeIfAbsent(player, id -> new Seen());
    }

    /** A new view of a player gets the `ChunkTrackingView` its field holds on a real one. */
    static void attach(ServerPlayer player) {
        player.setChunkTrackingView(seen(player.getUUID()).view);
    }

    /** The events `ChunkMap.markChunkPendingToSend` and `PlayerChunkSender` owe a view that came after its chunks did. */
    static void joined(ServerPlayer player, ServerLevel level) {
        var seen = seen(player.getUUID());
        if (!seen.dimension.equals(level.dimension().identifier().toString())) return;
        for (long at : seen.sent.toLongArray()) arrived(player, level, ChunkPos.getX(at), ChunkPos.getZ(at));
    }

    private static void arrived(ServerPlayer player, ServerLevel level, int x, int z) {
        var chunk = ChunkViews.get(level, x, z);
        if (chunk == null) return;
        EventHooks.fireChunkWatch(player, chunk, level);
        Clients.chunkArrived(level, player, chunk);
        EventHooks.fireChunkSent(player, chunk, level);
    }

    /** `ChunkMap.isChunkTracked`'s second half: `PlayerChunkSender.isPending` is whether Horizon's hub hasn't sent it yet. */
    public static boolean sent(ServerPlayer player, ServerLevel level, int x, int z) {
        var seen = PLAYERS.get(player.getUUID());
        return seen != null && seen.sent.contains(ChunkPos.pack(x, z)) && seen.dimension.equals(level.dimension().identifier().toString());
    }

    /** The hub's changes since the last call, as `Hub::take_tracking` lists them. */
    static void apply(ServerLevel level, DataInputStream in) throws IOException {
        for (int n = in.readInt(); n > 0; n--) {
            int kind = in.readInt();
            var id = new UUID(in.readLong(), in.readLong());
            var player = GameBridge.playerView(id);
            switch (kind) {
                case 0 -> {
                    var seen = seen(id);
                    seen.entity = in.readInt();
                    seen.dimension = GameBridge.readText(in);
                    seen.view = ChunkTrackingView.of(new ChunkPos(in.readInt(), in.readInt()), in.readInt());
                    if (player != null) player.setChunkTrackingView(seen.view);
                }
                case 1 -> {
                    int x = in.readInt();
                    int z = in.readInt();
                    seen(id).sent.add(ChunkPos.pack(x, z));
                    if (player != null) arrived(player, level, x, z);
                }
                case 2 -> {
                    int x = in.readInt();
                    int z = in.readInt();
                    seen(id).sent.remove(ChunkPos.pack(x, z));
                    if (player != null) EventHooks.fireChunkUnWatch(player, new ChunkPos(x, z), level);
                }
                case 3 -> seen(id).sent.clear();
                case 4 -> {
                    int entity = in.readInt();
                    if (in.readBoolean()) {
                        SEEN_BY.computeIfAbsent(entity, e -> new HashSet<>()).add(id);
                    } else {
                        var by = SEEN_BY.get(entity);
                        if (by != null && by.remove(id) && by.isEmpty()) SEEN_BY.remove(entity);
                    }
                }
                default -> {
                    PLAYERS.remove(id);
                    SEEN_BY.values().removeIf(by -> by.remove(id) && by.isEmpty());
                }
            }
        }
    }

    private static int id(Entity entity) {
        if (!(entity instanceof ServerPlayer player)) return entity.getId();
        var seen = PLAYERS.get(player.getUUID());
        return seen == null ? entity.getId() : seen.entity;
    }

    /** `TrackedEntity.seenBy` is empty. */
    public static boolean untracked(Entity entity) {
        return !SEEN_BY.containsKey(id(entity));
    }

    /** `TrackedEntity.sendToTrackingPlayersFiltered`: a tracker with no view has no connection in Java to send it through. */
    public static void sendToTracking(Entity entity, Packet<?> packet, Predicate<ServerPlayer> filter) {
        var by = SEEN_BY.get(id(entity));
        if (by == null) return;
        for (UUID id : by) {
            var player = GameBridge.playerView(id);
            if (player != null && filter.test(player)) player.connection.send(packet);
        }
    }

    /** `TrackedEntity.sendToTrackingPlayersAndSelf`. */
    public static void sendToTrackingAndSelf(Entity entity, Packet<?> packet) {
        sendToTracking(entity, packet, player -> true);
        if (entity instanceof ServerPlayer player) player.connection.send(packet);
    }
}
