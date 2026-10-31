package dev.horizonlab.javamods.boot.mixin;

import java.util.HashSet;
import java.util.Set;
import net.minecraft.network.protocol.Packet;
import net.minecraft.server.network.ServerCommonPacketListenerImpl;
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
        if (HORIZON$DROPPED.add(packet.getClass().getName())) System.err.println("Horizon: a Java mod's packet doesn't reach clients yet: " + packet.getClass().getName());
        info.cancel();
    }
}
