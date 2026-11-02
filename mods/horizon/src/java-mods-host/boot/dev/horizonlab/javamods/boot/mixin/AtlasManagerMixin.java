package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.Sprites;
import net.minecraft.client.renderer.texture.TextureAtlas;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.client.resources.model.sprite.AtlasManager;
import net.minecraft.client.resources.model.sprite.SpriteId;
import net.minecraft.resources.Identifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** No atlas is stitched in Java; each is a view that names its sprites' files. */
@Mixin(AtlasManager.class)
abstract class AtlasManagerMixin {
    @Inject(method = "getAtlasOrThrow", at = @At("HEAD"), cancellable = true)
    private void horizon$atlas(Identifier id, CallbackInfoReturnable<TextureAtlas> result) {
        result.setReturnValue(Sprites.atlas(id));
    }

    @Inject(method = "get(Lnet/minecraft/client/resources/model/sprite/SpriteId;)Lnet/minecraft/client/renderer/texture/TextureAtlasSprite;", at = @At("HEAD"), cancellable = true)
    private void horizon$sprite(SpriteId sprite, CallbackInfoReturnable<TextureAtlasSprite> result) {
        result.setReturnValue(Sprites.sprite(sprite.atlasLocation(), sprite.texture()));
    }
}
