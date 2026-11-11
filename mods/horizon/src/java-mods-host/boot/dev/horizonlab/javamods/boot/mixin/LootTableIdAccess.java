package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.resources.Identifier;
import net.neoforged.neoforge.common.loot.LootTableIdCondition;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

/** The table a `neoforge:loot_table_id` condition names, for the craft facts. */
@Mixin(LootTableIdCondition.class)
public interface LootTableIdAccess {
    @Accessor("targetLootTableId") Identifier horizon$target();
}
