package dev.horizonlab.javamods.boot.mixin;

import java.util.Stack;
import net.minecraft.client.gui.Gui;
import net.minecraft.client.gui.screens.Screen;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(Gui.class)
public interface GuiScreensAccessor {
    @Accessor("screenLayers")
    Stack<Screen> horizon$screenLayers();
}
