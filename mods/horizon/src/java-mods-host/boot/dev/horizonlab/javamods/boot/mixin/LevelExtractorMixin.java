package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.extract.LevelExtractor;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Rendering is native: Horizon meshes its own chunks, so no section here is dirty. */
@Mixin(LevelExtractor.class)
abstract class LevelExtractorMixin {
    @Inject(method = "setLevel", at = @At("HEAD"), cancellable = true)
    private void horizon$level(ClientLevel level, CallbackInfo info) {
        info.cancel();
    }

    @Inject(method = {"blockChanged", "setBlockDirty", "setSectionDirtyWithNeighbors", "setSectionRangeDirty", "setSectionDirty", "setBlocksDirty"}, at = @At("HEAD"), cancellable = true)
    private void horizon$dirty(CallbackInfo info) {
        info.cancel();
    }
}
