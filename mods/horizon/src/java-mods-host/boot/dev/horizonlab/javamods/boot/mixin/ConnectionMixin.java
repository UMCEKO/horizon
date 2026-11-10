package dev.horizonlab.javamods.boot.mixin;

import java.util.HashSet;
import java.util.Set;
import net.minecraft.network.protocol.Packet;
import dev.horizonlab.javamods.boot.Clients;
import net.minecraft.server.network.ServerCommonPacketListenerImpl;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(ServerCommonPacketListenerImpl.class)
abstract class ConnectionMixin {
    @Unique
    private static final Set<String> HORIZON$DROPPED = new HashSet<>();

    @Inject(method = "send(Lnet/minecraft/network/protocol/Packet;)V", at = @At("HEAD"), cancellable = true)
    private void horizon$send(Packet<?> packet, CallbackInfo info) {
        info.cancel();
        if ((Object) this instanceof ServerGamePacketListenerImpl game && Clients.toClient(game.player.getUUID(), packet)) return;
        String name = packet instanceof net.minecraft.network.protocol.common.ClientboundCustomPayloadPacket custom ? custom.payload().type().id().toString() : packet.getClass().getName();
        if (HORIZON$DROPPED.add(name)) System.err.println("Horizon: a Java mod's packet doesn't reach clients yet: " + name);
    }
}
