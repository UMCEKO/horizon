package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientBridge;
import net.minecraft.client.multiplayer.ClientCommonPacketListenerImpl;
import net.minecraft.network.protocol.Packet;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** What the Java client sends goes to its player's server view in the same JVM. */
@Mixin(ClientCommonPacketListenerImpl.class)
abstract class ClientSendMixin {
    @Inject(method = "send(Lnet/minecraft/network/protocol/Packet;)V", at = @At("HEAD"), cancellable = true)
    private void horizon$send(Packet<?> packet, CallbackInfo info) {
        info.cancel();
        ClientBridge.send(packet);
    }
}
