package dev.horizonlab.javamods.boot.mixin;

import java.util.Queue;
import net.minecraft.client.particle.Particle;
import net.minecraft.client.particle.ParticleGroup;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(ParticleGroup.class)
public interface ParticleGroupAccess {
    @Accessor("particles")
    Queue<Particle> horizon$particles();
}
