package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ChatLines;
import net.minecraft.client.gui.components.ChatComponent;
import net.minecraft.client.multiplayer.chat.GuiMessageSource;
import net.minecraft.client.multiplayer.chat.GuiMessageTag;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.MessageSignature;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The Java client's chat is the window's: a line it adds goes to native's chat. */
@Mixin(ChatComponent.class)
abstract class ChatComponentMixin {
    @Inject(method = "addMessage(Lnet/minecraft/network/chat/Component;Lnet/minecraft/network/chat/MessageSignature;Lnet/minecraft/client/multiplayer/chat/GuiMessageSource;Lnet/minecraft/client/multiplayer/chat/GuiMessageTag;)V", at = @At("HEAD"), cancellable = true)
    private void horizon$chat(Component contents, MessageSignature signature, GuiMessageSource source, GuiMessageTag tag, CallbackInfo info) {
        ChatLines.chat(contents, source == GuiMessageSource.SYSTEM_CLIENT);
        info.cancel();
    }
}
