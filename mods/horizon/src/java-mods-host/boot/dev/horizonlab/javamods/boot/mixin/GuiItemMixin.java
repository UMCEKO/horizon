package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.Recorder;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.renderer.state.gui.GuiRenderState;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import org.joml.Matrix3x2fStack;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Items in a screen are drawn by Horizon, which has their models: this stands in where vanilla's own body would ask for a model Java doesn't have. */
@Mixin(GuiGraphicsExtractor.class)
abstract class GuiItemMixin {
    @Shadow
    @Final
    private GuiRenderState guiRenderState;

    @Shadow
    @Final
    private Matrix3x2fStack pose;

    @Inject(method = "item(Lnet/minecraft/world/entity/LivingEntity;Lnet/minecraft/world/level/Level;Lnet/minecraft/world/item/ItemStack;III)V", at = @At(value = "NEW", target = "net/minecraft/client/renderer/item/TrackingItemStackRenderState"), cancellable = true)
    private void horizon$item(LivingEntity owner, Level level, ItemStack stack, int x, int y, int seed, CallbackInfo info) {
        if (guiRenderState instanceof Recorder recorder) {
            info.cancel();
            recorder.item(pose, stack, x, y, Recorder.scissor((GuiGraphicsExtractor) (Object) this));
        }
    }
}
