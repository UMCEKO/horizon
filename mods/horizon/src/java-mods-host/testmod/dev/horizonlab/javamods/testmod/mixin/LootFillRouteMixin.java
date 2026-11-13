package dev.horizonlab.javamods.testmod.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import dev.horizonlab.javamods.testmod.Checks;
import it.unimi.dsi.fastutil.objects.ObjectArrayList;
import net.minecraft.util.RandomSource;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.storage.loot.LootTable;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/** A body route on `LootTable.fill`: it sees the rolled items before they are spread over the container. */
@Mixin(LootTable.class)
public abstract class LootFillRouteMixin {
    @WrapOperation(method = "fill", at = @At(value = "INVOKE", target = "Lnet/minecraft/world/level/storage/loot/LootTable;shuffleAndSplitItems(Lit/unimi/dsi/fastutil/objects/ObjectArrayList;ILnet/minecraft/util/RandomSource;)V"))
    private void horizon$filling(LootTable table, ObjectArrayList<ItemStack> items, int slots, RandomSource random, Operation<Void> original) {
        Checks.check("fill_routed_rolls_items", () -> !items.isEmpty() && slots > 0);
        original.call(table, items, slots, random);
    }
}
