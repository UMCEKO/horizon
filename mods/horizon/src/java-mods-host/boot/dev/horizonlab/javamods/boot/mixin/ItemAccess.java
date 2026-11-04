package dev.horizonlab.javamods.boot.mixin;

import java.util.UUID;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityReference;
import net.minecraft.world.entity.item.ItemEntity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** Raw fields of `ItemEntity` the group bytes carry. */
@Mixin(ItemEntity.class)
public interface ItemAccess {
    @Accessor("age") void horizon$age(int age);

    @Accessor("pickupDelay") void horizon$pickupDelay(int ticks);

    @Accessor("target") void horizon$target(UUID target);

    @Accessor("thrower") void horizon$thrower(EntityReference<Entity> thrower);
}
