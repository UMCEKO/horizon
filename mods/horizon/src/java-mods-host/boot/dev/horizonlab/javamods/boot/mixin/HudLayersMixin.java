package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.gui.Hud;
import net.neoforged.neoforge.client.gui.GuiLayerManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Vanilla's HUD layers are native: the layers registered before the mods' event run into a discarded target, so their bar heights are kept and only what mods add is recorded. */
@Mixin(Hud.class)
abstract class HudLayersMixin {
    @Shadow
    private GuiLayerManager layerManager;

    @Inject(method = "initModdedOverlays", at = @At("HEAD"))
    private void horizon$native(CallbackInfo info) {
        var minecraft = net.minecraft.client.Minecraft.getInstance();
        dev.horizonlab.javamods.boot.HudLayers.discarding(((GuiLayersAccessor) layerManager).horizon$layers(),
                () -> new net.minecraft.client.gui.GuiGraphicsExtractor(minecraft, new dev.horizonlab.javamods.boot.Recorder(true), 0, 0));
    }
}
