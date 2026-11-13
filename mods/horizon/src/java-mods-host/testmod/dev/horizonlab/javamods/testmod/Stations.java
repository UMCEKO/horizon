package dev.horizonlab.javamods.testmod;

import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.AnvilUpdateEvent;
import net.neoforged.neoforge.event.GrindstoneEvent;
import net.neoforged.neoforge.event.entity.player.AnvilCraftEvent;

/** Mods' listeners on the anvil and the grindstone, driven through the real menus by `tools/scenarios/w11-anvil.json`. */
final class Stations {
    private Stations() {}

    static void register() {
        NeoForge.EVENT_BUS.addListener((AnvilUpdateEvent event) -> {
            if (!event.getLeft().is(Items.STICK) || !event.getRight().isEmpty()) return;
            if ("w11".equals(event.getName())) {
                event.setOutput(new ItemStack(Items.BLAZE_ROD));
                event.setXpCost(7);
                event.setMaterialCost(0);
            } else if ("w11x".equals(event.getName())) {
                event.setOutput(new ItemStack(Items.BARRIER));
                event.setXpCost(1);
            }
        });
        NeoForge.EVENT_BUS.addListener((GrindstoneEvent.OnPlaceItem event) -> {
            if (event.getTopItem().is(Items.PAPER) && event.getBottomItem().isEmpty()) event.setOutput(new ItemStack(Items.MAP));
        });
        NeoForge.EVENT_BUS.addListener((AnvilCraftEvent.Pre event) -> {
            if (event.getOutput().is(Items.BARRIER)) event.setCanceled(true);
        });
    }
}
