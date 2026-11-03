package dev.horizonlab.javamods.boot.mixin;

import java.util.List;
import net.neoforged.neoforge.client.gui.GuiLayerManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(GuiLayerManager.class)
public interface GuiLayersAccessor {
    @Accessor("layers")
    List<GuiLayerManager.NamedLayer> horizon$layers();
}
