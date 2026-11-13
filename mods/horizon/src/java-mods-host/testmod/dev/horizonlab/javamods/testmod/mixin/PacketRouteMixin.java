package dev.horizonlab.javamods.testmod.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import dev.horizonlab.javamods.testmod.Checks;
import dev.horizonlab.javamods.testmod.RouteState;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.LevelReader;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The pick and use-on packet handlers, patched at their head and inside their bodies, as pick-block and loot mods do. */
@Mixin(ServerGamePacketListenerImpl.class)
public abstract class PacketRouteMixin {
    @Inject(method = "tryPickItem", at = @At("HEAD"), cancellable = true)
    private void horizon$noDiamondPick(ItemStack stack, CallbackInfo ci) {
        if (stack.is(Items.DIAMOND_BLOCK)) ci.cancel();
    }

    @WrapOperation(method = "handlePickItemFromBlock", at = @At(value = "INVOKE", target = "Lnet/minecraft/world/level/block/state/BlockState;getCloneItemStack(Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/LevelReader;ZLnet/minecraft/world/entity/player/Player;)Lnet/minecraft/world/item/ItemStack;"))
    private ItemStack horizon$stoneGivesCobble(BlockState state, BlockPos pos, LevelReader level, boolean data, Player player, Operation<ItemStack> original) {
        if (state.is(Blocks.STONE)) return new ItemStack(Items.COBBLESTONE);
        return original.call(state, pos, level, data, player);
    }

    @WrapOperation(method = "handleUseItemOn", at = @At(value = "INVOKE", target = "Lnet/minecraft/server/level/ServerLevel;mayInteract(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/core/BlockPos;)Z"))
    private boolean horizon$platform(ServerLevel level, Entity entity, BlockPos pos, Operation<Boolean> original) {
        RouteState.mayInteractCalls++;
        Checks.check("may_interact_wrapped", () -> true);
        if (pos.getY() == 200) return true;
        return original.call(level, entity, pos);
    }
}
