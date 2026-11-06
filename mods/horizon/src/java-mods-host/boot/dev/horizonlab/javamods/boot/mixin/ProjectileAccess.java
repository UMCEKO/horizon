package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityReference;
import net.minecraft.world.entity.projectile.Projectile;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The raw field of `Projectile` the species group carries. */
@Mixin(Projectile.class)
public interface ProjectileAccess {
    @Accessor("owner") void horizon$owner(EntityReference<Entity> owner);
}
