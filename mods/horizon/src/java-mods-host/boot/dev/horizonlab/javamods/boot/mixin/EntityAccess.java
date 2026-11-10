package dev.horizonlab.javamods.boot.mixin;

import com.google.common.collect.ImmutableList;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Mutable;
import org.spongepowered.asm.mixin.gen.Accessor;

/** Raw fields of `Entity` the group bytes carry: no setter, so none of vanilla's logic runs. */
@Mixin(Entity.class)
public interface EntityAccess {
    @Accessor("yRot") void horizon$yRot(float value);

    @Accessor("xRot") void horizon$xRot(float value);

    @Accessor("onGround") void horizon$onGround(boolean value);

    @Accessor("remainingFireTicks") void horizon$fire(int ticks);

    @Accessor("wasTouchingWater") void horizon$inWater(boolean value);

    @Accessor("deltaMovement") void horizon$motion(Vec3 value);

    @Accessor("removalReason") Entity.RemovalReason horizon$removal();

    @Accessor("removalReason") void horizon$removal(Entity.RemovalReason reason);

    @Accessor("vehicle") void horizon$vehicle(Entity vehicle);

    @Accessor("passengers") @Mutable void horizon$passengers(ImmutableList<Entity> passengers);
}
