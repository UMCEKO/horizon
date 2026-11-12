package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ChatLines;
import net.minecraft.client.gui.Hud;
import net.minecraft.network.chat.Component;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The Java client's action bar is the window's: a message it sets goes to native's. */
@Mixin(Hud.class)
abstract class HudOverlayMixin {
    @Inject(method = "setOverlayMessage(Lnet/minecraft/network/chat/Component;Z)V", at = @At("HEAD"), cancellable = true)
    private void horizon$overlay(Component text, boolean animate, CallbackInfo info) {
        ChatLines.overlay(text, animate);
        info.cancel();
    }
}
