package dev.horizonlab.javamods.boot;

import io.netty.buffer.Unpooled;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.resources.Identifier;
import net.neoforged.neoforge.network.connection.ConnectionType;

/** Data component values on the wire, by their type's own codecs: what `DataComponentPatch.STREAM_CODEC` writes after the type's id, and its saved form. */
final class StackComponents {
    private StackComponents() {}

    private static String text(DataInputStream in) throws IOException {
        return new String(in.readNBytes(in.readInt()), StandardCharsets.UTF_8);
    }

    private static DataComponentType<?> type(String name) {
        return BuiltInRegistries.DATA_COMPONENT_TYPE.getValue(Identifier.parse(name));
    }

    /** The registries reading the wire: the client's from its configuration, or on a dedicated server its own; a connection is NeoForge's on both ends. */
    private static net.minecraft.core.RegistryAccess reading() {
        if (!net.neoforged.fml.loading.FMLEnvironment.getDist().isClient()) return GameBridge.registries();
        var registries = Clients.registries;
        if (registries == null) throw new IllegalStateException("component_read before the Java client's configuration");
        return registries;
    }

    static byte[] read(byte[] input) {
        try {
            var in = new DataInputStream(new ByteArrayInputStream(input));
            var type = type(text(in));
            return type == null ? new byte[0] : read(type, in.readNBytes(in.readInt()), reading());
        } catch (Throwable t) {
            t.printStackTrace();
            return new byte[0];
        }
    }

    private static <T> byte[] read(DataComponentType<T> type, byte[] input, net.minecraft.core.RegistryAccess registries) throws IOException {
        var buffer = new RegistryFriendlyByteBuf(Unpooled.wrappedBuffer(input), registries, ConnectionType.NEOFORGE);
        T value = type.streamCodec().decode(buffer);
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(buffer.readerIndex());
        var codec = type.codec();
        out.writeBoolean(codec != null);
        if (codec != null) {
            var root = new CompoundTag();
            root.put("v", codec.encodeStart(registries.createSerializationContext(NbtOps.INSTANCE), value).getOrThrow());
            GameBridge.writeTag(out, root);
        }
        return bytes.toByteArray();
    }

    static byte[] write(byte[] input) {
        try {
            var in = new DataInputStream(new ByteArrayInputStream(input));
            var bytes = new ByteArrayOutputStream();
            var out = new DataOutputStream(bytes);
            int values = in.readInt();
            for (int i = 0; i < values; i++) {
                var type = type(text(in));
                var saved = NbtIo.read(new DataInputStream(new ByteArrayInputStream(in.readNBytes(in.readInt()))), NbtAccounter.unlimitedHeap());
                byte[] stream = type == null ? null : write(type, saved);
                out.writeBoolean(stream != null);
                if (stream != null) {
                    out.writeInt(stream.length);
                    out.write(stream);
                }
            }
            return bytes.toByteArray();
        } catch (Throwable t) {
            t.printStackTrace();
            return new byte[0];
        }
    }

    private static <T> byte[] write(DataComponentType<T> type, CompoundTag saved) {
        var codec = type.codec();
        if (codec == null) return null;
        try {
            T value = codec.parse(GameBridge.registries().createSerializationContext(NbtOps.INSTANCE), saved.get("v")).getOrThrow();
            var bytes = Unpooled.buffer();
            type.streamCodec().encode(new RegistryFriendlyByteBuf(bytes, GameBridge.registries(), ConnectionType.NEOFORGE), value);
            byte[] out = new byte[bytes.readableBytes()];
            bytes.readBytes(out);
            return out;
        } catch (RuntimeException e) {
            e.printStackTrace();
            return null;
        }
    }
}
