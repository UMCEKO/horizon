package dev.horizonlab.javamods.boot.mixin;

import java.util.Optional;
import net.minecraft.core.Holder;
import net.minecraft.world.level.storage.loot.predicates.LootItemCondition;
import net.neoforged.neoforge.common.loot.LootModifier;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The condition a loot modifier tests, for the craft facts. */
@Mixin(LootModifier.class)
public interface LootModifierAccess {
    @Accessor("condition") Optional<Holder<LootItemCondition>> horizon$condition();
}
