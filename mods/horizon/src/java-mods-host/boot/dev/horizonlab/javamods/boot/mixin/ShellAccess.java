package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.world.inventory.Slot;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Mutable;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(AbstractContainerScreen.class)
public interface ShellAccess {
    @Accessor("leftPos")
    void horizon$leftPos(int value);

    @Accessor("topPos")
    void horizon$topPos(int value);

    @Mutable
    @Accessor("imageWidth")
    void horizon$imageWidth(int value);

    @Mutable
    @Accessor("imageHeight")
    void horizon$imageHeight(int value);

    @Accessor("hoveredSlot")
    void horizon$hovered(Slot slot);
}
