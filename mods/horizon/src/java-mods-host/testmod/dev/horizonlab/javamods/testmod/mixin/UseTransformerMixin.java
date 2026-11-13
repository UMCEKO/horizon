package dev.horizonlab.javamods.testmod.mixin;

import com.llamalad7.mixinextras.injector.wrapoperation.Operation;
import com.llamalad7.mixinextras.injector.wrapoperation.WrapOperation;
import dev.horizonlab.javamods.testmod.Checks;
import dev.horizonlab.javamods.testmod.UseState;
import net.minecraft.core.BlockPos;
import net.minecraft.core.component.BlockTransformer;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.feature.stateproviders.BlockStateProvider;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;

/** A body route: `BlockTransformer.transformBlock` counts the transforms a click finds, in Java's body for the item's transformer. */
@Mixin(BlockTransformer.class)
public abstract class UseTransformerMixin {
    @WrapOperation(method = "transformBlock", at = @At(value = "INVOKE", target = "Lnet/minecraft/world/level/levelgen/feature/stateproviders/BlockStateProvider;getOptionalState(Lnet/minecraft/world/level/LevelAccessor;Lnet/minecraft/util/RandomSource;Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/block/state/BlockState;"))
    private BlockState horizon$count(BlockStateProvider provider, LevelAccessor level, RandomSource random, BlockPos pos, Operation<BlockState> original) {
        var state = original.call(provider, level, random, pos);
        if (state != null && !level.isClientSide()) {
            UseState.transforms++;
            Checks.check("use_transform_counted_once", () -> UseState.transforms == 1);
        }
        return state;
    }
}
