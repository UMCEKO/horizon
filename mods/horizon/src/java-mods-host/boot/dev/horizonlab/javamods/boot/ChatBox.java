package dev.horizonlab.javamods.boot;

import com.mojang.brigadier.suggestion.Suggestions;
import com.mojang.brigadier.tree.ArgumentCommandNode;
import com.mojang.brigadier.tree.CommandNode;
import dev.horizonlab.javamods.boot.mixin.CommandSuggestionsAccess;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.CommandSuggestions;
import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.gui.screens.ChatScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.Style;
import net.minecraft.network.chat.FontDescription;
import net.minecraft.util.FormattedCharSequence;

/** The window's chat box through the Java client's real `CommandSuggestions`, for the roots whose parsing only the Java client's tree can do. */
public final class ChatBox {
    private ChatBox() {}

    public static final byte CLIENT_COMMAND = 1, MOD_ARGUMENT = 2;
    static final byte NONE = 0, DONE = 1, PENDING = 2;

    private static EditBox box;
    private static ChatScreen screen;
    private static CommandSuggestions suggestions;
    private static Object known;
    private static String lastValue = "";
    private static int lastCursor, lastWidth, lastHeight;
    private static volatile boolean ready, announce;

    /** One merged run of the formatted text: char range (UTF-16 units) and colour (-1 for none). */
    record Span(int start, int end, int rgb) {}

    /** `formatText`'s sequence read back as spans, same-coloured neighbours merged. */
    static List<Span> spans(FormattedCharSequence sequence) {
        var spans = new ArrayList<Span>();
        int[] at = {0};
        sequence.accept((index, style, codepoint) -> {
            int rgb = style.getColor() == null ? -1 : style.getColor().getValue() & 0xFFFFFF;
            int end = at[0] + Character.charCount(codepoint);
            int last = spans.size() - 1;
            if (last >= 0 && spans.get(last).rgb() == rgb) spans.set(last, new Span(spans.get(last).start(), end, rgb));
            else spans.add(new Span(at[0], end, rgb));
            at[0] = end;
            return true;
        });
        return spans;
    }

    /** `rootsChanged` before the next status: the Java client received a new command tree. */
    public static void rootsChanged() {
        known = null;
    }

    /** Dropped at leave. */
    static void forget() {
        box = null;
        screen = null;
        suggestions = null;
        known = null;
        ready = false;
        announce = false;
        lastValue = "";
    }

    private static boolean made(Minecraft minecraft) {
        if (suggestions != null) return true;
        if (minecraft.player == null) return false;
        screen = new ChatScreen("", false);
        box = new EditBox(minecraft.fontFilterFishy, 4, 0, 0, 12, Component.translatable("chat.editBox"));
        box.setMaxLength(256);
        box.setBordered(false);
        suggestions = new CommandSuggestions(minecraft, screen, box, minecraft.font, false, false, 1, 10, true, -805306368);
        return true;
    }

    /** The roots of the Java client's command tree that its own parser must read, with why: 1 a client command, 2 a mod's argument type below. */
    public static Map<String, Byte> javaParsedRoots() {
        var found = new LinkedHashMap<String, Byte>();
        var connection = Minecraft.getInstance().getConnection();
        if (connection == null) return found;
        for (var root : connection.getCommands().getRoot().getChildren()) {
            if (modArgumentBelow(root)) found.put(root.getName(), MOD_ARGUMENT);
        }
        for (var root : net.neoforged.neoforge.client.ClientCommandHandler.getDispatcher().getRoot().getChildren()) {
            found.put(root.getName(), CLIENT_COMMAND);
        }
        return found;
    }

    private static boolean modArgumentBelow(CommandNode<?> node) {
        if (node instanceof ArgumentCommandNode<?, ?> argument && argument.getType() != null && ModCommands.modOwned(argument.getType())) return true;
        for (CommandNode<?> child : node.getChildren()) {
            if (modArgumentBelow(child)) return true;
        }
        return false;
    }

