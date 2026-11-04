package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.ItemStack;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The item entity's stack accessor. */
@Mixin(ItemEntity.class)
public interface ItemDataAccess {
    @Accessor("DATA_ITEM") static EntityDataAccessor<ItemStack> horizon$item() {
        throw new AssertionError();
    }
}
