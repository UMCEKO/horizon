package dev.horizonlab.javamods.testmod.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import dev.horizonlab.javamods.testmod.Checks;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.ChestBlock;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/** A static body route on `isBlockedChestByBlock`: stone above a chest, which vanilla counts as blocking, doesn't block it. */
@Mixin(ChestBlock.class)
public abstract class ChestBlockedRouteMixin {
    @WrapOperation(method = "isBlockedChestByBlock", at = @At(value = "INVOKE", target = "Lnet/minecraft/world/level/block/state/BlockState;isRedstoneConductor(Lnet/minecraft/world/level/BlockGetter;Lnet/minecraft/core/BlockPos;)Z"))
    private static boolean horizon$stoneDoesntBlock(BlockState above, BlockGetter level, BlockPos pos, Operation<Boolean> original) {
        if (above.is(Blocks.STONE)) {
            Checks.check("stone_doesnt_block_chest", () -> true);
            return false;
        }
        return original.call(above, level, pos);
    }
}
