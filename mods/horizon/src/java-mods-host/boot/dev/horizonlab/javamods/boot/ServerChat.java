package dev.horizonlab.javamods.boot;

import java.io.IOException;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.neoforged.neoforge.common.CommonHooks;

/** `server_chat`: a player's message through NeoForge's chat decorator, which posts `ServerChatEvent`. */
final class ServerChat {
    private ServerChat() {}

    private static final int UNCHANGED = 0, CANCELLED = 1, CHANGED = 2;

    /** The acting player's view, then `text raw`; answers a byte (0 unchanged, 1 cancelled, 2 and the component) and the effects. */
    static void run(Crossing.In in, Crossing.Out out) throws Exception {
        var call = GameBridge.fill(in, true);
        String raw = in.text();
        int[] verdict = {UNCHANGED};
        Component[] changed = {null};
        GameBridge.safely("ServerChatEvent", () -> {
            var original = Component.literal(raw);
            var decorated = CommonHooks.getServerChatSubmittedDecorator().decorate(call.player(), original);
            if (decorated == null) verdict[0] = CANCELLED;
            else if (!decorated.equals(original)) {
                verdict[0] = CHANGED;
                changed[0] = decorated;
            }
        });
        out.writeByte(verdict[0]);
        if (verdict[0] == CHANGED) write(out, changed[0]);
        GameBridge.writeEffects(out);
    }

    private static void write(Crossing.Out out, Component component) throws IOException {
        var tag = ComponentSerialization.CODEC.encodeStart(GameBridge.registries().createSerializationContext(NbtOps.INSTANCE), component).getOrThrow();
        var wrapper = new CompoundTag();
        wrapper.put("text", tag);
        GameBridge.writeTag(out, wrapper);
    }
}
