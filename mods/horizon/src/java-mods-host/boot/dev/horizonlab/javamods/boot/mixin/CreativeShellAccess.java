package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.gui.screens.inventory.CreativeInventoryListener;
import net.minecraft.client.gui.screens.inventory.CreativeModeInventoryScreen;
import net.minecraft.world.item.CreativeModeTab;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(CreativeModeInventoryScreen.class)
public interface CreativeShellAccess {
    @Accessor("listener")
    CreativeInventoryListener horizon$listener();

    @Accessor("selectedTab")
    static void horizon$selectedTab(CreativeModeTab tab) {
        throw new AssertionError();
    }
}
