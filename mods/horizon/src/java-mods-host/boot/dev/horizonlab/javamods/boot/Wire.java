package dev.horizonlab.javamods.boot;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.util.HashMap;
import java.util.Map;
import java.util.UUID;
import net.minecraft.core.RegistryAccess;
import net.minecraft.network.ProtocolInfo;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.protocol.BundlePacket;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.configuration.ConfigurationProtocols;
import net.minecraft.network.protocol.game.ClientGamePacketListener;
import net.minecraft.network.protocol.game.GameProtocols;
import net.minecraft.network.protocol.game.ServerGamePacketListener;
import net.neoforged.neoforge.network.connection.ConnectionType;

/** The Java client's packets between two processes: the packets the in-JVM queues carry, written by the protocols a connection writes them with, in the payload `horizon:java`. */
public final class Wire {
    private Wire() {}

    private static final boolean DEBUG = System.getenv("HORIZON_JAVA_DEBUG") != null;
    static final int CONFIGURATION = 0;
    static final int PLAY = 1;

    /** Configuration packets, in a buffer that says it is NeoForge's so that mods' payload types are the ones read. */
    private static final ProtocolInfo<net.minecraft.network.protocol.configuration.ClientConfigurationPacketListener> CONFIGURATION_CODEC =
            ConfigurationProtocols.CLIENTBOUND_TEMPLATE.bind(buf -> new RegistryFriendlyByteBuf(buf, RegistryAccess.EMPTY, ConnectionType.NEOFORGE));

    private static RegistryAccess clientboundAccess;
    private static ProtocolInfo<ClientGamePacketListener> clientbound;
    private static RegistryAccess serverboundAccess;
    private static ProtocolInfo<ServerGamePacketListener> serverbound;
    private static final Map<UUID, ProtocolInfo<ServerGamePacketListener>> SERVER_SIDE = new HashMap<>();

    /** Bridged: a frame for this player's client, which Horizon's connection carries. */
    static void wire(String player, byte[] frame) {
        throw new IllegalStateException("not bridged");
    }

    private static synchronized ProtocolInfo<ClientGamePacketListener> clientbound(RegistryAccess access) {
        if (clientbound == null || clientboundAccess != access) {
            clientbound = GameProtocols.CLIENTBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(access, ConnectionType.NEOFORGE));
            clientboundAccess = access;
        }
        return clientbound;
    }

    private static final GameProtocols.Context CLIENT_CONTEXT = new GameProtocols.Context() {
        @Override
        public boolean hasInfiniteMaterials() {
            return true;
        }

        @Override
        public boolean canUseCommandBlocks() {
            return true;
        }
    };

    private static synchronized ProtocolInfo<ServerGamePacketListener> clientSide(RegistryAccess access) {
        if (serverbound == null || serverboundAccess != access) {
            serverbound = GameProtocols.SERVERBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(access, ConnectionType.NEOFORGE), CLIENT_CONTEXT);
            serverboundAccess = access;
        }
        return serverbound;
    }

    /** `PlayerList.placeNewPlayer`'s inbound protocol: what the player's creative state allows is decided by their server-side view. */
    private static synchronized ProtocolInfo<ServerGamePacketListener> serverSide(UUID player) {
        return SERVER_SIDE.computeIfAbsent(player, id -> GameProtocols.SERVERBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(GameBridge.registries(), ConnectionType.NEOFORGE), new GameProtocols.Context() {
            @Override
            public boolean hasInfiniteMaterials() {
                var view = GameBridge.playerView(id);
                return view != null && view.hasInfiniteMaterials();
            }

            @Override
            public boolean canUseCommandBlocks() {
                var view = GameBridge.playerView(id);
                return view != null && view.canUseGameMasterBlocks();
            }
        }));
    }

    static synchronized void forget(UUID player) {
        SERVER_SIDE.remove(player);
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static byte[] frame(int phase, net.minecraft.network.codec.StreamCodec codec, Packet<?> packet) {
        ByteBuf buf = Unpooled.buffer();
        try {
            buf.writeByte(phase);
            codec.encode(buf, packet);
            byte[] out = new byte[buf.readableBytes()];
            buf.readBytes(out);
            return out;
        } finally {
            buf.release();
        }
    }

    /** The server's packet to this player's client in another process; a bundle goes as its packets, which the client handles in order. */
    static void toClient(UUID player, Packet<?> packet) {
        if (packet instanceof BundlePacket<?> bundle) {
            for (var each : bundle.subPackets()) toClient(player, each);
            return;
        }
        if (DEBUG) System.err.println("Horizon: to " + player + " " + packet.getClass().getSimpleName());
        wire(player.toString(), frame(PLAY, clientbound(GameBridge.registries()).codec(), packet));
    }

    /** A configuration packet the server sends a client, written with vanilla's configuration protocol. */
    static byte[] configuration(Packet<?> packet) {
        return frame(CONFIGURATION, CONFIGURATION_CODEC.codec(), packet);
    }

    /** The client's packet to the server it is connected to. */
    static byte[] toServer(Packet<?> packet) {
        return frame(PLAY, clientSide(Clients.registries).codec(), packet);
    }

    /** A frame from the server, decoded with the registries its configuration gave. */
    @SuppressWarnings("unchecked")
    static Packet<?> fromServer(byte[] frame) {
        ByteBuf buf = Unpooled.wrappedBuffer(frame, 1, frame.length - 1);
        try {
            return frame[0] == CONFIGURATION
                    ? CONFIGURATION_CODEC.codec().decode(buf)
                    : clientbound(Clients.registries).codec().decode(buf);
        } finally {
            buf.release();
        }
    }

    static boolean configuring(byte[] frame) {
        return frame[0] == CONFIGURATION;
    }

    /** A player's packet, decoded with the server's registries; a packet the player may not send was skipped, as a connection skips it. */
    static Packet<?> fromClient(UUID player, byte[] frame) {
        ByteBuf buf = Unpooled.wrappedBuffer(frame, 1, frame.length - 1);
        try {
            return serverSide(player).codec().decode(buf);
        } catch (net.minecraft.network.SkipPacketDecoderException skipped) {
            return null;
        } finally {
            buf.release();
        }
    }
}
