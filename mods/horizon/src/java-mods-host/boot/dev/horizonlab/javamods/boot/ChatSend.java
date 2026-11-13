package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ServerboundChatCommandPacket;
import net.minecraft.network.protocol.game.ServerboundChatCommandSignedPacket;
import net.minecraft.network.protocol.game.ServerboundChatPacket;
import org.jetbrains.annotations.Nullable;

/** A line typed in the window's chat box, run through the Java client's real `sendChat`/`sendCommand` with the packet they would send kept instead. */
public final class ChatSend {
    private ChatSend() {}

    static final byte AS_TYPED = 0;
    static final byte NOTHING = 1;
    static final byte TEXT = 2;

    /** Packets `ClientBridge.send` keeps instead of sending, while a chat send runs. */
    static @Nullable List<Packet<?>> capturing;

    /** `client_chat_send`: `text line` (normalized, with its slash for a command); replies `byte verdict` (0 as typed, 1 nothing to send, 2 `text` to send), then the chat lines this call produced. */
    static byte[] send(DataInputStream in) throws IOException {
        String line = new String(in.readNBytes(in.readInt()), StandardCharsets.UTF_8);
        var connection = Minecraft.getInstance().getConnection();
        var kept = new ArrayList<Packet<?>>();
        byte verdict = NOTHING;
        if (connection != null) {
            capturing = kept;
            try {
                if (line.startsWith("/")) connection.sendCommand(line.substring(1));
                else connection.sendChat(line);
            } finally {
                capturing = null;
            }
            verdict = verdict(line, kept);
        }
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeByte(verdict);
        if (verdict == TEXT) {
            byte[] utf = sentMessage(kept).getBytes(StandardCharsets.UTF_8);
            out.writeInt(utf.length);
            out.write(utf);
        }
        ChatLines.write(out);
        return bytes.toByteArray();
    }

    /** Whether `packet` was kept instead of sent. */
    static boolean capture(Packet<?> packet) {
        if (capturing == null) return false;
        capturing.add(packet);
        return true;
    }

    /** What native sends for `line` given the packets the Java client would have sent: as typed, nothing, or another text. */
    static byte verdict(String line, List<Packet<?>> captured) {
        if (line.startsWith("/")) {
            boolean sent = captured.stream().anyMatch(p -> p instanceof ServerboundChatCommandPacket || p instanceof ServerboundChatCommandSignedPacket);
            return sent ? AS_TYPED : NOTHING;
        }
        String message = sentMessage(captured);
        if (message == null) return NOTHING;
        return message.equals(line) ? AS_TYPED : TEXT;
    }

    static @Nullable String sentMessage(List<Packet<?>> captured) {
        for (Packet<?> packet : captured) {
            if (packet instanceof ServerboundChatPacket chat) return chat.message();
        }
        return null;
    }
}
