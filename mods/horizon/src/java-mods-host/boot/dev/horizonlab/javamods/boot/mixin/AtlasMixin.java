package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.Sprites;
import net.minecraft.client.renderer.texture.TextureAtlas;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Atlases are Horizon's: each sprite Java asks for is its own file. */
@Mixin(TextureAtlas.class)
abstract class AtlasMixin {
    @Shadow
    @Final
    private Identifier location;

    @Inject(method = "getSprite", at = @At("HEAD"), cancellable = true)
    private void horizon$sprite(Identifier id, CallbackInfoReturnable<TextureAtlasSprite> result) {
        result.setReturnValue(Sprites.sprite(location, id));
    }

    @Inject(method = "missingSprite", at = @At("HEAD"), cancellable = true)
    private void horizon$missing(CallbackInfoReturnable<TextureAtlasSprite> result) {
        result.setReturnValue(Sprites.missing(location));
    }
}
