package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientParticles;
import dev.horizonlab.javamods.boot.ParticleFacts;
import net.minecraft.client.particle.Particle;
import net.minecraft.client.particle.ParticleEngine;
import net.minecraft.core.particles.ParticleOptions;
import net.minecraft.core.particles.ParticleType;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;
import org.spongepowered.asm.mixin.injection.callback.LocalCapture;

/** Mods' particles live here; a vanilla one goes to the window only when Java's own code made it. */
@Mixin(ParticleEngine.class)
abstract class ParticleEngineMixin {
    @Inject(method = "createParticle", at = @At("HEAD"), cancellable = true)
    private void horizon$particle(ParticleOptions options, double x, double y, double z, double xa, double ya, double za, CallbackInfoReturnable<Particle> result) {
        if (ParticleFacts.owned((ParticleType<?>) options.getType())) return;
        if (ClientParticles.sourced()) ClientParticles.add(options, x, y, z, xa, ya, za);
        else ClientParticles.dropped++;
        result.setReturnValue(null);
    }

    @Inject(method = "createParticle", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/particle/ParticleEngine;add(Lnet/minecraft/client/particle/Particle;)V"), locals = LocalCapture.CAPTURE_FAILHARD)
    private void horizon$admit(ParticleOptions options, double x, double y, double z, double xa, double ya, double za, CallbackInfoReturnable<Particle> result, Particle particle) {
        ClientParticles.admitted = particle;
    }

    @Inject(method = "add", at = @At("HEAD"), cancellable = true)
    private void horizon$add(Particle particle, CallbackInfo info) {
        if (particle == ClientParticles.admitted) ClientParticles.admitted = null;
        else if (!ClientParticles.keeps(particle.getClass())) info.cancel();
    }
}
