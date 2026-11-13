package dev.horizonlab.javamods.boot;

import java.io.IOException;
import net.minecraft.world.entity.ai.attributes.AttributeModifier;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.component.ItemAttributeModifiers;

/** `ItemStack.getAttributeModifiers` (the component, then `ItemAttributeModifierEvent`) for `java_listen_item_attributes.rs`. */
final class ItemAttributes {
    private ItemAttributes() {}

    /** A stack's saved form, answered with `int m` of (`text attribute`, `text id`, `double amount`, `byte operation`, `text slot group`). */
    static void modifiers(Crossing.In in, Crossing.Out out) throws IOException {
        ItemStack stack = GameBridge.decode(in.rest());
        var entries = stack.isEmpty() ? java.util.List.<ItemAttributeModifiers.Entry>of() : stack.getAttributeModifiers().modifiers();
        out.writeInt(entries.size());
        for (var entry : entries) {
            text(out, entry.attribute().getRegisteredName());
            text(out, entry.modifier().id().toString());
            out.writeDouble(entry.modifier().amount());
            out.writeByte(operation(entry.modifier().operation()));
            text(out, entry.slot().getSerializedName());
        }
    }

    private static int operation(AttributeModifier.Operation operation) {
        return switch (operation) {
            case ADD_VALUE -> 0;
            case ADD_MULTIPLIED_BASE -> 1;
            case ADD_MULTIPLIED_TOTAL -> 2;
        };
    }

    private static void text(Crossing.Out out, String value) throws IOException {
        byte[] utf = value.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        out.writeInt(utf.length);
        out.write(utf);
    }
}
