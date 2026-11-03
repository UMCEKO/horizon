package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import net.minecraft.client.multiplayer.ClientRegistryLayer;
import net.minecraft.client.multiplayer.RegistryDataCollector;
import net.minecraft.core.RegistryAccess;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.common.ClientboundUpdateTagsPacket;
import net.minecraft.network.protocol.configuration.ClientboundFinishConfigurationPacket;
import net.minecraft.network.protocol.configuration.ClientboundRegistryDataPacket;
import net.minecraft.network.protocol.configuration.ClientboundUpdateEnabledFeaturesPacket;
import net.minecraft.server.packs.resources.ResourceProvider;
import net.minecraft.world.flag.FeatureFlagSet;
import net.minecraft.world.flag.FeatureFlags;

/** The Java client of a window connected to a server in another process: that server's Java side sends its configuration and play packets over the connection, and this client's go back the same way. */
final class RemoteClient {
    private RemoteClient() {}

    private static UUID id;
    private static String name;
    private static net.minecraft.client.multiplayer.ServerData server;
    private static RegistryDataCollector collector;
    private static FeatureFlagSet features = FeatureFlags.DEFAULT_FLAGS;
    private static final List<byte[]> OUTBOX = new ArrayList<>();

    /** Whether this client's server is in another process. */
    static boolean active() {
        return id != null;
    }

    /** The window's login to the server finished: configuration comes next, as the server's packets arrive. */
    static byte[] connect(DataInputStream in) throws Exception {
        var login = new UUID(in.readLong(), in.readLong());
        String profile = new String(in.readNBytes(in.readInt()), java.nio.charset.StandardCharsets.UTF_8);
        String address = new String(in.readNBytes(in.readInt()), java.nio.charset.StandardCharsets.UTF_8);
        if (ClientBridge.player() != null || id != null) ClientBridge.leaveFor();
        id = login;
        name = profile;
        server = new net.minecraft.client.multiplayer.ServerData(address, address, net.minecraft.client.multiplayer.ServerData.Type.OTHER);
        collector = new RegistryDataCollector();
        features = FeatureFlags.DEFAULT_FLAGS;
        OUTBOX.clear();
        ClientBridge.localServer(false);
        return new byte[] {1};
    }

    /** A configuration frame, which arrives in the connection's own configuration phase. */
    static byte[] configure(DataInputStream in) throws Exception {
        inbound(in.readNBytes(in.readInt()));
        return new byte[] {1};
    }

    /** One frame from the server: a configuration packet until it finishes, then play packets for the listener's tick. */
    static void inbound(byte[] frame) throws Exception {
        if (id == null) return;
        Packet<?> packet = Wire.fromServer(frame);
        if (collector == null) {
            Clients.deliver(id, packet);
            return;
        }
        switch (packet) {
            case ClientboundUpdateEnabledFeaturesPacket enabled -> features = FeatureFlags.REGISTRY.fromNames(enabled.features());
            case ClientboundRegistryDataPacket data -> collector.appendContents(data.registry(), data.entries());
            case ClientboundUpdateTagsPacket tags -> collector.appendTags(tags.tags());
            case net.minecraft.network.protocol.common.ClientboundCustomPayloadPacket custom -> receive(custom.payload());
            case ClientboundFinishConfigurationPacket finish -> finish();
            default -> throw new IllegalStateException("configuration packet " + packet);
        }
    }

    /** What NeoForge's handler for the payload does with it: a synced server config is loaded as the client's. */
    private static void receive(net.minecraft.network.protocol.common.custom.CustomPacketPayload payload) {
        if (payload instanceof net.neoforged.neoforge.network.payload.ConfigFilePayload config) net.neoforged.neoforge.network.ConfigSync.receiveSyncedConfig(config.contents(), config.fileName());
        else throw new IllegalStateException("configuration payload " + payload.type().id());
    }

    /** `ClientConfigurationPacketListenerImpl.handleConfigurationFinished` over a network connection. */
    private static void finish() throws Exception {
        var received = ClientRegistryLayer.createRegistryAccess().compositeAccess();
        RegistryAccess.Frozen registries = collector.collectGameRegistries(ResourceProvider.EMPTY, received, false);
        collector = null;
        ClientBridge.begin(id, name, registries, features, server);
        Clients.expect(id);
    }

    /** A packet of the Java client for the server. */
    static void outbound(byte[] frame) {
        OUTBOX.add(frame);
    }

    /** The frames waiting for the server, each with its length. */
    static byte[] take() throws Exception {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(OUTBOX.size());
        for (var frame : OUTBOX) {
            out.writeInt(frame.length);
            out.write(frame);
        }
        OUTBOX.clear();
        return bytes.toByteArray();
    }

    static void left() {
        id = null;
        name = null;
        server = null;
        collector = null;
        OUTBOX.clear();
    }
}
