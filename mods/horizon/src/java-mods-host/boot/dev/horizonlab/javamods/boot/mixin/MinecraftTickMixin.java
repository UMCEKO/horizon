package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientBridge;
import dev.horizonlab.javamods.boot.Lockstep;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(Minecraft.class)
abstract class MinecraftTickMixin {
    /** Horizon's window handles the keys and acts on the world; in a lockstep tick it does so here, while this tick waits. */
    @Inject(method = "handleKeybinds", at = @At("HEAD"), cancellable = true)
    private void horizon$keybinds(CallbackInfo info) {
        if (Lockstep.active()) Lockstep.park(Lockstep.Point.K, new byte[0]);
        info.cancel();
    }

    /** The window's title is Horizon's. */
    @Inject(method = "updateTitle", at = @At("HEAD"), cancellable = true)
    private void horizon$title(CallbackInfo info) {
        info.cancel();
    }

    /** The window's own keys (screenshot, fullscreen) are native. */
    @Inject(method = "handleGlobalKeyPress", at = @At("HEAD"), cancellable = true)
    private void horizon$globalKeys(com.mojang.blaze3d.platform.InputConstants.Key key, boolean control, org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(false);
    }

    /** Rendering is native: `setScreenAndShow` draws no frame here. */
    @Inject(method = "renderFrame", at = @At("HEAD"), cancellable = true)
    private void horizon$frame(boolean advance, CallbackInfo info) {
        info.cancel();
    }

    /** The pause screen is native: the window opens its own and says so. */
    @Inject(method = "pauseGame", at = @At("HEAD"), cancellable = true)
    private void horizon$pause(boolean suppressMenu, CallbackInfo info) {
        info.cancel();
    }

    @Inject(method = "setLevel", at = @At("HEAD"))
    private void horizon$level(ClientLevel level, CallbackInfo info) {
        ClientBridge.levelChanged();
    }
}
