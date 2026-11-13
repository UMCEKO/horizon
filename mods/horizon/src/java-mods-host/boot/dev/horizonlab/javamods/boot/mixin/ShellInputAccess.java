package dev.horizonlab.javamods.boot.mixin;

import java.util.Set;
import net.minecraft.client.gui.screens.inventory.AbstractContainerScreen;
import net.minecraft.world.inventory.Slot;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The input state of a shell that native keeps (`Clicks`, its quick craft drag), written and read back around a mod's input. */
@Mixin(AbstractContainerScreen.class)
public interface ShellInputAccess {
    @Accessor("isQuickCrafting")
    boolean horizon$quickCrafting();

    @Accessor("isQuickCrafting")
    void horizon$setQuickCrafting(boolean value);

    @Accessor("quickCraftingType")
    int horizon$quickCraftingType();

    @Accessor("quickCraftingType")
    void horizon$setQuickCraftingType(int value);

    @Accessor("quickCraftingButton")
    int horizon$quickCraftingButton();

    @Accessor("quickCraftingButton")
    void horizon$setQuickCraftingButton(int value);

    @Accessor("skipNextRelease")
    boolean horizon$skipNextRelease();

    @Accessor("skipNextRelease")
    void horizon$setSkipNextRelease(boolean value);

    @Accessor("quickCraftSlots")
    Set<Slot> horizon$quickCraftSlots();
}
