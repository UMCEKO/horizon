package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.Animate;
import dev.horizonlab.javamods.boot.ClientParticles;
import net.minecraft.client.particle.Particle;
import net.minecraft.client.particle.ParticleEngine;
import net.minecraft.core.particles.ParticleOptions;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Rendering is native: a particle made here has no sprites and nothing to draw it, so those of Java-run `animateTick` code go to the window and the rest are dropped. */
@Mixin(ParticleEngine.class)
abstract class ParticleEngineMixin {
    @Inject(method = "createParticle", at = @At("HEAD"), cancellable = true)
    private void horizon$particle(ParticleOptions options, double x, double y, double z, double xa, double ya, double za, CallbackInfoReturnable<Particle> result) {
        if (Animate.forwarding()) ClientParticles.add(options, x, y, z, xa, ya, za);
        result.setReturnValue(null);
    }
}
