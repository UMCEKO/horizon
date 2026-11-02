package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.vertex.PoseStack;
import dev.horizonlab.javamods.boot.BlockRenders;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.item.ItemStackRenderState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** A kept stack isn't empty, and submits as one recorded item. */
@Mixin(ItemStackRenderState.class)
abstract class ItemStateMixin {
    @Inject(method = "isEmpty", at = @At("HEAD"), cancellable = true)
    private void horizon$empty(CallbackInfoReturnable<Boolean> result) {
        if (BlockRenders.hasItem(this)) result.setReturnValue(false);
    }

    @Inject(method = "submit", at = @At("HEAD"), cancellable = true)
    private void horizon$submit(PoseStack poseStack, SubmitNodeCollector collector, int light, int overlay, int outline, CallbackInfo info) {
        if (BlockRenders.submitItem(this, poseStack)) info.cancel();
    }
}
