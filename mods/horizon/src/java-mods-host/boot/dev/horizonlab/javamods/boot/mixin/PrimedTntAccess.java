package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.world.entity.EntityReference;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** Raw fields of `PrimedTnt` the species group carries. */
@Mixin(PrimedTnt.class)
public interface PrimedTntAccess {
    @Accessor("explosionPower") float horizon$power();

    @Accessor("explosionPower") void horizon$power(float power);

    @Accessor("owner") EntityReference<LivingEntity> horizon$owner();

    @Accessor("owner") void horizon$owner(EntityReference<LivingEntity> owner);

    @Accessor("DATA_FUSE_ID") static EntityDataAccessor<Integer> horizon$fuse() {
        throw new AssertionError();
    }

    @Accessor("DATA_BLOCK_STATE_ID") static EntityDataAccessor<BlockState> horizon$state() {
        throw new AssertionError();
    }
}
