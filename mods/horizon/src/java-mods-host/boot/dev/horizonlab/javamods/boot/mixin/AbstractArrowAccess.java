package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.world.entity.projectile.arrow.AbstractArrow;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The raw field of `AbstractArrow` the species group carries. */
@Mixin(AbstractArrow.class)
public interface AbstractArrowAccess {
    @Accessor("baseDamage") void horizon$baseDamage(double damage);
}
