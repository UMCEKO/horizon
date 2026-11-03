package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.gui.Hud;
import net.neoforged.neoforge.client.gui.GuiLayerManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Vanilla's HUD layers are native: the layers registered before the mods' event draw nothing here, so only what mods add is recorded. */
@Mixin(Hud.class)
abstract class HudLayersMixin {
    @Shadow
    private GuiLayerManager layerManager;

    @Inject(method = "initModdedOverlays", at = @At("HEAD"))
    private void horizon$native(CallbackInfo info) {
        ((GuiLayersAccessor) layerManager).horizon$layers().replaceAll(layer -> new GuiLayerManager.NamedLayer(layer.name(), (graphics, delta) -> {}));
    }
}
