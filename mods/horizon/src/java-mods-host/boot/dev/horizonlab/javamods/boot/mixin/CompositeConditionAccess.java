package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.core.HolderSet;
import net.minecraft.world.level.storage.loot.predicates.CompositeLootItemCondition;
import net.minecraft.world.level.storage.loot.predicates.LootItemCondition;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The terms of an `all_of` or `any_of` condition, for the craft facts. */
@Mixin(CompositeLootItemCondition.class)
public interface CompositeConditionAccess {
    @Accessor("terms") HolderSet<LootItemCondition> horizon$terms();
}
