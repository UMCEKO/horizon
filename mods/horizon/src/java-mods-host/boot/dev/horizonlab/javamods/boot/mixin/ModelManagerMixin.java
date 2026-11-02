package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientModels;
import net.minecraft.client.renderer.item.ClientItem;
import net.minecraft.client.renderer.item.ItemModel;
import net.minecraft.client.resources.model.ModelManager;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Horizon bakes the models it draws; Java answers only for items only a mod's code can draw. */
@Mixin(ModelManager.class)
abstract class ModelManagerMixin {
    @Inject(method = "getItemModel", at = @At("HEAD"), cancellable = true)
    private void horizon$itemModel(Identifier id, CallbackInfoReturnable<ItemModel> result) {
        result.setReturnValue(ClientModels.itemModel(id));
    }

    @Inject(method = "getItemProperties", at = @At("HEAD"), cancellable = true)
    private void horizon$itemProperties(Identifier id, CallbackInfoReturnable<ClientItem.Properties> result) {
        result.setReturnValue(ClientModels.itemProperties(id));
    }
}
