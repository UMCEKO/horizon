package dev.horizonlab.javamods.boot;

import java.io.IOException;
import java.util.ArrayList;
import net.minecraft.core.Holder;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.Registries;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.enchantment.Enchantment;
import net.minecraft.world.item.enchantment.EnchantmentHelper;
import net.minecraft.world.item.enchantment.ItemEnchantments;

/** The tool of a native loot roll as Java sees its enchantments: the item's own code, NeoForge's event and mixins on `getItemEnchantmentLevel`. */
public final class Loot {
    private Loot() {}

    /** `tool_enchantments`: the enchantments above level 0 on the tool, in registry order. */
    static void toolEnchantments(Crossing.In in, Crossing.Out out) throws IOException {
        var level = GameBridge.level(in.readInt());
        in.readLong();
        ItemStack tool = GameBridge.decode(in.readNBytes(in.readInt()));
        var found = new ArrayList<Holder.Reference<Enchantment>>();
        var levels = new ArrayList<Integer>();
        level.registryAccess().lookupOrThrow(Registries.ENCHANTMENT).listElements().forEach(holder -> {
            int at = EnchantmentHelper.getItemEnchantmentLevel(holder, tool);
            if (at > 0) {
                found.add(holder);
                levels.add(at);
            }
        });
        out.writeInt(found.size());
        for (int i = 0; i < found.size(); i++) {
            GameBridge.writeText(out, found.get(i).key().identifier().toString());
            out.writeInt(levels.get(i));
        }
    }

    /** A copy of the tool whose enchantments component holds the levels Java gives, for one roll. */
    public static ItemStack effective(ItemStack tool, RegistryAccess access) {
        var levels = new ItemEnchantments.Mutable(ItemEnchantments.EMPTY);
        access.lookupOrThrow(Registries.ENCHANTMENT).listElements().forEach(holder -> {
            int at = EnchantmentHelper.getItemEnchantmentLevel(holder, tool);
            if (at > 0) levels.set(holder, at);
        });
        ItemStack copy = tool.copy();
        copy.set(DataComponents.ENCHANTMENTS, levels.toImmutable());
        return copy;
    }

    /** Whether a roll's tool must be flattened through `effective`. */
    static boolean needsEffective(ItemStack tool) {
        return !tool.isEmpty() && (CraftFacts.enchantCode(tool.getItem()) || CraftFacts.enchantmentEventHeard());
    }
}
