package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ChatBox;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.network.protocol.game.ClientboundCommandsPacket;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The Java client's command tree changed: the roots native asks the Java client to parse are asked again. */
@Mixin(ClientPacketListener.class)
abstract class CommandsReceivedMixin {
    @Inject(method = "handleCommands(Lnet/minecraft/network/protocol/game/ClientboundCommandsPacket;)V", at = @At("TAIL"))
    private void horizon$commands(ClientboundCommandsPacket packet, CallbackInfo info) {
        ChatBox.rootsChanged();
    }
}
