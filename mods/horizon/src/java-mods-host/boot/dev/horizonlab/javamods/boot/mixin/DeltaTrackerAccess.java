package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.DeltaTracker;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(DeltaTracker.Timer.class)
public interface DeltaTrackerAccess {
    @Accessor("deltaTickResidual")
    void horizon$residual(float residual);
}
