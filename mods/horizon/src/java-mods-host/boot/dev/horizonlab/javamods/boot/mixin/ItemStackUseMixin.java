package dev.horizonlab.javamods.boot.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import dev.horizonlab.javamods.boot.NativeUse;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/** The `Item.use` call of `ItemStack.use` goes to native's port for a vanilla item of a click native started. */
@Mixin(ItemStack.class)
abstract class ItemStackUseMixin {
    @WrapOperation(method = "use", at = @At(value = "INVOKE", target = "Lnet/minecraft/world/item/Item;use(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/InteractionHand;)Lnet/minecraft/world/InteractionResult;"))
    private InteractionResult horizon$use(Item item, Level level, Player player, InteractionHand hand, Operation<InteractionResult> original) {
        return NativeUse.use(item, player, () -> original.call(item, level, player, hand));
    }
}