    /** `client_command_info`: `text value`, `int cursor` (UTF-16 units), `int width`, `int height`; replies spans, usage lines and the completions, empty before the player exists. */
    static byte[] info(DataInputStream in) throws IOException {
        var minecraft = Minecraft.getInstance();
        String value = new String(in.readNBytes(in.readInt()), StandardCharsets.UTF_8);
        int cursor = in.readInt();
        int width = in.readInt();
        int height = in.readInt();
        if (!made(minecraft) || minecraft.player == null || minecraft.player.connection == null) return new byte[0];
        var access = (CommandSuggestionsAccess) suggestions;
        boolean same = value.equals(lastValue) && cursor == lastCursor && width == lastWidth && height == lastHeight;
        if (same && ready) {
            ready = false;
        } else {
            screen.width = width;
            screen.height = height;
            box.setX(4);
            box.setY(height - 12);
            box.setWidth(width - 4);
            box.setValue(value);
            box.setCursorPosition(cursor);
            ready = false;
            suggestions.setAllowSuggestions(true);
            suggestions.updateCommandInfo();
            var pending = access.horizon$pendingSuggestions();
            if (pending != null && !pending.isDone()) {
                pending.thenRun(() -> {
                    ready = true;
                    announce = true;
                });
            }
            lastValue = value;
            lastCursor = cursor;
            lastWidth = width;
            lastHeight = height;
        }
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        var parse = access.horizon$currentParse();
        var spans = parse == null ? new ArrayList<Span>() : spans(CommandSuggestionsAccess.horizon$formatText(parse, value, 0));
        out.writeInt(spans.size());
        for (Span span : spans) {
            out.writeInt(span.start());
            out.writeInt(span.end());
            out.writeInt(span.rgb());
        }
        var usage = access.horizon$commandUsage();
        out.writeInt(usage.size());
        for (var line : usage) runs(out, line);
        out.writeInt(access.horizon$commandUsagePosition());
        out.writeInt(access.horizon$commandUsageWidth());
        var pending = access.horizon$pendingSuggestions();
        if (pending == null) {
            out.writeByte(NONE);
        } else if (!pending.isDone()) {
            out.writeByte(PENDING);
        } else {
            Suggestions done = pending.join();
            out.writeByte(DONE);
            out.writeInt(done.getRange().getStart());
            out.writeInt(done.getRange().getEnd());
            out.writeInt(done.getList().size());
            for (var suggestion : done.getList()) {
                text(out, suggestion.getText());
                out.writeBoolean(suggestion.getTooltip() != null);
                if (suggestion.getTooltip() != null) text(out, suggestion.getTooltip().getString());
            }
        }
        return bytes.toByteArray();
    }

    /** `int n` x (`text`, `int rgb or -1`, `byte flags`, `text font`), as `Recorder` writes a TEXT's runs. */
    private static void runs(DataOutputStream out, FormattedCharSequence line) throws IOException {
        var runs = new ArrayList<Object[]>();
        var run = new StringBuilder();
        Style[] last = {null};
        line.accept((index, style, codepoint) -> {
            if (last[0] != null && !style.equals(last[0])) {
                runs.add(new Object[] {run.toString(), last[0]});
                run.setLength(0);
            }
            last[0] = style;
            run.appendCodePoint(codepoint);
            return true;
        });
        if (last[0] != null) runs.add(new Object[] {run.toString(), last[0]});
        out.writeInt(runs.size());
        for (Object[] r : runs) {
            Style style = (Style) r[1];
            text(out, (String) r[0]);
            out.writeInt(style.getColor() == null ? -1 : style.getColor().getValue() & 0xFFFFFF);
            out.writeByte((style.isBold() ? 1 : 0) | (style.isItalic() ? 2 : 0) | (style.isUnderlined() ? 4 : 0) | (style.isStrikethrough() ? 8 : 0) | (style.isObfuscated() ? 16 : 0));
            text(out, style.getFont() instanceof FontDescription.Resource resource ? resource.id().toString() : "");
        }
    }

    private static void text(DataOutputStream out, String value) throws IOException {
        byte[] utf = value.getBytes(StandardCharsets.UTF_8);
        out.writeInt(utf.length);
        out.write(utf);
    }

    /** `client_command_names`: `int n` x (`text root`, `byte why`), the roots native hands to the Java client's parser. */
    static byte[] names() throws IOException {
        var roots = javaParsedRoots();
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(roots.size());
        for (var entry : roots.entrySet()) {
            text(out, entry.getKey());
            out.writeByte(entry.getValue());
        }
        return bytes.toByteArray();
    }

    /** The status tail: `bool commandInfoReady` (set once, cleared when written), then `bool rootsChanged` (the Java client's tree is not the one the roots were last read from). */
    static void writeStatus(DataOutputStream out) throws IOException {
        out.writeBoolean(announce);
        announce = false;
        var connection = Minecraft.getInstance().getConnection();
        Object commands = connection == null ? null : connection.getCommands();
        out.writeBoolean(commands != known);
        known = commands;
    }
}
