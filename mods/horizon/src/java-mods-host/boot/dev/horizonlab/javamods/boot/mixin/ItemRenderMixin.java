package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.vertex.PoseStack;
import dev.horizonlab.javamods.boot.BlockRenders;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.item.ItemModelResolver;
import net.minecraft.client.renderer.item.ItemStackRenderState;
import net.minecraft.world.item.ItemDisplayContext;
import net.minecraft.world.entity.ItemOwner;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Items a block entity renderer shows are kept by stack while it is recorded; Rust draws them with its own item models. */
@Mixin(ItemModelResolver.class)
abstract class ItemRenderMixin {
    @Inject(method = "updateForTopItem", at = @At("HEAD"), cancellable = true)
    private void horizon$top(ItemStackRenderState output, ItemStack item, ItemDisplayContext context, Level level, ItemOwner owner, int seed, CallbackInfo info) {
        if (BlockRenders.item(output, item, context, seed)) info.cancel();
    }
}
