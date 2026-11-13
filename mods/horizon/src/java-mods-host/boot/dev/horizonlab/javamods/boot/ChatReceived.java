package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.time.Instant;
import java.util.Optional;
import java.util.UUID;
import net.minecraft.client.Minecraft;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.network.chat.ChatType;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.network.chat.FilterMask;
import net.minecraft.network.chat.LastSeenMessages;
import net.minecraft.network.chat.MessageSignature;
import net.minecraft.network.chat.PlayerChatMessage;
import net.minecraft.network.chat.SignedMessageBody;
import net.minecraft.network.chat.SignedMessageLink;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.util.Util;
import net.neoforged.neoforge.client.ClientHooks;

/** `ClientChatReceivedEvent` for the lines native is about to show: Rust's listener posts it at `ChatListener`'s hook spots, with vanilla's inputs, and applies the answer. */
final class ChatReceived {
    private ChatReceived() {}

    static final byte SYSTEM = 0, OVERLAY = 1, PLAYER = 2;
    static final byte UNCHANGED = 0, CANCELLED = 1, CHANGED = 2;

    private static String text(DataInputStream in) throws IOException {
        return new String(in.readNBytes(in.readInt()), StandardCharsets.UTF_8);
    }

    private static Tag tag(DataInputStream in) throws IOException {
        return NbtIo.readAnyTag(new DataInputStream(new java.io.ByteArrayInputStream(in.readNBytes(in.readInt()))), NbtAccounter.unlimitedHeap());
    }

    private static Component component(Tag tag) {
        return ComponentSerialization.CODEC.parse(NbtOps.INSTANCE, tag).getOrThrow();
    }

    /** `byte kind`, the component; then for a player or disguised message its chat type and bound, and for a player one its signed message. */
    static byte[] ask(DataInputStream in) throws IOException {
        byte kind = in.readByte();
        Component line = component(tag(in));
        Component result = line;
        var minecraft = Minecraft.getInstance();
        var connection = minecraft.getConnection();
        if (connection == null) return new byte[] {UNCHANGED};
        if (kind <= OVERLAY) {
            result = hook(line, () -> ClientHooks.onClientSystemChat(line, kind == OVERLAY));
        } else {
            String chatType = text(in);
            // Rust sends no bound when it had none to give.
            if (chatType.isEmpty() || in.available() == 0) return new byte[] {UNCHANGED};
            var registry = connection.registryAccess().lookupOrThrow(Registries.CHAT_TYPE);
            var holder = registry.get(ResourceKey.create(Registries.CHAT_TYPE, Identifier.parse(chatType))).orElse(null);
            if (holder == null) return new byte[] {UNCHANGED};
            Component name = component(tag(in));
            Optional<Component> target = in.readBoolean() ? Optional.of(component(tag(in))) : Optional.empty();
            var bound = new ChatType.Bound(holder, name, target);
            if (kind == PLAYER) {
                UUID sender = new UUID(in.readLong(), in.readLong());
                int index = in.readInt();
                MessageSignature signature = in.readBoolean() ? new MessageSignature(in.readNBytes(in.readInt())) : null;
                String content = text(in);
                long timestamp = in.readLong();
                long salt = in.readLong();
                Component unsigned = in.readBoolean() ? component(tag(in)) : null;
                FilterMask filter = in.readByte() == 1 ? FilterMask.FULLY_FILTERED : FilterMask.PASS_THROUGH;
                var info = connection.getPlayerInfo(sender);
                var session = info == null ? null : info.getChatSession();
                UUID sessionId = session == null ? Util.NIL_UUID : session.sessionId();
                var message = new PlayerChatMessage(new SignedMessageLink(index, sender, sessionId), signature,
                        new SignedMessageBody(content, Instant.ofEpochMilli(timestamp), salt, LastSeenMessages.EMPTY), unsigned, filter);
                result = hook(line, () -> ClientHooks.onClientPlayerChat(bound, line, message, sender));
            } else {
                result = hook(line, () -> ClientHooks.onClientChat(bound, line, Util.NIL_UUID));
            }
        }
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        if (result == null) {
            out.writeByte(CANCELLED);
        } else if (result.equals(line)) {
            out.writeByte(UNCHANGED);
        } else {
            out.writeByte(CHANGED);
            var encoded = new ByteArrayOutputStream();
            NbtIo.writeAnyTag(ComponentSerialization.CODEC.encodeStart(NbtOps.INSTANCE, result).getOrThrow(), new DataOutputStream(encoded));
            out.writeInt(encoded.size());
            encoded.writeTo(out);
        }
        return bytes.toByteArray();
    }

    /** Runs the hook behind `GameBridge.safely`; a mod that threw leaves the line as it was. */
    private static Component hook(Component line, java.util.function.Supplier<Component> post) {
        Component[] answer = new Component[1];
        boolean[] ran = new boolean[1];
        GameBridge.safely("ClientChatReceivedEvent", () -> {
            answer[0] = post.get();
            ran[0] = true;
        });
        return ran[0] ? answer[0] : line;
    }
}
