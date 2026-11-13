package dev.horizonlab.javamods.boot;

import java.time.Instant;
import java.util.BitSet;
import java.util.List;
import java.util.Optional;
import net.minecraft.network.chat.LastSeenMessages;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ServerboundChatCommandPacket;
import net.minecraft.network.protocol.game.ServerboundChatPacket;

/** What native sends for a typed line, from the packets the Java client's `sendChat`/`sendCommand` would have sent. */
public final class ChatSendCheck {
    private static ServerboundChatPacket chat(String message) {
        return new ServerboundChatPacket(message, Instant.EPOCH, 0L, Optional.empty(), new LastSeenMessages.Update(0, new BitSet(), (byte) 0));
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) {
        List<Packet<?>> command = List.of(new ServerboundChatCommandPacket("time query daytime"));
        check(ChatSend.verdict("/time query daytime", command) == ChatSend.AS_TYPED, "a command packet sends the command as typed");
        check(ChatSend.verdict("/horizon_java_test_client", List.of()) == ChatSend.NOTHING, "a client command sends nothing");
        check(ChatSend.verdict("a", List.of(chat("a"))) == ChatSend.AS_TYPED, "an unchanged chat packet sends as typed");
        check(ChatSend.verdict("a", List.of(chat("b"))) == ChatSend.TEXT, "a changed chat packet sends its text");
        check("b".equals(ChatSend.sentMessage(List.of(chat("b")))), "the text is the packet's");
        check(ChatSend.verdict("a", List.of()) == ChatSend.NOTHING, "a canceled chat sends nothing");
        System.out.println("ChatSendCheck ok");
    }
}
