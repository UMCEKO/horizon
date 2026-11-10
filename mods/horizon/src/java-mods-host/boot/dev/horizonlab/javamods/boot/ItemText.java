package dev.horizonlab.javamods.boot;

import com.mojang.serialization.JsonOps;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import net.minecraft.client.Minecraft;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;

/** A stack's `getHoverName` and `getTooltipLines` as the Java client computes them, so a mod's `getName`, `appendHoverText` and `ItemTooltipEvent` listeners run. */
final class ItemText {
    private ItemText() {}

    static byte[] take(DataInputStream in) throws IOException {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        int n = in.readInt();
        out.writeInt(n);
        var minecraft = Minecraft.getInstance();
        for (int i = 0; i < n; i++) {
            byte[] nbt = in.readNBytes(in.readInt());
            boolean advanced = in.readBoolean();
            boolean creative = in.readBoolean();
            var level = minecraft.level;
            if (level == null) {
                out.writeByte(2);
                continue;
            }
            try {
                var stack = GameBridge.decode(nbt);
                var flag = advanced ? TooltipFlag.Default.ADVANCED : TooltipFlag.Default.NORMAL;
                if (creative) flag = flag.asCreative();
                var name = stack.getHoverName();
                var lines = stack.getTooltipLines(Item.TooltipContext.of(level), minecraft.player, flag);
                var context = level.registryAccess().createSerializationContext(JsonOps.INSTANCE);
                var texts = new java.util.ArrayList<String>();
                for (var line : lines) texts.add(json(context, line));
                var shown = json(context, name);
                out.writeBoolean(true);
                writeText(out, shown);
                out.writeInt(texts.size());
                for (var text : texts) writeText(out, text);
            } catch (Throwable t) {
                System.err.println("Horizon: an item's text failed");
                t.printStackTrace();
                out.writeBoolean(false);
            }
        }
        return bytes.toByteArray();
    }

    private static void writeText(DataOutputStream out, String text) throws IOException {
        var utf = text.getBytes(StandardCharsets.UTF_8);
        out.writeInt(utf.length);
        out.write(utf);
    }

    private static String json(com.mojang.serialization.DynamicOps<com.google.gson.JsonElement> ops, Component component) {
        return ComponentSerialization.CODEC.encodeStart(ops, component).getOrThrow().toString();
    }
}
