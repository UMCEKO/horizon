package dev.horizonlab.javamods.testmod.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import dev.horizonlab.javamods.testmod.Checks;
import dev.horizonlab.javamods.testmod.RouteState;
import net.minecraft.core.NonNullList;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/** A body route: with slot 9 selected the server's inventory reports slot 0 as full, inside the vanilla body that runs in Java. */
@Mixin(Inventory.class)
public abstract class InventoryFreeSlotMixin {
    @WrapOperation(method = "getFreeSlot", at = @At(value = "INVOKE", target = "Lnet/minecraft/core/NonNullList;get(I)Ljava/lang/Object;"))
    private Object horizon$slotZeroIsFull(NonNullList<ItemStack> items, int index, Operation<Object> original) {
        var inventory = (Inventory) (Object) this;
        if (index == 0 && RouteState.server(inventory) && inventory.getSelectedSlot() == 8) {
            Checks.check("route_body_free_slot_java", () -> true);
            return new ItemStack(Items.STONE);
        }
        return original.call(items, index);
    }
}
