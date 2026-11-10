package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.multiplayer.ClientPacketListener;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The level-loading screen is native: the window shows its own and tells the server, so here the player counts as loaded. */
@Mixin(ClientPacketListener.class)
abstract class LevelLoadingMixin {
    @Shadow
    private void setClientLoaded(boolean loaded) {}

    @Inject(method = "startWaitingForNewLevel*", at = @At("HEAD"), cancellable = true)
    private void horizon$wait(CallbackInfo info) {
        info.cancel();
        setClientLoaded(true);
    }
}
