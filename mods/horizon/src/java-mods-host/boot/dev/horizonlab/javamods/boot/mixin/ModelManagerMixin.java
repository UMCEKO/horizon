package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.renderer.item.ItemModel;
import net.minecraft.client.resources.model.ModelManager;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Horizon bakes the models it draws; Java has none to answer with. */
@Mixin(ModelManager.class)
abstract class ModelManagerMixin {
    @Inject(method = "getItemModel", at = @At("HEAD"), cancellable = true)
    private void horizon$itemModel(Identifier id, CallbackInfoReturnable<ItemModel> result) {
        result.setReturnValue(null);
    }
}
