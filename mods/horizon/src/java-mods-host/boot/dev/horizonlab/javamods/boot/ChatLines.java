package dev.horizonlab.javamods.boot;

import java.io.DataOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.core.RegistryAccess;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;

/** What the Java client adds to its chat or action bar, kept for the window's chat instead. */
public final class ChatLines {
    private ChatLines() {}

    static final byte SYSTEM_CLIENT = 0;
    static final byte SERVER_OR_PLAYER = 1;
    static final byte ACTION_BAR = 2;

    private record Line(byte kind, boolean animate, Component text) {}

    private static final List<Line> LINES = new ArrayList<>();

    /** `ChatComponent.addMessage`: `system` is a `GuiMessageSource.SYSTEM_CLIENT` line. */
    public static void chat(Component text, boolean system) {
        LINES.add(new Line(system ? SYSTEM_CLIENT : SERVER_OR_PLAYER, false, text));
    }

    /** `Hud.setOverlayMessage`. */
    public static void overlay(Component text, boolean animate) {
        LINES.add(new Line(ACTION_BAR, animate, text));
    }

    /** `int n` x (`byte kind`, `bool animate`, the component as a `ComponentSerialization.CODEC` tag under `v`); clears. */
    static void write(DataOutputStream out) throws IOException {
        var lines = List.copyOf(LINES);
        LINES.clear();
        out.writeInt(lines.size());
        var minecraft = Minecraft.getInstance();
        var access = minecraft.level != null ? minecraft.level.registryAccess() : RegistryAccess.EMPTY;
        var context = access.createSerializationContext(NbtOps.INSTANCE);
        for (Line line : lines) {
            out.writeByte(line.kind());
            out.writeBoolean(line.animate());
            var tag = new CompoundTag();
            tag.put("v", ComponentSerialization.CODEC.encodeStart(context, line.text()).getOrThrow());
            GameBridge.writeTag(out, tag);
        }
    }
}
