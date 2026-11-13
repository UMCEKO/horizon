package dev.horizonlab.javamods.testmod.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import dev.horizonlab.javamods.testmod.Checks;
import dev.horizonlab.javamods.testmod.UseState;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.Level;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** A body route: `ItemStack.use` patched at HEAD and around its `Item.use` call, whose body native serves for a vanilla item. */
@Mixin(ItemStack.class)
public abstract class UseStackMixin {
    /** A stick is used up by the mod: an apple appears and the use succeeds, before `Item.use` runs. */
    @Inject(method = "use", at = @At("HEAD"), cancellable = true)
    private void horizon$stick(Level level, Player player, InteractionHand hand, CallbackInfoReturnable<InteractionResult> result) {
        if (!(player instanceof ServerPlayer) || !((ItemStack) (Object) this).is(Items.STICK)) return;
        player.getInventory().add(new ItemStack(Items.APPLE));
        Checks.check("use_stick_head", () -> true);
        result.setReturnValue(InteractionResult.SUCCESS);
    }

    /** Counts the `Item.use` calls of an ender pearl: native's port serves them, once per click. */
    @WrapOperation(method = "use", at = @At(value = "INVOKE", target = "Lnet/minecraft/world/item/Item;use(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/InteractionHand;)Lnet/minecraft/world/InteractionResult;"))
    private InteractionResult horizon$count(Item item, Level level, Player player, InteractionHand hand, Operation<InteractionResult> original) {
        if (player instanceof ServerPlayer && item == Items.ENDER_PEARL) {
            UseState.pearlUses++;
            Checks.check("use_pearl_wrap_counted_once", () -> UseState.pearlUses == 1);
        }
        return original.call(item, level, player, hand);
    }
}
