package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.Sprites;
import net.minecraft.client.renderer.texture.AbstractTexture;
import net.minecraft.client.renderer.texture.TextureManager;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Textures are Horizon's: Java's stand for their files by name. */
@Mixin(TextureManager.class)
abstract class TextureManagerMixin {
    @Inject(method = "getTexture(Lnet/minecraft/resources/Identifier;)Lnet/minecraft/client/renderer/texture/AbstractTexture;", at = @At("HEAD"), cancellable = true)
    private void horizon$texture(Identifier id, CallbackInfoReturnable<AbstractTexture> result) {
        result.setReturnValue(Sprites.texture(id));
    }
}
