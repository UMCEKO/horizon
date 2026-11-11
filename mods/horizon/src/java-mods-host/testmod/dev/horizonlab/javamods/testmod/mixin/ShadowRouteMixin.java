package dev.horizonlab.javamods.testmod.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import dev.horizonlab.javamods.testmod.ClientGlyphs;
import net.minecraft.client.gui.Font;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;

/** A text builder whose shadow colour a mod replaces for its own font, as a theme mod does. */
@Mixin(targets = "net.minecraft.client.gui.Font$PreparedTextBuilder")
public abstract class ShadowRouteMixin {
    @Shadow
    @Final
    Font this$0;

    @WrapOperation(method = "getShadowColor", at = @At(value = "INVOKE", target = "Lnet/minecraft/util/ARGB;scaleRGB(IF)I"))
    private int horizon$greenShadow(int color, float scale, Operation<Integer> original) {
        return this$0 instanceof ClientGlyphs.ShadowFont ? 0xFF00FF00 : original.call(color, scale);
    }
}
