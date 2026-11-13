package dev.horizonlab.javamods.boot;

import java.util.List;
import net.minecraft.ChatFormatting;
import net.minecraft.network.chat.Style;
import net.minecraft.util.FormattedCharSequence;

/** The chat box's span reader over a hand-built formatted sequence. */
public final class ChatBoxCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) {
        var gray = Style.EMPTY.withColor(ChatFormatting.GRAY);
        var aqua = Style.EMPTY.withColor(ChatFormatting.AQUA);
        var sequence = FormattedCharSequence.composite(List.of(
            FormattedCharSequence.forward("/give ", gray),
            FormattedCharSequence.forward("@s", aqua),
            FormattedCharSequence.forward(" x", gray)));
        var spans = ChatBox.spans(sequence);
        check(spans.size() == 3, "three differently coloured neighbours stay three spans: " + spans);
        var merged = ChatBox.spans(FormattedCharSequence.composite(List.of(
            FormattedCharSequence.forward("/give", gray),
            FormattedCharSequence.forward(" ", gray),
            FormattedCharSequence.forward("@s", aqua))));
        check(merged.equals(List.of(new ChatBox.Span(0, 6, 0xAAAAAA), new ChatBox.Span(6, 8, 0x55FFFF))), "same-coloured neighbours merge: " + merged);
        System.out.println("ChatBoxCheck ok");
    }
}
