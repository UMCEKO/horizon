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

    /** The registries of the Java client's connection and of the server view alike; a connection is NeoForge's on both ends. */
    private static RegistryFriendlyByteBuf buffer(io.netty.buffer.ByteBuf bytes) {
        return new RegistryFriendlyByteBuf(bytes, GameBridge.registries(), ConnectionType.NEOFORGE);
    }

    static byte[] read(byte[] input) {
        try {
            var in = new DataInputStream(new ByteArrayInputStream(input));
            var type = type(text(in));
            return type == null ? new byte[0] : read(type, in.readNBytes(in.readInt()));
        } catch (Throwable t) {
            t.printStackTrace();
            return new byte[0];
        }
    }

    private static <T> byte[] read(DataComponentType<T> type, byte[] input) throws IOException {
        var buffer = buffer(Unpooled.wrappedBuffer(input));
        T value = type.streamCodec().decode(buffer);
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(buffer.readerIndex());
        var codec = type.codec();
        out.writeBoolean(codec != null);
        if (codec != null) {
            var root = new CompoundTag();
            root.put("v", codec.encodeStart(GameBridge.registries().createSerializationContext(NbtOps.INSTANCE), value).getOrThrow());
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
            type.streamCodec().encode(buffer(bytes), value);
            byte[] out = new byte[bytes.readableBytes()];
            bytes.readBytes(out);
            return out;
        } catch (RuntimeException e) {
            e.printStackTrace();
            return null;
        }
    }
}
