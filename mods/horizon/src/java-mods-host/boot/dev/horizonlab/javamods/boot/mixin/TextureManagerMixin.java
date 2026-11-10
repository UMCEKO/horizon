package dev.horizonlab.javamods.boot.mixin;

import com.mojang.blaze3d.platform.NativeImage;
import dev.horizonlab.javamods.boot.Sprites;
import java.util.Map;
import java.util.function.Supplier;
import net.minecraft.client.renderer.texture.AbstractTexture;
import net.minecraft.client.renderer.texture.DynamicTexture;
import net.minecraft.client.renderer.texture.TextureManager;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The atlases the reload stitched are registered here; any other texture stands for its file, as Horizon draws it. */
@Mixin(TextureManager.class)
abstract class TextureManagerMixin {
    @Shadow
    @Final
    private Map<Identifier, AbstractTexture> byPath;

    /** The missing texture would be made on the GPU; Horizon has its own, and lookups never reach this one. */
    @Redirect(method = "<init>", at = @At(value = "NEW", target = "net/minecraft/client/renderer/texture/DynamicTexture"))
    private DynamicTexture horizon$missing(Supplier<String> label, NativeImage image) {
        image.close();
        return Sprites.unallocated(DynamicTexture.class);
    }

    @Inject(method = "getTexture(Lnet/minecraft/resources/Identifier;)Lnet/minecraft/client/renderer/texture/AbstractTexture;", at = @At("HEAD"), cancellable = true)
    private void horizon$texture(Identifier id, CallbackInfoReturnable<AbstractTexture> result) {
        AbstractTexture registered = byPath.get(id);
        result.setReturnValue(registered != null ? registered : Sprites.texture(id));
    }
}
