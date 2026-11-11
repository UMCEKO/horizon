package dev.horizonlab.javamods.boot.mixin;

import java.util.Map;
import net.minecraft.client.particle.ParticleEngine;
import net.minecraft.client.particle.ParticleGroup;
import net.minecraft.client.particle.ParticleRenderType;
import net.minecraft.client.particle.ParticleResources;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(ParticleEngine.class)
public interface ParticleEngineAccess {
    @Accessor("resourceManager")
    ParticleResources horizon$resources();

    @Accessor("particles")
    Map<ParticleRenderType, ParticleGroup<?>> horizon$groups();
}
