package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.world.entity.LivingEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** `LivingEntity`'s private health accessor. */
@Mixin(LivingEntity.class)
public interface LivingDataAccess {
    @Accessor("DATA_HEALTH_ID") static EntityDataAccessor<Float> horizon$health() {
        throw new AssertionError();
    }
}
