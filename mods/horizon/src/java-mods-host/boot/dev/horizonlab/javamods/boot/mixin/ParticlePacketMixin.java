package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientParticles;
import net.minecraft.client.multiplayer.ClientPacketListener;
import net.minecraft.network.protocol.game.ClientboundLevelParticlesPacket;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** A particle packet's particles are the Java server's: native draws none of them. */
@Mixin(ClientPacketListener.class)
abstract class ParticlePacketMixin {
    @Inject(method = "tryAddParticle", at = @At("HEAD"))
    private void horizon$enter(ClientboundLevelParticlesPacket packet, double x, double y, double z, double xa, double ya, double za, CallbackInfoReturnable<Boolean> result) {
        ClientParticles.enter();
    }

    @Inject(method = "tryAddParticle", at = @At("RETURN"))
    private void horizon$exit(ClientboundLevelParticlesPacket packet, double x, double y, double z, double xa, double ya, double za, CallbackInfoReturnable<Boolean> result) {
        ClientParticles.exit();
    }
}
